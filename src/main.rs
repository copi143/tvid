#![feature(sync_unsafe_cell)]

use anyhow::{Context, Result};
use ffmpeg_next as av;
use std::{
    panic,
    sync::{LazyLock, Mutex},
};
use tokio::runtime::Runtime;

use crate::{playlist::PLAYLIST, term::TERM_QUIT};

#[macro_use]
pub mod error;

pub mod audio;
pub mod ffmpeg;
pub mod playlist;
pub mod stdin;
pub mod stdout;
pub mod subtitle;
pub mod term;
pub mod ui;
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

pub static CURRENT_PLAYING: Mutex<String> = Mutex::new(String::new());

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

    if std::env::args().len() < 2 {
        eprintln!(
            "Usage: {} <input> [input] ...",
            std::env::args().nth(0).unwrap()
        );
        std::process::exit(1);
    }

    PLAYLIST
        .lock()
        .unwrap()
        .extend(std::env::args().skip(1).collect::<Vec<_>>());

    av::init().context("av init failed")?;

    term::init();

    stdin::register_keypress_callback(b'q', |_| term::request_quit());
    stdin::register_keypress_callback(b'n', |_| ffmpeg::notify_quit());

    term::add_render_callback(video::render_frame);
    term::add_render_callback(subtitle::render_subtitle);
    term::add_render_callback(ui::render_ui);

    let input_main = std::thread::spawn(stdin::input_main);
    let output_main = std::thread::spawn(stdout::output_main);

    while let Some(path) = PLAYLIST.lock().unwrap().next() {
        CURRENT_PLAYING.lock().unwrap().clone_from(&path);
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

    term::quit();
}
