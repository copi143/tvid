use anyhow::{Context, Result};
use ffmpeg_next as av;
use std::{
    env::args,
    panic,
    sync::{
        LazyLock,
        atomic::{AtomicBool, Ordering},
    },
};
use tokio::runtime::Runtime;

use crate::{playlist::PLAYLIST, stdin::Key, term::TERM_QUIT};

#[macro_use]
pub mod error;

#[macro_use]
pub mod ui;

pub mod audio;
pub mod config;
pub mod ffmpeg;
pub mod osc;
pub mod playlist;
pub mod sixel;
pub mod stdin;
pub mod stdout;
pub mod subtitle;
pub mod term;
pub mod util;
pub mod video;

pub static TOKIO_RUNTIME: LazyLock<Runtime> = LazyLock::new(|| {
    let num_cores = std::thread::available_parallelism().unwrap().get();
    tokio::runtime::Builder::new_multi_thread()
        .worker_threads(num_cores)
        .enable_all()
        .build()
        .expect("Failed to create Tokio runtime")
});

pub static PAUSE: AtomicBool = AtomicBool::new(false);

fn register_keypress_callbacks() {
    stdin::register_keypress_callback(Key::Normal(' '), |_| {
        PAUSE.fetch_xor(true, Ordering::SeqCst);
        true
    });
    stdin::register_keypress_callback(Key::Normal('q'), |_| {
        term::request_quit();
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

    playlist::register_keypress_callbacks();
    ui::register_keypress_callbacks();
}

fn main() -> Result<()> {
    panic::set_hook(Box::new(|info| {
        let msg = if let Some(s) = info.payload().downcast_ref::<&str>() {
            *s
        } else if let Some(s) = info.payload().downcast_ref::<String>() {
            s.as_str()
        } else {
            "Unknown panic"
        };
        let location = info
            .location()
            .map(|l| format!("{}:{}", l.file(), l.line()))
            .unwrap_or_default();
        send_error!("[panic] {} at {}", msg, location);
        term::quit();
    }));

    config::create_if_not_exists(None)?;
    config::load(None)?;

    if args().len() < 2 && PLAYLIST.lock().len() == 0 {
        let divider = "-".repeat(term::get_winsize().map(|w| w.col as usize).unwrap_or(80));
        eprintln!("No input files.");
        eprintln!("Usage: {} <input> [input] ...", args().nth(0).unwrap());
        eprintln!("{}", divider);
        eprintln!("tvid - Terminal Video Player");
        eprintln!("version: {}", env!("CARGO_PKG_VERSION"));
        eprintln!("repo: {}", env!("CARGO_PKG_REPOSITORY"));
        eprintln!("license: {}", env!("CARGO_PKG_LICENSE"));
        std::process::exit(1);
    }

    if args().len() > 1 {
        PLAYLIST
            .lock()
            .clear()
            .extend(args().skip(1).collect::<Vec<_>>());
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

    register_keypress_callbacks();

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
