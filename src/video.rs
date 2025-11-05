use anyhow::Context;
use av::software::scaling::{context::Context as Scaler, flag::Flags};
use av::util::frame::video::Video as VideoFrame;
use ffmpeg_next as av;
use parking_lot::{Condvar, Mutex};
use std::mem::MaybeUninit;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use crate::ffmpeg::{DECODER_WAKEUP, DECODER_WAKEUP_MUTEX, FFMPEG_END, VIDEO_TIME_BASE};
use crate::statistics::increment_video_skipped_frames;
use crate::term;
use crate::term::{RenderWrapper, TERM_DEFAULT_BG, TERM_DEFAULT_FG};
use crate::term::{TERM_QUIT, VIDEO_PIXELS};
use crate::util::{Cell, Color};
use crate::{PAUSE, audio};

pub static VIDEO_FRAMETIME: AtomicU64 = AtomicU64::new(1_000_000 / 30);

pub static VIDEO_FRAME: Mutex<Option<VideoFrame>> = Mutex::new(None);
pub static VIDEO_FRAME_SIG: Condvar = Condvar::new();

pub fn video_main() {
    let mut scaler = MaybeUninit::uninit();

    let mut scaler_format = None;
    let mut scaler_width = None;
    let mut scaler_height = None;

    while TERM_QUIT.load(Ordering::SeqCst) == false {
        let frame = {
            let mut lock = VIDEO_FRAME.lock();
            while lock.is_none() && TERM_QUIT.load(Ordering::SeqCst) == false {
                if FFMPEG_END.load(Ordering::SeqCst) {
                    break;
                }
                VIDEO_FRAME_SIG.wait_for(&mut lock, Duration::from_millis(100));
            }
            if lock.is_none() {
                break;
            }
            lock.take().unwrap()
        };
        *DECODER_WAKEUP_MUTEX.lock() = true;
        DECODER_WAKEUP.notify_one();

        let frametime = {
            let pts = frame.pts().unwrap();
            let base = VIDEO_TIME_BASE.lock().unwrap();
            Duration::new(
                pts as u64 * base.0 as u64 / base.1 as u64,
                (pts as u64 * base.0 as u64 % base.1 as u64 * 1_000_000_000 / base.1 as u64) as u32,
            )
        };

        if frametime + Duration::from_millis(100) < audio::played_time_or_zero() {
            send_debug!(
                "Video frame too late: frame time {:?}, audio time {:?}",
                frametime,
                audio::played_time_or_zero()
            );
            increment_video_skipped_frames();
            send_error!("Video frame too late, skipping");
            continue;
        }

        let term_size_changed = term::updatesize(frame.width() as usize, frame.height() as usize);

        if Some(frame.format()) != scaler_format
            || Some(frame.width()) != scaler_width
            || Some(frame.height()) != scaler_height
            || term_size_changed
        {
            scaler = MaybeUninit::new(
                Scaler::get(
                    frame.format(),
                    frame.width(),
                    frame.height(),
                    av::format::Pixel::RGBA,
                    VIDEO_PIXELS.x() as u32,
                    VIDEO_PIXELS.y() as u32,
                    Flags::BILINEAR,
                )
                .context("Could not create scaler")
                .unwrap(),
            );
            scaler_format = Some(frame.format());
            scaler_width = Some(frame.width());
            scaler_height = Some(frame.height());
        }

        let mut scaled = VideoFrame::empty();
        unsafe { scaler.assume_init_mut() }
            .run(&frame, &mut scaled)
            .expect("scaler run failed");

        let bytes = scaled.data(0);
        let colors: &[Color] = unsafe {
            std::slice::from_raw_parts(
                bytes.as_ptr() as *const Color,
                bytes.len() / std::mem::size_of::<Color>(),
            )
        };

        let mut render_start = Instant::now();
        term::render(colors, scaled.stride(0) / std::mem::size_of::<Color>());

        while frametime > audio::played_time_or_zero() + Duration::from_millis(5) {
            if PAUSE.load(Ordering::SeqCst) {
                let remaining = Duration::from_millis(33).saturating_sub(render_start.elapsed());
                std::thread::sleep(remaining);
                render_start = Instant::now();
                term::render(colors, scaled.stride(0) / std::mem::size_of::<Color>());
                if TERM_QUIT.load(Ordering::SeqCst) {
                    return;
                }
            } else {
                let remaining = frametime - audio::played_time_or_zero();
                let max = Duration::from_micros(VIDEO_FRAMETIME.load(Ordering::SeqCst) * 2);
                std::thread::sleep(remaining.min(max));
                break;
            }
        }
    }
}

pub fn render_frame(wrap: &mut RenderWrapper) {
    for cy in 0..wrap.padding_top {
        for cx in 0..wrap.frame_width {
            wrap.cells[cy * wrap.cells_pitch + cx] = Cell {
                c: Some(' '),
                fg: TERM_DEFAULT_FG,
                bg: TERM_DEFAULT_BG,
            };
        }
    }
    for cy in wrap.padding_top..(wrap.cells_height - wrap.padding_bottom) {
        for cx in 0..wrap.padding_left {
            wrap.cells[cy * wrap.cells_pitch + cx] = Cell {
                c: Some(' '),
                fg: TERM_DEFAULT_FG,
                bg: TERM_DEFAULT_BG,
            };
        }
        for cx in wrap.padding_left..(wrap.cells_width - wrap.padding_right) {
            let fy = cy - wrap.padding_top;
            let fx = cx - wrap.padding_left;
            wrap.cells[cy * wrap.cells_pitch + cx] = Cell {
                c: None,
                fg: wrap.frame[fy * wrap.frame_pitch * 2 + fx + wrap.frame_pitch],
                bg: wrap.frame[fy * wrap.frame_pitch * 2 + fx],
            };
        }
        for cx in (wrap.cells_width - wrap.padding_right)..wrap.cells_width {
            wrap.cells[cy * wrap.cells_pitch + cx] = Cell {
                c: Some(' '),
                fg: TERM_DEFAULT_FG,
                bg: TERM_DEFAULT_BG,
            };
        }
    }
    for cy in (wrap.cells_height - wrap.padding_bottom)..wrap.cells_height {
        for cx in 0..wrap.frame_width {
            wrap.cells[cy * wrap.cells_pitch + cx] = Cell {
                c: Some(' '),
                fg: TERM_DEFAULT_FG,
                bg: TERM_DEFAULT_BG,
            };
        }
    }
}
