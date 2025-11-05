use anyhow::{Context, Result};
use av::Subtitle;
use av::codec::context::Context as AVCCtx;
use av::ffi::{AV_TIME_BASE, AVSEEK_FLAG_BACKWARD, av_read_frame, av_seek_frame};
use av::format::context::Input;
use av::packet::Mut as _;
use av::util::frame::{Audio as AudioFrame, video::Video as VideoFrame};
use ffmpeg_next::{self as av, Packet};
use parking_lot::{Condvar, Mutex};
use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use crate::audio::{
    AUDIO_FRAME, AUDIO_FRAME_SIG, AUDIO_HINT_SEEKED, audio_main, played_time_or_zero,
};
use crate::subtitle;
use crate::term::TERM_QUIT;
use crate::video::{VIDEO_FRAME, VIDEO_FRAME_SIG, VIDEO_FRAMETIME, video_main};

pub static VIDEO_TIME_BASE: Mutex<Option<av::Rational>> = Mutex::new(None);
pub static AUDIO_TIME_BASE: Mutex<Option<av::Rational>> = Mutex::new(None);

pub static DECODER_WAKEUP_MUTEX: Mutex<bool> = Mutex::new(false);
pub static DECODER_WAKEUP: Condvar = Condvar::new();

pub static FFMPEG_END: AtomicBool = AtomicBool::new(false);

pub static VIDEO_DURATION: Mutex<Duration> = Mutex::new(Duration::ZERO);

pub fn playback_progress() -> f64 {
    let playback_time = played_time_or_zero().as_secs_f64();
    let video_duration = VIDEO_DURATION.lock().as_secs_f64();
    playback_time / video_duration
}

/// (是否为绝对寻址, 偏移量)
static SEEK_REQUEST: Mutex<Option<(bool, f64)>> = Mutex::new(None);

pub fn seek_request_relative(sec: f64) {
    let mut lock = SEEK_REQUEST.lock();
    lock.replace((false, sec));
    *DECODER_WAKEUP_MUTEX.lock() = true;
    DECODER_WAKEUP.notify_one();
}

pub fn seek_request_absolute(sec: f64) {
    let mut lock = SEEK_REQUEST.lock();
    lock.replace((true, sec));
    *DECODER_WAKEUP_MUTEX.lock() = true;
    DECODER_WAKEUP.notify_one();
}

