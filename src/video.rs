use av::software::scaling::{context::Context as Scaler, flag::Flags};
use av::util::frame::video::Video as VideoFrame;
use parking_lot::{Condvar, Mutex};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Duration;

use crate::avsync::{self, played_time_or_zero};
use crate::ffmpeg::{DECODER_WAKEUP, DECODER_WAKEUP_MUTEX, VIDEO_TIME_BASE};
use crate::render;
use crate::statistics::increment_video_skipped_frames;
use crate::term::TERM_QUIT;

pub static VIDEO_FRAMETIME: AtomicU64 = AtomicU64::new(1_000_000 / 30);

/// 当前待渲染的视频帧
///
/// ASSUME 永远只能在解码器处插入视频帧
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
            error_l10n!("Video frame has zero width or height, skipping");
            continue;
        }

        let frametime = if let Some(pts) = frame.pts() {
            let base = VIDEO_TIME_BASE.lock().unwrap();
            Duration::new(
                pts as u64 * base.0 as u64 / base.1 as u64,
                (pts as u64 * base.0 as u64 % base.1 as u64 * 1_000_000_000 / base.1 as u64) as u32,
            )
        } else {
            Duration::from_millis(0)
        };

        // 为了防止视频卡死，seek 永远播放一帧旧的画面
        let seeked = HINT_SEEKED.swap(false, Ordering::SeqCst);
        let played = played_time_or_zero();
        if !seeked && frametime + Duration::from_millis(100) < played {
            debug_f16n!(
                "Video frame too late: frame time {:?}, played time {:?}",
                frametime,
                played
            );
            increment_video_skipped_frames(0, 1);
            error_l10n!("Video frame too late, skipping");
            continue;
        }

        {
            let mut ctx = render::RENDER_CONTEXT.lock();
            ctx.update_size(Some(frame.width() as usize), Some(frame.height() as usize));
        }

        loop {
            let ctx = render::RENDER_CONTEXT.lock();
            let ss = frame.width() != scaler_src_width || frame.height() != scaler_src_height;
            let ts = ctx.frame_width != scaler_dst_width || ctx.frame_height != scaler_dst_height;
            if ss || ts || Some(frame.format()) != scaler_format {
                let Ok(sws) = Scaler::get(
                    frame.format(),
                    frame.width(),
                    frame.height(),
                    av::format::Pixel::RGBA,
                    ctx.frame_width as u32,
                    ctx.frame_height as u32,
                    Flags::BILINEAR,
                ) else {
                    error_l10n!("Could not create scaler for video frame");
                    break;
                };
                scaler = Some(sws);
                scaler_format = Some(frame.format());
                scaler_src_width = frame.width();
                scaler_src_height = frame.height();
                scaler_dst_width = ctx.frame_width;
                scaler_dst_height = ctx.frame_height;
            }
            drop(ctx);

            let scaler = scaler.as_mut().unwrap();

            let mut scaled = VideoFrame::empty();
            if let Err(e) = scaler.run(&frame, &mut scaled) {
                error_f16n!("Could not scale video frame: {}", e);
                break;
            }

            // 使用 if 防止卡死
            if !avsync::is_paused() && frametime > played_time_or_zero() + Duration::from_millis(5)
            {
                let remaining = frametime - played_time_or_zero();
                let max = Duration::from_micros(VIDEO_FRAMETIME.load(Ordering::SeqCst) * 2);
                if render::api_wait_frame_request_for(remaining.min(max)) {
                    let ctx = render::RENDER_CONTEXT.lock();
                    if ctx.frame_width != scaler_dst_width || ctx.frame_height != scaler_dst_height
                    {
                        continue;
                    }
                }
            }

            render::api_send_frame(scaled);
            avsync::hint_video_played_time(frametime);

            // 使用 if 防止卡死
            if !avsync::is_paused() && frametime > played_time_or_zero() + Duration::from_millis(5)
            {
                let remaining = frametime - played_time_or_zero();
                let max = Duration::from_micros(VIDEO_FRAMETIME.load(Ordering::SeqCst) * 2);
                if render::api_wait_frame_request_for(remaining.min(max)) {
                    continue;
                }
            }

            let mut wakeup_by_request = false;
            while avsync::is_paused() {
                if render::api_wait_frame_request_for(Duration::from_millis(33)) {
                    wakeup_by_request = true;
                    break;
                }
                if TERM_QUIT.load(Ordering::SeqCst) {
                    let mut ctx = render::RENDER_CONTEXT.lock();
                    ctx.update_size(Some(0), Some(0));
                    return;
                }
            }
            if wakeup_by_request {
                continue;
            }

            break;
        }
    }

    let mut ctx = render::RENDER_CONTEXT.lock();
    ctx.update_size(Some(0), Some(0));
}
