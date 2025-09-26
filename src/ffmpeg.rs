use anyhow::{Context, Result};
use av::{
    Subtitle,
    util::frame::{Audio as AudioFrame, video::Video as VideoFrame},
};
use ffmpeg_next as av;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use crate::audio::{AUDIO_FRAME, AUDIO_FRAME_SIG, audio_main};
use crate::subtitle;
use crate::term::TERM_QUIT;
use crate::video::{VIDEO_FRAME, VIDEO_FRAME_SIG, video_main};

pub static VIDEO_TIME_BASE: Mutex<Option<av::Rational>> = Mutex::new(None);
pub static AUDIO_TIME_BASE: Mutex<Option<av::Rational>> = Mutex::new(None);

pub static FFMPEG_END: AtomicBool = AtomicBool::new(false);

#[allow(static_mut_refs)]
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

    let audio_main = std::thread::spawn(audio_main);
    let video_main = std::thread::spawn(video_main);

    for (stream, packet) in ictx.packets() {
        if TERM_QUIT.load(Ordering::SeqCst) || FFMPEG_END.load(Ordering::SeqCst) {
            break;
        }

        if stream.index() as isize == video_stream_index {
            VIDEO_TIME_BASE.lock().unwrap().replace(stream.time_base());
            let video_decoder = video_decoder.as_mut().unwrap();
            if let Err(e) = video_decoder.send_packet(&packet) {
                eprintln!("video send_packet err: {:?}", e);
                continue;
            }
            let mut frame = VideoFrame::empty();
            while video_decoder.receive_frame(&mut frame).is_ok() {
                {
                    let mut lock = unsafe { VIDEO_FRAME.lock().unwrap() };
                    while lock.is_some()
                        && TERM_QUIT.load(Ordering::SeqCst) == false
                        && FFMPEG_END.load(Ordering::SeqCst) == false
                    {
                        lock = unsafe { VIDEO_FRAME_SIG.wait(lock).unwrap() };
                    }
                    if TERM_QUIT.load(Ordering::SeqCst) || FFMPEG_END.load(Ordering::SeqCst) {
                        break;
                    }
                    *lock = Some(std::mem::replace(&mut frame, VideoFrame::empty()));
                }
                unsafe { VIDEO_FRAME_SIG.notify_all() };
            }
        }

        if stream.index() as isize == audio_stream_index {
            AUDIO_TIME_BASE.lock().unwrap().replace(stream.time_base());
            let audio_decoder = audio_decoder.as_mut().unwrap();
            if let Err(e) = audio_decoder.send_packet(&packet) {
                eprintln!("audio send_packet err: {:?}", e);
                continue;
            }
            let mut frame = AudioFrame::empty();
            while audio_decoder.receive_frame(&mut frame).is_ok() {
                {
                    let mut lock = unsafe { AUDIO_FRAME.lock().unwrap() };
                    while lock.is_some()
                        && TERM_QUIT.load(Ordering::SeqCst) == false
                        && FFMPEG_END.load(Ordering::SeqCst) == false
                    {
                        lock = unsafe { AUDIO_FRAME_SIG.wait(lock).unwrap() };
                    }
                    if TERM_QUIT.load(Ordering::SeqCst) || FFMPEG_END.load(Ordering::SeqCst) {
                        break;
                    }
                    *lock = Some(std::mem::replace(&mut frame, AudioFrame::empty()));
                }
                unsafe { AUDIO_FRAME_SIG.notify_all() };
            }
        }

        if stream.index() as isize == subtitle_stream_index {
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
    let _ = unsafe { VIDEO_FRAME.lock().unwrap().take() };
    let _ = unsafe { AUDIO_FRAME.lock().unwrap().take() };

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

#[allow(static_mut_refs)]
pub fn notify_quit() {
    // 标记 ffmpeg 处理结束，以便音频和视频线程可以退出
    FFMPEG_END.store(true, Ordering::SeqCst);

    // 唤醒所有等待的线程
    unsafe { VIDEO_FRAME_SIG.notify_all() };
    unsafe { AUDIO_FRAME_SIG.notify_all() };
}
