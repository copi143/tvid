#![expect(clippy::collapsible_if)]
#![expect(clippy::bool_comparison)]
#![expect(clippy::too_many_arguments)]
#![expect(clippy::manual_range_contains)]
#![expect(clippy::iter_nth_zero)]
#![expect(clippy::len_zero)]
#![expect(clippy::partialeq_to_none)]
#![expect(clippy::option_map_unit_fn)]
#![deny(unused_must_use)]

#[cfg(feature = "i18n")]
static_l10n::main!();

macro_rules! usemod {
    ($name:ident) => {
        mod $name;
        pub use $name::*;
    };
}

use anyhow::{Context, Result};
use clap::Parser;
use data_classes::{ToNext as _, ToPrev as _};
use parking_lot::Mutex;
use std::env;
use std::sync::atomic::Ordering;
use std::sync::{LazyLock, OnceLock};
use std::time::Instant;
use tokio::runtime::Runtime;

use crate::escape::format_link;
use crate::ffmpeg::seek_request_relative;
use crate::ui::QUIT_CONFIRMATION;
use crate::{playlist::PLAYLIST, stdin::Key, term::TERM_QUIT};

#[macro_use]
#[allow(unused_macros)]
mod logging;

#[allow(unused)]
#[deny(unused_must_use)]
mod util;

#[macro_use]
#[allow(unused_macros)]
mod ui;

#[allow(unused)]
#[deny(unused_must_use)]
mod avsync;

mod playlist;
mod render;
mod statistics;
mod stdin;
mod stdout;
mod term;

#[cfg(feature = "command")]
mod command;

/// TODO
#[allow(unused)]
#[cfg(feature = "ssh")]
mod ssh;

/// TODO
#[allow(unused)]
#[cfg(feature = "config")]
mod config;

#[allow(unused_imports)]
#[cfg(feature = "ffmpeg")]
mod ffmpeg;

#[cfg(feature = "audio")]
mod audio;

#[cfg(feature = "video")]
mod video;

#[cfg(feature = "subtitle")]
mod subtitle;

#[allow(unused)]
#[deny(unused_must_use)]
mod escape {
    #[cfg(feature = "sixel")]
    usemod!(sixel);
    usemod!(osc8);
    #[cfg(feature = "osc1337")]
    usemod!(osc1337);
}

pub static TOKIO_RUNTIME: LazyLock<Runtime> = LazyLock::new(|| {
    let num_cores = std::thread::available_parallelism().unwrap().get();
    let workers = num_cores.max(4); // 防止 I/O 卡顿
    tokio::runtime::Builder::new_multi_thread()
        .worker_threads(workers)
        .enable_all()
        .build()
        .expect(l10n!("Failed to create Tokio runtime"))
});

macro_rules! eprintlns {
    ($($fmt:expr $(, $args:expr)*);+ $(;)?) => {{
        $(
            eprintln!($fmt $(, $args)*);
        )+
    }};
}

fn print_no_playlist(program_name: &str) {
    let divider = "-".repeat(term::get_winsize().map(|w| w.col as usize).unwrap_or(80));
    let version = env!("CARGO_PKG_VERSION");
    let repo = format_link(env!("CARGO_PKG_REPOSITORY"), env!("CARGO_PKG_REPOSITORY"));
    #[rustfmt::skip]
    let license = env!("CARGO_PKG_LICENSE")
        .replace("MIT", &format_link("MIT", "https://choosealicense.com/licenses/mit/"))
        .replace("Apache-2.0", &format_link("Apache-2.0", "https://choosealicense.com/licenses/apache-2.0/"));
    eprintlns!(
        "{}", l10n!("No input files as playlist.");
        "{}", f16n!("Usage: {} <input> [input] ...", program_name);
        "{}", divider;
        "{}", l10n!("tvid - Terminal Video Player");
        "{}", f16n!("version: {}", version);
        "{}", f16n!("repo: {}", repo);
        "{}", f16n!("license: {}", license);
    );
}

const TVID_LOGO_BASE64: &str = include_str!("tvid.min.svg.base64");