pub fn decode_main(path: &str) -> Result<()> {
    let mut ictx = av::format::input(path).context("could not open input file")?;

    let video_stream_index = ictx
        .streams()
        .best(av::media::Type::Video)
        .map_or(-1, |s| s.index() as isize);
    let audio_stream_index = ictx
        .streams()
        .best(av::media::Type::Audio)
        .map_or(-1, |s| s.index() as isize);
    let subtitle_stream_index = ictx
        .streams()
        .best(av::media::Type::Subtitle)
        .map_or(-1, |s| s.index() as isize);

    if TERM_QUIT.load(Ordering::SeqCst) != false {
        return Ok(());
    }

    let (mut video_decoder, video_timebase, video_rate) = if video_stream_index >= 0 {
        let stream = ictx
            .stream(video_stream_index as usize)
            .context("video stream")?;
        let codec_ctx = AVCCtx::from_parameters(stream.parameters()).context("video decoder")?;
        let codec = codec_ctx.decoder().video().context("video decoder")?;
        (
            Some(codec),
            Some(stream.time_base()),
            Some(stream.avg_frame_rate()),
        )
    } else {
        (None, None, None)
    };

    let (mut audio_decoder, audio_timebase, audio_rate) = if audio_stream_index >= 0 {
        let stream = ictx
            .stream(audio_stream_index as usize)
            .context("audio stream")?;
        let codec_ctx = AVCCtx::from_parameters(stream.parameters()).context("audio decoder")?;
        let codec = codec_ctx.decoder().audio().context("audio decoder")?;
        (
            Some(codec),
            Some(stream.time_base()),
            Some(stream.avg_frame_rate()),
        )
    } else {
        (None, None, None)
    };

    let (mut subtitle_decoder, subtitle_timebase) = if subtitle_stream_index >= 0 {
        let stream = ictx
            .stream(subtitle_stream_index as usize)
            .context("subtitle stream")?;
        let codec_ctx = AVCCtx::from_parameters(stream.parameters()).context("subtitle decoder")?;
        let codec = codec_ctx.decoder().subtitle().context("subtitle decoder")?;
        (Some(codec), Some(stream.time_base()))
    } else {
        (None, None)
    };

    if let (Some(video_timebase), Some(video_rate)) = (video_timebase, video_rate) {
        VIDEO_TIME_BASE.lock().replace(video_timebase);
        VIDEO_FRAMETIME.store(
            video_rate.1 as u64 * 1_000_000 / video_rate.0 as u64,
            Ordering::SeqCst,
        );
    }

    if let Some(audio_timebase) = audio_timebase {
        AUDIO_TIME_BASE.lock().replace(audio_timebase);
    }

    if TERM_QUIT.load(Ordering::SeqCst) != false {
        return Ok(());
    }

    FFMPEG_END.store(false, Ordering::SeqCst);

    *VIDEO_DURATION.lock() = Duration::new(
        ictx.duration() as u64 / AV_TIME_BASE as u64,
        (ictx.duration() as u64 % AV_TIME_BASE as u64 * 1_000_000_000 / AV_TIME_BASE as u64) as u32,
    );

    let video_main = std::thread::spawn(video_main);
    let audio_main = std::thread::spawn(audio_main);

    let mut video_queue = VecDeque::new();
    let mut audio_queue = VecDeque::new();

    while !(TERM_QUIT.load(Ordering::SeqCst) || FFMPEG_END.load(Ordering::SeqCst)) {
        if let Some((abs, off)) = SEEK_REQUEST.lock().take() {
            if do_seek(&mut ictx, abs, off, &mut video_queue, &mut audio_queue) {
                continue;
            } else {
                break;
            }
        }

        let packet = {
            let mut packet = Packet::empty();
            if unsafe { av_read_frame(ictx.as_mut_ptr(), packet.as_mut_ptr()) } < 0 {
                break;
            }
            packet
        };

        if let Some((abs, off)) = SEEK_REQUEST.lock().take() {
            if do_seek(&mut ictx, abs, off, &mut video_queue, &mut audio_queue) {
                continue;
            } else {
                break;
            }
        }

        if TERM_QUIT.load(Ordering::SeqCst) || FFMPEG_END.load(Ordering::SeqCst) {
            break;
        }

        if packet.stream() as isize == video_stream_index {
            video_queue.push_back(packet);
        } else if packet.stream() as isize == audio_stream_index {
            audio_queue.push_back(packet);
        } else if packet.stream() as isize == subtitle_stream_index {
            let subtitle_decoder = subtitle_decoder.as_mut().unwrap();
            let mut subtitle = Subtitle::new();
            if subtitle_decoder.decode(&packet, &mut subtitle).is_ok() {
                let timebase = packet.time_base();
                let pts = Duration::new(
                    subtitle.pts().unwrap_or(0) as u64 * timebase.0 as u64 / timebase.1 as u64,
                    (subtitle.pts().unwrap_or(0) as u64 * timebase.0 as u64 % timebase.1 as u64
                        * 1_000_000_000
                        / timebase.1 as u64) as u32,
                );
                let start = pts + Duration::from_millis(subtitle.start() as u64);
                let end = pts + Duration::from_millis(subtitle.end() as u64);
                subtitle::push_nothing();
                for rect in subtitle.rects() {
                    match rect {
                        av::subtitle::Rect::None(_) => {}
                        av::subtitle::Rect::Bitmap(sub) => {
                            let _x = sub.x();
                            let _y = sub.y();
                            let _width = sub.width();
                            let _height = sub.height();
                            send_warn!("bitmap subtitle not supported");
                        }
                        av::subtitle::Rect::Text(sub) => {
                            subtitle::push_text(start, end, sub.get());
                        }
                        av::subtitle::Rect::Ass(sub) => {
                            subtitle::push_ass(start, end, sub.get());
                        }
                    }
                }
            }
        }

        while audio_queue.len() > 0 && video_queue.len() > 0 {
            if TERM_QUIT.load(Ordering::SeqCst) || FFMPEG_END.load(Ordering::SeqCst) {
                break;
            }

            if SEEK_REQUEST.lock().is_some() {
                break;
            }

            decode_video(&mut video_decoder, &mut video_queue);
            decode_audio(&mut audio_decoder, &mut audio_queue);

            let mut lock = DECODER_WAKEUP_MUTEX.lock();
            if *lock == false {
                DECODER_WAKEUP.wait_for(&mut lock, Duration::from_millis(50));
            }
            *lock = false;
        }
    }

    notify_quit();

    // 等待所有线程结束
    video_main.join().unwrap_or_else(|err| {
        send_error!("video thread join error: {:?}", err);
    });
    audio_main.join().unwrap_or_else(|err| {
        send_error!("audio thread join error: {:?}", err);
    });

    // 清除还没处理的音频和视频帧
    let _ = VIDEO_FRAME.lock().take();
    let _ = AUDIO_FRAME.lock().take();
    // 清除字幕
    subtitle::clear();

    Ok(())
}

