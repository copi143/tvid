use anyhow::{Context, Result};
use av::codec::context::Context as AVCCtx;
use av::ffi::{AV_TIME_BASE, av_read_frame, av_seek_frame};
use av::format::context::Input;
use av::packet::Mut as _;
use av::util::frame::{Audio as AudioFrame, video::Video as VideoFrame};
use av::{Packet, Subtitle};
use ffmpeg_next as av;
use parking_lot::{Condvar, Mutex};
use std::collections::VecDeque;
use std::sync::atomic::Ordering;
use std::time::Duration;

use crate::audio::{self, AUDIO_FRAME, AUDIO_FRAME_SIG, audio_main};
use crate::avsync::played_time_or_zero;
use crate::term::TERM_QUIT;
use crate::video::{VIDEO_FRAME, VIDEO_FRAME_SIG, VIDEO_FRAMETIME, video_main};
use crate::{avsync, subtitle, video};

#[allow(static_mut_refs)]
#[allow(unsafe_op_in_unsafe_fn)]
unsafe extern "C" fn ffmpeg_log_callback(
    arg1: *mut libc::c_void,
    arg2: libc::c_int,
    arg3: *const libc::c_char,
    arg4: *mut ffmpeg_sys_next::__va_list_tag,
) {
    if arg2 > ffmpeg_sys_next::AV_LOG_WARNING {
        return;
    }
    static mut PRINT_PREFIX: core::ffi::c_int = 0;
    static mut BUF: [u8; 1024] = [0u8; 1024];
    ffmpeg_sys_next::av_log_format_line(
        arg1,
        arg2,
        arg3,
        arg4,
        BUF.as_mut_ptr() as *mut core::ffi::c_char,
        BUF.len() as core::ffi::c_int,
        &mut PRINT_PREFIX as *mut core::ffi::c_int,
    );
    let c_str = std::ffi::CStr::from_ptr(BUF.as_ptr() as *const core::ffi::c_char);
    if let Ok(str_slice) = c_str.to_str() {
        let str_slice = str_slice.trim_end();
        match arg2 {
            ffmpeg_sys_next::AV_LOG_PANIC => send_fatal!("{}", str_slice),
            ffmpeg_sys_next::AV_LOG_FATAL => send_error!("{}", str_slice),
            ffmpeg_sys_next::AV_LOG_ERROR => send_warn!("{}", str_slice),
            ffmpeg_sys_next::AV_LOG_WARNING => send_warn!("{}", str_slice),
            ffmpeg_sys_next::AV_LOG_INFO => send_info!("{}", str_slice),
            ffmpeg_sys_next::AV_LOG_VERBOSE => send_debug!("{}", str_slice),
            ffmpeg_sys_next::AV_LOG_DEBUG => send_debug!("{}", str_slice),
            ffmpeg_sys_next::AV_LOG_TRACE => send_debug!("{}", str_slice),
            _ => send_error!("FFmpeg unknown log level {}: {}", arg2, str_slice),
        }
    } else {
        send_error!("FFmpeg log: <invalid UTF-8>");
    }
}

/// 初始化 FFmpeg 日志回调
pub fn init() {
    unsafe { ffmpeg_sys_next::av_log_set_callback(Some(ffmpeg_log_callback)) };
}

pub static VIDEO_TIME_BASE: Mutex<Option<av::Rational>> = Mutex::new(None);
pub static AUDIO_TIME_BASE: Mutex<Option<av::Rational>> = Mutex::new(None);

/// 唤醒解码线程的条件变量和互斥锁
/// - bool 表示是否有新的任务需要处理
pub static DECODER_WAKEUP_MUTEX: Mutex<bool> = Mutex::new(false);
pub static DECODER_WAKEUP: Condvar = Condvar::new();

/// 进度跳转请求
/// - (是否为绝对寻址: bool, 偏移量: f64)
static SEEK_REQUEST: Mutex<Option<(bool, f64)>> = Mutex::new(None);

pub fn seek_request_relative(sec: f64) {
    let mut lock = SEEK_REQUEST.lock();
    if let Some((existing_abs, existing_off)) = *lock {
        lock.replace((existing_abs, existing_off + sec));
    } else {
        lock.replace((false, sec));
    }
    *DECODER_WAKEUP_MUTEX.lock() = true;
    DECODER_WAKEUP.notify_one();
}