fn print_help(program_name: &str) {
    let version = env!("CARGO_PKG_VERSION");
    let repo = format_link(env!("CARGO_PKG_REPOSITORY"), env!("CARGO_PKG_REPOSITORY"));
    #[rustfmt::skip]
    let license = env!("CARGO_PKG_LICENSE")
        .replace("MIT", &format_link("MIT", "https://choosealicense.com/licenses/mit/"))
        .replace("Apache-2.0", &format_link("Apache-2.0", "https://choosealicense.com/licenses/apache-2.0/"));
    eprintln!(
        "\x1b]1337;File=inline=1;size={}:{}\x1b\\",
        TVID_LOGO_BASE64.len(),
        TVID_LOGO_BASE64,
    );
    eprintlns!(
        "{}", l10n!("tvid - Terminal Video Player");
        "{}", f16n!("version: {}", version);
        "{}", f16n!("repo: {}", repo);
        "{}", f16n!("license: {}", license);
        "";
        "{}", f16n!("Usage: {} <input> [input] ...", program_name);
        "";
        "{}", l10n!("Controls:");
        "{}", l10n!("  Space         : Play/Pause");
        "{}", l10n!("  q             : Quit");
        "{}", l10n!("  n             : Next video in playlist");
        "{}", l10n!("  l             : Toggle playlist display");
        "{}", l10n!("  f             : Open file selector");
        "{}", l10n!("  c             : Cycle color mode");
    );
}

#[derive(Parser, Debug)]
#[command(
    name = "tvid",
    about = "Terminal Video Player",
    version = env!("CARGO_PKG_VERSION"),
    disable_help_flag = true,
    disable_help_subcommand = true,
)]
struct CliArgs {
    /// Print help and exit
    #[arg(short = 'h', long = "help")]
    show_help: bool,

    /// Input files (playlist)
    #[arg(value_name = "INPUT")]
    inputs: Vec<String>,

    /// Small seek step in seconds
    #[arg(long = "seek-small", default_value_t = 5.0)]
    seek_small: f64,

    /// Large seek step in seconds
    #[arg(long = "seek-large", default_value_t = 30.0)]
    seek_large: f64,

    #[arg(short = 'l', long = "loop")]
    loop_playlist: bool,

    #[arg(short = 'p', long = "playlist")]
    playlist: Option<String>,
}

static SEEK_SMALL_STEP: Mutex<f64> = Mutex::new(5.0);
static SEEK_LARGE_STEP: Mutex<f64> = Mutex::new(30.0);

fn register_input_callbacks() {
    stdin::register_keypress_callback(Key::Escape, |_, _| {
        info_l10n!("Press 'q' to quit.");
        true
    });
    stdin::register_keypress_callback(Key::Normal(' '), |_, _| {
        avsync::switch_pause_state();
        true
    });
    stdin::register_keypress_callback(Key::Normal('q'), |id, _| {
        if id == 0 {
            QUIT_CONFIRMATION.store(true, Ordering::SeqCst);
        }
        true
    });
    stdin::register_keypress_callback(Key::Normal('n'), |_, _| {
        ffmpeg::notify_quit();
        true
    });
    stdin::register_keypress_callback(Key::Normal('l'), |_, _| {
        playlist::toggle_show_playlist();
        true
    });
    stdin::register_keypress_callback(Key::Normal('m'), |_, _| true);
    stdin::register_keypress_callback(Key::Normal('f'), |_, _| {
        ui::FILE_SELECT.fetch_xor(true, Ordering::SeqCst);
        true
    });
    #[cfg(feature = "audio")]
    stdin::register_keypress_callback(Key::Normal('w'), |_, _| {
        render::toggle_show_audio_visualizer();
        true
    });

    stdin::register_keypress_callback(Key::Lower('c'), |_, _| {
        let mut ctx = render::RENDER_CONTEXT.lock();
        ctx.color_mode.switch_to_next();
        let (fppc_x, fppc_y) = ctx.color_mode.fppc();
        ctx.update_fppc(fppc_x, fppc_y);
        ctx.force_flush_next();
        true
    });

    stdin::register_keypress_callback(Key::Upper('c'), |_, _| {
        let mut ctx = render::RENDER_CONTEXT.lock();
        ctx.color_mode.switch_to_prev();
        let (fppc_x, fppc_y) = ctx.color_mode.fppc();
        ctx.update_fppc(fppc_x, fppc_y);
        ctx.force_flush_next();
        true
    });

    stdin::register_keypress_callback(Key::Up, |_, _| {
        seek_request_relative(-*SEEK_LARGE_STEP.lock());
        true
    });
    stdin::register_keypress_callback(Key::Down, |_, _| {
        seek_request_relative(*SEEK_LARGE_STEP.lock());
        true
    });
    stdin::register_keypress_callback(Key::Left, |_, _| {
        seek_request_relative(-*SEEK_SMALL_STEP.lock());
        true
    });
    stdin::register_keypress_callback(Key::Right, |_, _| {
        seek_request_relative(*SEEK_SMALL_STEP.lock());
        true
    });

    playlist::register_keypress_callbacks();
    ui::register_input_callbacks();
    #[cfg(feature = "command")]
    command::register_input_callbacks();
}

