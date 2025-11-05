#![allow(clippy::collapsible_if)]
#![allow(clippy::bool_comparison)]
#![allow(clippy::too_many_arguments)]
#![allow(clippy::manual_range_contains)]
#![allow(clippy::iter_nth_zero)]
#![allow(clippy::len_zero)]
#![allow(clippy::new_without_default)]
#![allow(clippy::len_without_is_empty)]
#![allow(clippy::partialeq_to_none)]
#![allow(clippy::should_implement_trait)]

use anyhow::{Context, Result};
use ffmpeg_next as av;
use std::env::args;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{LazyLock, OnceLock};
use std::time::Instant;
use tokio::runtime::Runtime;

use crate::ffmpeg::seek_request_relative;
use crate::term::{COLOR_MODE, FORCEFLUSH_NEXT};
use crate::ui::QUIT_CONFIRMATION;
use crate::{playlist::PLAYLIST, stdin::Key, term::TERM_QUIT};

#[macro_use]
mod logging;

#[macro_use]
mod ui;

mod audio;
mod config;
mod ffmpeg;
mod osc;
mod playlist;
mod sixel;
mod statistics;
mod stdin;
mod stdout;
mod subtitle;
mod term;
mod util;
mod video;

pub static TOKIO_RUNTIME: LazyLock<Runtime> = LazyLock::new(|| {
    let num_cores = std::thread::available_parallelism().unwrap().get();
    tokio::runtime::Builder::new_multi_thread()
        .worker_threads(num_cores)
        .enable_all()
        .build()
        .expect("Failed to create Tokio runtime")
});

static PAUSE: AtomicBool = AtomicBool::new(false);

macro_rules! eprintlns {
    ($($fmt:expr $(, $args:expr)*);+ $(;)?) => {
        $(
            eprintln!($fmt $(, $args)*);
        )+
    };
}

fn print_no_playlist() {
    let divider = "-".repeat(term::get_winsize().map(|w| w.col as usize).unwrap_or(80));
    eprintlns!(
        "No input files.";
        "Usage: {} <input> [input] ...", args().nth(0).unwrap();
        "{}",  divider;
        "tvid - Terminal Video Player";
        "version: {}", env!("CARGO_PKG_VERSION");
        "repo: {}", env!("CARGO_PKG_REPOSITORY");
        "license: {}", env!("CARGO_PKG_LICENSE");
    );
}

fn print_help() {
    eprintlns!(
        "tvid - Terminal Video Player";
        "version: {}", env!("CARGO_PKG_VERSION");
        "repo: {}", env!("CARGO_PKG_REPOSITORY");
        "license: {}", env!("CARGO_PKG_LICENSE");
        "";
        "Usage: {} <input> [input] ...", args().nth(0).unwrap();
        "";
        "Controls:";
        "  Space         : Play/Pause";
        "  q             : Quit";
        "  n             : Next video in playlist";
        "  l             : Toggle playlist display";
        "  f             : Open file selector";
        "  c             : Cycle color mode";
    );
}

fn register_input_callbacks() {
    stdin::register_keypress_callback(Key::Normal(' '), |_| {
        PAUSE.fetch_xor(true, Ordering::SeqCst);
        true
    });
    stdin::register_keypress_callback(Key::Normal('q'), |_| {
        QUIT_CONFIRMATION.store(true, Ordering::SeqCst);
        true
    });
    stdin::register_keypress_callback(Key::Normal('n'), |_| {
        ffmpeg::notify_quit();
        true
    });
    stdin::register_keypress_callback(Key::Normal('l'), |_| {
        playlist::toggle_show_playlist();
        true
    });
    stdin::register_keypress_callback(Key::Normal('m'), |_| true);
    stdin::register_keypress_callback(Key::Normal('f'), |_| {
        ui::FILE_SELECT.fetch_xor(true, Ordering::SeqCst);
        true
    });

    stdin::register_keypress_callback(Key::Normal('c'), |_| {
        COLOR_MODE.lock().switch_next();
        FORCEFLUSH_NEXT.store(true, Ordering::SeqCst);
        true
    });

    stdin::register_keypress_callback(Key::Up, |_| {
        seek_request_relative(-30.0);
        true
    });
    stdin::register_keypress_callback(Key::Down, |_| {
        seek_request_relative(30.0);
        true
    });
    stdin::register_keypress_callback(Key::Left, |_| {
        seek_request_relative(-5.0);
        true
    });
    stdin::register_keypress_callback(Key::Right, |_| {
        seek_request_relative(5.0);
        true
    });

    playlist::register_keypress_callbacks();
    ui::register_input_callbacks();
}

static APP_START_TIME: OnceLock<Instant> = OnceLock::new();

fn main() -> Result<()> {
    APP_START_TIME.set(Instant::now()).unwrap();

    config::create_if_not_exists(None)?;
    config::load(None)?;

    if args().len() == 2 {
        let arg1 = args().nth(1).unwrap();
        if arg1 == "-h" || arg1 == "--help" {
            print_help();
            std::process::exit(0);
        }
    }

    if args().len() > 1 {
        PLAYLIST
            .lock()
            .clear()
            .extend(args().skip(1).collect::<Vec<_>>());
    }

    if PLAYLIST.lock().len() == 0 {
        print_no_playlist();
        std::process::exit(1);
    }

    // stdout::print(b"\x1bPq");
    // stdout::print(b"#0;2;0;0;0#1;2;100;100;0#2;2;0;100;0");
    // stdout::print(b"#1~~@@vv@@~~@@~~$");
    // stdout::print(b"#2??}}GG}}??}}??-");
    // stdout::print(b"#1!14@-");
    // stdout::print(b"#0;2;0;0;0#1;2;100;100;100#2;2;0;0;100");
    // stdout::print(b"#1~~@@vv@@~~@@~~$");
    // stdout::print(b"#2??}}GG}}??}}??-");
    // stdout::print(b"#1!14@-");
    // stdout::print(b"\x1b\\");

    // stdout::print(b"\x1bPq#0;2;100;100;100#1;2;0;100;0#1~\x1b\\");

    av::init().context("av init failed")?;

    term::init();
    term::setup_panic_handler(); // 一定要在初始化之后设置，且必须立刻设置

    register_input_callbacks();

    term::add_render_callback(video::render_frame);
    term::add_render_callback(subtitle::render_subtitle);
    term::add_render_callback(ui::render_ui);

    let input_main = std::thread::spawn(stdin::input_main);
    let output_main = std::thread::spawn(stdout::output_main);

    while let Some(path) = { PLAYLIST.lock().next().cloned() } {
        ffmpeg::decode_main(&path).unwrap_or_else(|err| {
            send_error!("ffmpeg decode error: {}", err);
        });
        if TERM_QUIT.load(std::sync::atomic::Ordering::SeqCst) {
            break;
        }
    }

    term::request_quit();

    output_main.join().unwrap_or_else(|err| {
        send_error!("output thread join error: {:?}", err);
    });
    input_main.join().unwrap_or_else(|err| {
        send_error!("input thread join error: {:?}", err);
    });

    config::save(None).unwrap_or_else(|err| {
        send_error!("config save error: {}", err);
    });

    term::quit();
}
