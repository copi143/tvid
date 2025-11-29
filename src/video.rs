use av::software::scaling::{context::Context as Scaler, flag::Flags};
use av::util::frame::video::Video as VideoFrame;
use ffmpeg_next as av;
use parking_lot::{Condvar, Mutex};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Duration;

use crate::avsync::{self, played_time_or_zero};
use crate::ffmpeg::{DECODER_WAKEUP, DECODER_WAKEUP_MUTEX, VIDEO_TIME_BASE};
use crate::render::{self, RenderWrapper, VIDEO_PIXELS};
use crate::statistics::increment_video_skipped_frames;
use crate::term::TERM_QUIT;
use crate::util::{Cell, Color};

pub static VIDEO_FRAMETIME: AtomicU64 = AtomicU64::new(1_000_000 / 30);

pub static VIDEO_FRAME: Mutex<Option<VideoFrame>> = Mutex::new(None);
pub static VIDEO_FRAME_SIG: Condvar = Condvar::new();

static HINT_SEEKED: AtomicBool = AtomicBool::new(false);

/// 提示视频模块已经 seek 到指定时间点
pub fn hint_seeked() {
    HINT_SEEKED.store(true, Ordering::SeqCst);
}

pub fn video_main() {
    let mut scaler = None;
    let mut scaler_format = None;
    let mut scaler_src_width = 0;
    let mut scaler_src_height = 0;
    let mut scaler_dst_width = 0;
    let mut scaler_dst_height = 0;

    while TERM_QUIT.load(Ordering::SeqCst) == false {
        let frame = {
            let mut lock = VIDEO_FRAME.lock();
            while lock.is_none() && TERM_QUIT.load(Ordering::SeqCst) == false {
                if avsync::decode_ended() {
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

        if frame.width() == 0 || frame.height() == 0 {
            send_error!("Video frame has zero width or height, skipping");
            continue;
        }

        let frametime = {
            let pts = frame.pts().unwrap();
            let base = VIDEO_TIME_BASE.lock().unwrap();
            Duration::new(
                pts as u64 * base.0 as u64 / base.1 as u64,
                (pts as u64 * base.0 as u64 % base.1 as u64 * 1_000_000_000 / base.1 as u64) as u32,
            )
        };

        // 为了防止视频卡死，seek 永远播放一帧旧的画面
        let seeked = HINT_SEEKED.swap(false, Ordering::SeqCst);
        let played = played_time_or_zero();
        if !seeked && frametime + Duration::from_millis(100) < played {
            send_debug!("Video frame too late: frame time {frametime:?}, played time {played:?}");
            increment_video_skipped_frames();
            send_error!("Video frame too late, skipping");
            continue;
        }

        render::VIDEO_SIZE_CACHE.set(frame.width() as usize, frame.height() as usize);

        loop {
            let ss = frame.width() != scaler_src_width || frame.height() != scaler_src_height;
            let ts = VIDEO_PIXELS.get() != (scaler_dst_width as usize, scaler_dst_height as usize);
            if ss || ts || Some(frame.format()) != scaler_format {
                let Ok(sws) = Scaler::get(
                    frame.format(),
                    frame.width(),
                    frame.height(),
                    av::format::Pixel::RGBA,
                    VIDEO_PIXELS.x() as u32,
                    VIDEO_PIXELS.y() as u32,
                    Flags::BILINEAR,
                ) else {
                    send_error!("Could not create scaler for video frame");
                    break;
                };
                scaler = Some(sws);
                scaler_format = Some(frame.format());
                scaler_src_width = frame.width();
                scaler_src_height = frame.height();
                scaler_dst_width = VIDEO_PIXELS.x() as u32;
                scaler_dst_height = VIDEO_PIXELS.y() as u32;
            }

            let scaler = scaler.as_mut().unwrap();

            let mut scaled = VideoFrame::empty();
            scaler.run(&frame, &mut scaled).expect("scaler run failed");

            // 使用 if 防止卡死
            if !avsync::is_paused() && frametime > played_time_or_zero() + Duration::from_millis(5)
            {
                let remaining = frametime - played_time_or_zero();
                let max = Duration::from_micros(VIDEO_FRAMETIME.load(Ordering::SeqCst) * 2);
                if render::wait_frame_request_for(remaining.min(max)) {
                    if VIDEO_PIXELS.get() != (scaler_dst_width as usize, scaler_dst_height as usize)
                    {
                        continue;
                    }
                }
            }

            render::send_frame(scaled);
            avsync::hint_video_played_time(frametime);

            // 使用 if 防止卡死
            if !avsync::is_paused() && frametime > played_time_or_zero() + Duration::from_millis(5)
            {
                let remaining = frametime - played_time_or_zero();
                let max = Duration::from_micros(VIDEO_FRAMETIME.load(Ordering::SeqCst) * 2);
                if render::wait_frame_request_for(remaining.min(max)) {
                    continue;
                }
            }

            let mut wakeup_by_request = false;
            while avsync::is_paused() {
                if render::wait_frame_request_for(Duration::from_millis(33)) {
                    wakeup_by_request = true;
                    break;
                }
                if TERM_QUIT.load(Ordering::SeqCst) {
                    render::VIDEO_SIZE_CACHE.set(0, 0);
                    return;
                }
            }
            if wakeup_by_request {
                continue;
            }

            break;
        }
    }

    render::VIDEO_SIZE_CACHE.set(0, 0);
}

/// 绿幕背景色
pub static CHROMA_KEY_COLOR: Mutex<Option<Color>> = Mutex::new(None);

pub fn render_frame(wrap: &mut RenderWrapper) {
    if let Some(chroma_key) = *CHROMA_KEY_COLOR.lock() {
        for cy in wrap.padding_top..(wrap.cells_height - wrap.padding_bottom) {
            for cx in wrap.padding_left..(wrap.cells_width - wrap.padding_right) {
                let fy = cy - wrap.padding_top;
                let fx = cx - wrap.padding_left;
                let fg = wrap.frame[fy * wrap.frame_pitch * 2 + fx + wrap.frame_pitch];
                let bg = wrap.frame[fy * wrap.frame_pitch * 2 + fx];
                let fs = fg.similar_to(&chroma_key, 0.1);
                let bs = bg.similar_to(&chroma_key, 0.1);
                wrap.cells[cy * wrap.cells_pitch + cx] = match (fs, bs) {
                    (true, true) => Cell {
                        c: Some(' '),
                        fg: Color::transparent(),
                        bg: Color::transparent(),
                    },
                    (true, false) => Cell {
                        c: None,
                        fg: bg,
                        bg,
                    },
                    (false, true) => Cell {
                        c: None,
                        fg,
                        bg: fg,
                    },
                    (false, false) => Cell { c: None, fg, bg },
                };
            }
        }
    } else {
        for cy in wrap.padding_top..(wrap.cells_height - wrap.padding_bottom) {
            for cx in wrap.padding_left..(wrap.cells_width - wrap.padding_right) {
                let fy = cy - wrap.padding_top;
                let fx = cx - wrap.padding_left;
                let fg = wrap.frame[fy * wrap.frame_pitch * 2 + fx + wrap.frame_pitch];
                let bg = wrap.frame[fy * wrap.frame_pitch * 2 + fx];
                wrap.cells[cy * wrap.cells_pitch + cx] = Cell { c: None, fg, bg };
            }
        }
    }
}