pub fn seek_request_absolute(sec: f64) {
    let mut lock = SEEK_REQUEST.lock();
    lock.replace((true, sec));
    *DECODER_WAKEUP_MUTEX.lock() = true;
    DECODER_WAKEUP.notify_one();
}

pub fn decode_main(path: &str) -> Result<bool> {
    let Ok(mut ictx) = av::format::input(path) else {
        send_error!("Failed to open input file: {}", path);
        return Ok(false);
    };

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
        return Ok(true);
    }

    let (mut video_decoder, video_timebase, video_rate) = if video_stream_index >= 0 {
        let Some(stream) = ictx.stream(video_stream_index as usize) else {
            send_error!("video stream index is valid, so stream must exist");
            send_fatal!("What happened with FFmpeg?");
        };
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

    let (mut audio_decoder, audio_timebase, _audio_rate) = if audio_stream_index >= 0 {
        let Some(stream) = ictx.stream(audio_stream_index as usize) else {
            send_error!("audio stream index is valid, so stream must exist");
            send_fatal!("What happened with FFmpeg?");
        };
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

    let (mut subtitle_decoder, _subtitle_timebase) = if subtitle_stream_index >= 0 {
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

    if video_decoder.is_none() && audio_decoder.is_none() {
        send_error!("No audio or video stream found");
        send_error!("What the fuck is this file?");
        return Ok(false);
    }

    if TERM_QUIT.load(Ordering::SeqCst) != false {
        return Ok(true);
    }

    let duration = Duration::new(
        ictx.duration() as u64 / AV_TIME_BASE as u64,
        (ictx.duration() as u64 % AV_TIME_BASE as u64 * 1_000_000_000 / AV_TIME_BASE as u64) as u32,
    );

    avsync::reset(duration);

    let video_main = if video_stream_index >= 0 {
        Some(std::thread::spawn(video_main))
    } else {
        None
    };
    let audio_main = if audio_stream_index >= 0 {
        Some(std::thread::spawn(audio_main))
    } else {
        None
    };

    let mut video_queue = VecDeque::new();
    let mut audio_queue = VecDeque::new();

    avsync::hint_seeked(Duration::ZERO);

    while !(TERM_QUIT.load(Ordering::SeqCst) || avsync::decode_ended()) {
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

        if TERM_QUIT.load(Ordering::SeqCst) || avsync::decode_ended() {
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

        while (audio_stream_index < 0 || audio_queue.len() > 0)
            && (video_stream_index < 0 || video_queue.len() > 0)
        {
            if TERM_QUIT.load(Ordering::SeqCst) || avsync::decode_ended() {
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
    if let Some(video_main) = video_main {
        video_main.join().unwrap_or_else(|err| {
            send_error!("video thread join error: {:?}", err);
        });
    }
    if let Some(audio_main) = audio_main {
        audio_main.join().unwrap_or_else(|err| {
            send_error!("audio thread join error: {:?}", err);
        });
    }

    // 清除还没处理的音频和视频帧
    let _ = VIDEO_FRAME.lock().take();
    let _ = AUDIO_FRAME.lock().take();
    // 清除字幕
    subtitle::clear();

    Ok(true)
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
    let ts = ts.max(0);
    let ret = unsafe { av_seek_frame(ictx.as_mut_ptr(), -1, ts, 0) };

    // 清除还没处理的音频和视频包
    video_queue.clear();
    audio_queue.clear();
    // 清除字幕 (实际上不应该清除，但是我的处理逻辑有点问题，先这样吧)
    subtitle::clear();

    // 清除还没处理的音频和视频帧
    let _ = VIDEO_FRAME.lock().take();
    let _ = AUDIO_FRAME.lock().take();

    audio::hint_seeked();
    video::hint_seeked();
    avsync::hint_seeked(Duration::from_secs_f64(ts as f64 / AV_TIME_BASE as f64));

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
    avsync::end_decode();
    // 恢复播放状态，防止音视频线程不退出
    avsync::resume();

    // 唤醒所有等待的线程
    DECODER_WAKEUP.notify_one();
    VIDEO_FRAME_SIG.notify_one();
    AUDIO_FRAME_SIG.notify_one();
}