static APP_START_TIME: OnceLock<Instant> = OnceLock::new();

fn main() -> Result<()> {
    APP_START_TIME.set(Instant::now()).unwrap();

    #[cfg(feature = "i18n")]
    match sys_locale::get_locale()
        .map(|l| l.to_lowercase().replace('_', "-"))
        .unwrap_or("en-us".to_string())
        .as_str()
    {
        "zh-cn" => static_l10n::lang!("zh-cn"),
        "zh-tw" => static_l10n::lang!("zh-tw"),
        "ja-jp" => static_l10n::lang!("ja-jp"),
        "fr-fr" => static_l10n::lang!("fr-fr"),
        "de-de" => static_l10n::lang!("de-de"),
        "es-es" => static_l10n::lang!("es-es"),
        "ko-kr" => static_l10n::lang!("ko-kr"),
        "pt-br" => static_l10n::lang!("pt-br"),
        "ru-ru" => static_l10n::lang!("ru-ru"),
        "it-it" => static_l10n::lang!("it-it"),
        "tr-tr" => static_l10n::lang!("tr-tr"),
        "vi-vn" => static_l10n::lang!("vi-vn"),
        _ => static_l10n::lang!("en-us"),
    }

    let program_name = env::args().nth(0).unwrap_or_else(|| {
        eprintln!("{}", l10n!("Got 0 args? What the fuck?"));
        std::process::exit(1);
    });

    let cli = CliArgs::parse();
    *SEEK_SMALL_STEP.lock() = cli.seek_small;
    *SEEK_LARGE_STEP.lock() = cli.seek_large;

    #[cfg(feature = "config")]
    {
        config::create_if_not_exists(None)?;
        config::load(None)?;
    }

    if cli.show_help {
        print_help(&program_name);
        std::process::exit(0);
    }

    if !cli.inputs.is_empty() {
        PLAYLIST.lock().clear().extend(cli.inputs.clone());
    }

    // if let Some(playlist_path) = cli.playlist {
    //     PLAYLIST.lock().load_from_file(&playlist_path)?;
    // }

    if PLAYLIST.lock().len() == 0 {
        print_no_playlist(&program_name);
        std::process::exit(1);
    }

    av::init().context(l10n!("av init failed"))?;

    term::init();

    #[cfg(feature = "ssh")]
    ssh::run()?;

    ffmpeg::init();

    register_input_callbacks();

    render::add_render_callback(render::render_video);
    #[cfg(feature = "subtitle")]
    render::add_render_callback(subtitle::render_subtitle);
    render::add_render_callback(ui::render_ui);

    #[cfg(feature = "command")]
    command::register_commands();

    let input_main = TOKIO_RUNTIME.spawn(stdin::input_main());
    let output_main = TOKIO_RUNTIME.spawn(stdout::output_main());
    let render_main = std::thread::spawn(render::render_main);

    let mut continuous_failure_count = 0;
    while let Some(path) = { PLAYLIST.lock().next().cloned() } {
        let success = ffmpeg::decode_main(&path).unwrap_or_else(|err| {
            error_f16n!("ffmpeg decode error: {}", err);
            false
        });
        if success {
            continuous_failure_count = 0;
        } else {
            continuous_failure_count += 1;
        }
        if TERM_QUIT.load(Ordering::SeqCst) {
            break;
        }
        if continuous_failure_count >= PLAYLIST.lock().len() {
            error_l10n!("Too many continuous failures, exiting.");
            break;
        }
    }

    term::request_quit();

    render_main.join().unwrap_or_else(|err| {
        error_f16n!("render thread join error: {:?}", err);
    });
    TOKIO_RUNTIME.block_on(async {
        output_main.await.unwrap_or_else(|err| {
            error_f16n!("output task join error: {:?}", err);
        });
        input_main.await.unwrap_or_else(|err| {
            error_f16n!("input task join error: {:?}", err);
        });
    });

    #[cfg(feature = "config")]
    config::save(None).unwrap_or_else(|err| {
        error_f16n!("config save error: {}", err);
    });

    term::quit();
}
