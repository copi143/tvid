use anyhow::{Context, Result};
use av::{
    Subtitle,
    util::frame::{Audio as AudioFrame, video::Video as VideoFrame},
};
use ffmpeg_next as av;
use std::sync::{
    Condvar,
    atomic::{AtomicBool, Ordering},
};
use std::time::Duration;
use std::{collections::VecDeque, sync::Mutex};

use crate::audio::{AUDIO_FRAME, AUDIO_FRAME_SIG, audio_main};
use crate::subtitle;
use crate::term::TERM_QUIT;
use crate::video::{VIDEO_FRAME, VIDEO_FRAME_SIG, video_main};

pub static VIDEO_TIME_BASE: Mutex<Option<av::Rational>> = Mutex::new(None);
pub static AUDIO_TIME_BASE: Mutex<Option<av::Rational>> = Mutex::new(None);

pub static DECODER_WAKEUP: Condvar = Condvar::new();

pub static FFMPEG_END: AtomicBool = AtomicBool::new(false);

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

    let mut video_decoder = if video_stream_index >= 0 {
        let vs = ictx
            .stream(video_stream_index as usize)
            .context("video stream")?;
        let codec_ctx = av::codec::context::Context::from_parameters(vs.parameters())
            .context("video decoder")?;
        let codec = codec_ctx.decoder().video().context("video decoder")?;
        Some(codec)
    } else {
        None
    };

    let mut audio_decoder = if audio_stream_index >= 0 {
        let as_ = ictx
            .stream(audio_stream_index as usize)
            .context("audio stream")?;
        let codec_ctx = av::codec::context::Context::from_parameters(as_.parameters())
            .context("audio decoder")?;
        let codec = codec_ctx.decoder().audio().context("audio decoder")?;
        Some(codec)
    } else {
        None
    };

    let mut subtitle_decoder = if subtitle_stream_index >= 0 {
        let ss = ictx
            .stream(subtitle_stream_index as usize)
            .context("subtitle stream")?;
        let codec_ctx = av::codec::context::Context::from_parameters(ss.parameters())
            .context("subtitle decoder")?;
        let codec = codec_ctx.decoder().subtitle().context("subtitle decoder")?;
        Some(codec)
    } else {
        None
    };

    if TERM_QUIT.load(Ordering::SeqCst) != false {
        return Ok(());
    }

    FFMPEG_END.store(false, Ordering::SeqCst);

    let video_main = std::thread::spawn(video_main);
    let audio_main = std::thread::spawn(audio_main);

    let mut video_queue = VecDeque::new();
    let mut audio_queue = VecDeque::new();

    let mut decoder = ictx.packets().peekable();
    while decoder.peek().is_some() {
        if TERM_QUIT.load(Ordering::SeqCst) || FFMPEG_END.load(Ordering::SeqCst) {
            break;
        }

        if video_queue.len() > 0 && VIDEO_FRAME.lock().unwrap().is_none() {
            let video_decoder = video_decoder.as_mut().unwrap();
            if let Err(e) = video_decoder.send_packet(&video_queue.pop_front().unwrap()) {
                eprintln!("video send_packet err: {:?}", e);
                continue;
            }
            let mut frame = VideoFrame::empty();
            while video_decoder.receive_frame(&mut frame).is_ok() {
                {
                    let mut lock = VIDEO_FRAME.lock().unwrap();
                    while lock.is_some()
                        && TERM_QUIT.load(Ordering::SeqCst) == false
                        && FFMPEG_END.load(Ordering::SeqCst) == false
                    {
                        lock = VIDEO_FRAME_SIG.wait(lock).unwrap();
                    }
                    if TERM_QUIT.load(Ordering::SeqCst) || FFMPEG_END.load(Ordering::SeqCst) {
                        break;
                    }
                    *lock = Some(std::mem::replace(&mut frame, VideoFrame::empty()));
                }
                VIDEO_FRAME_SIG.notify_all();
            }
            if video_queue.len() > 0 {
                continue;
            }
        }

        if audio_queue.len() > 0 && AUDIO_FRAME.lock().unwrap().is_none() {
            let audio_decoder = audio_decoder.as_mut().unwrap();
            if let Err(e) = audio_decoder.send_packet(&audio_queue.pop_front().unwrap()) {
                eprintln!("audio send_packet err: {:?}", e);
                continue;
            }
            let mut frame = AudioFrame::empty();
            while audio_decoder.receive_frame(&mut frame).is_ok() {
                {
                    let mut lock = AUDIO_FRAME.lock().unwrap();
                    while lock.is_some()
                        && TERM_QUIT.load(Ordering::SeqCst) == false
                        && FFMPEG_END.load(Ordering::SeqCst) == false
                    {
                        lock = AUDIO_FRAME_SIG.wait(lock).unwrap();
                    }
                    if TERM_QUIT.load(Ordering::SeqCst) || FFMPEG_END.load(Ordering::SeqCst) {
                        break;
                    }
                    *lock = Some(std::mem::replace(&mut frame, AudioFrame::empty()));
                }
                AUDIO_FRAME_SIG.notify_all();
            }
            if audio_queue.len() > 0 {
                continue;
            }
        }

        if audio_queue.len() > 0 && video_queue.len() > 0 {
            static DECODER_WAKEUP_MUTEX: Mutex<()> = Mutex::new(());
            let timeout = Duration::from_millis(50);
            let guard = DECODER_WAKEUP_MUTEX.lock().unwrap();
            let _ = DECODER_WAKEUP.wait_timeout(guard, timeout).unwrap();
            continue;
        }

        let (stream, packet) = decoder.next().unwrap();
        if stream.index() as isize == video_stream_index {
            VIDEO_TIME_BASE.lock().unwrap().replace(stream.time_base());
            video_queue.push_back(packet);
        } else if stream.index() as isize == audio_stream_index {
            AUDIO_TIME_BASE.lock().unwrap().replace(stream.time_base());
            audio_queue.push_back(packet);
        } else if stream.index() as isize == subtitle_stream_index {
            let subtitle_decoder = subtitle_decoder.as_mut().unwrap();
            let mut subtitle = Subtitle::new();
            if subtitle_decoder.decode(&packet, &mut subtitle).is_ok() {
                let timebase = stream.time_base();
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

        if TERM_QUIT.load(Ordering::SeqCst) || FFMPEG_END.load(Ordering::SeqCst) {
            break;
        }
    }

    notify_quit();

    // 清除还没处理的音频和视频帧
    let _ = VIDEO_FRAME.lock().unwrap().take();
    let _ = AUDIO_FRAME.lock().unwrap().take();

    // 等待所有线程结束
    video_main.join().unwrap_or_else(|err| {
        send_error!("video thread join error: {:?}", err);
    });
    audio_main.join().unwrap_or_else(|err| {
        send_error!("audio thread join error: {:?}", err);
    });

    // 清除字幕
    subtitle::clear();

    Ok(())
}

pub fn notify_quit() {
    // 标记 ffmpeg 处理结束，以便音频和视频线程可以退出
    FFMPEG_END.store(true, Ordering::SeqCst);

    // 唤醒所有等待的线程
    VIDEO_FRAME_SIG.notify_all();
    AUDIO_FRAME_SIG.notify_all();
}