fn do_seek(
    ictx: &mut Input,
    abs: bool,
    off: f64,
    video_queue: &mut VecDeque<ffmpeg_next::Packet>,
    audio_queue: &mut VecDeque<ffmpeg_next::Packet>,
) -> bool {
    let now = || played_time_or_zero().as_secs_f64();
    let ts = (if abs { off } else { now() + off } * AV_TIME_BASE as f64) as i64;
    let ret = unsafe { av_seek_frame(ictx.as_mut_ptr(), -1, ts, AVSEEK_FLAG_BACKWARD) };

    // 清除还没处理的音频和视频包
    video_queue.clear();
    audio_queue.clear();
    // 清除字幕 (实际上不应该清除，但是我的处理逻辑有点问题，先这样吧)
    subtitle::clear();

    // 清除还没处理的音频和视频帧
    let _ = VIDEO_FRAME.lock().take();
    let _ = AUDIO_FRAME.lock().take();

    AUDIO_HINT_SEEKED.store(true, Ordering::SeqCst);

    ret >= 0
}

fn decode_video(
    video_decoder: &mut Option<ffmpeg_next::decoder::Video>,
    video_queue: &mut VecDeque<ffmpeg_next::Packet>,
) {
    while video_queue.len() > 0 && VIDEO_FRAME.lock().is_none() {
        let Some(video_decoder) = video_decoder.as_mut() else {
            panic!("video_queue is not empty, so video_decoder must exist");
        };
        let Some(packet) = video_queue.pop_front() else {
            panic!("video_queue is not empty, so packet must exist");
        };
        if let Err(e) = video_decoder.send_packet(&packet) {
            eprintln!("video send_packet err: {:?}", e);
            return;
        }
        let pts = packet.pts();
        drop(packet);
        let mut frame = VideoFrame::empty();
        while video_decoder.receive_frame(&mut frame).is_ok() {
            if frame.pts().is_none() {
                frame.set_pts(pts);
            }
            let mut lock = VIDEO_FRAME.lock();
            assert!(lock.is_none(), "video frame queue should be empty");
            lock.replace(std::mem::replace(&mut frame, VideoFrame::empty()));
            VIDEO_FRAME_SIG.notify_one();
        }
    }
}

fn decode_audio(
    audio_decoder: &mut Option<ffmpeg_next::decoder::Audio>,
    audio_queue: &mut VecDeque<ffmpeg_next::Packet>,
) {
    while audio_queue.len() > 0 && AUDIO_FRAME.lock().is_none() {
        let Some(audio_decoder) = audio_decoder.as_mut() else {
            panic!("audio_queue is not empty, so audio_decoder must exist");
        };
        let Some(packet) = audio_queue.pop_front() else {
            panic!("audio_queue is not empty, so packet must exist");
        };
        if let Err(e) = audio_decoder.send_packet(&packet) {
            eprintln!("audio send_packet err: {:?}", e);
            return;
        }
        let pts = packet.pts();
        drop(packet);
        let mut frame = AudioFrame::empty();
        while audio_decoder.receive_frame(&mut frame).is_ok() {
            if frame.pts().is_none() {
                frame.set_pts(pts);
            }
            let mut lock = AUDIO_FRAME.lock();
            assert!(lock.is_none(), "audio frame queue should be empty");
            lock.replace(std::mem::replace(&mut frame, AudioFrame::empty()));
            AUDIO_FRAME_SIG.notify_one();
        }
    }
}

/// 通知所有解码相关的线程退出
pub fn notify_quit() {
    // 标记 ffmpeg 处理结束，以便音频和视频线程可以退出
    FFMPEG_END.store(true, Ordering::SeqCst);

    // 唤醒所有等待的线程
    DECODER_WAKEUP.notify_one();
    VIDEO_FRAME_SIG.notify_one();
    AUDIO_FRAME_SIG.notify_one();
}
