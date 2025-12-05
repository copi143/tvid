use av::software::scaling::{context::Context as Scaler, flag::Flags};
use av::util::frame::video::Video as VideoFrame;
use ffmpeg_next as av;
use parking_lot::{Condvar, Mutex};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Duration;

use crate::avsync::{self, played_time_or_zero};
use crate::ffmpeg::{DECODER_WAKEUP, DECODER_WAKEUP_MUTEX, VIDEO_TIME_BASE};
use crate::render::{self, VIDEO_PIXELS};
use crate::statistics::increment_video_skipped_frames;
use crate::term::TERM_QUIT;

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
            error_l10n!(
                "zh-cn" => "视频帧宽度或高度为零，跳过";
                "zh-tw" => "視訊幀寬度或高度為零，跳過";
                "ja-jp" => "ビデオフレームの幅または高さがゼロのため、スキップします";
                "fr-fr" => "la largeur ou la hauteur de la trame vidéo est nulle, saut";
                "de-de" => "Videoframe hat null Breite oder Höhe, überspringen";
                "es-es" => "El fotograma de video tiene ancho o alto cero, se omite";
                _       => "Video frame has zero width or height, skipping";
            );
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
            debug_l10n!(
                "zh-cn" => "视频帧太晚: 帧时间 {frametime:?}, 播放时间 {played:?}";
                "zh-tw" => "視訊幀太晚: 幀時間 {frametime:?}, 播放時間 {played:?}";
                "ja-jp" => "ビデオフレームが遅すぎる: フレーム時間 {frametime:?}, 再生時間 {played:?}";
                "fr-fr" => "Trame vidéo trop tard : temps de la trame {frametime:?}, temps de lecture {played:?}";
                "de-de" => "Videoframe zu spät: Frame-Zeit {frametime:?}, Wiedergabe-Zeit {played:?}";
                "es-es" => "Fotograma de video demasiado tarde: tiempo de fotograma {frametime:?}, tiempo de reproducción {played:?}";
                _       => "Video frame too late: frame time {frametime:?}, played time {played:?}";
            );
            increment_video_skipped_frames();
            error_l10n!(
                "zh-cn" => "视频帧太晚，跳过";
                "zh-tw" => "視訊幀太晚，跳過";
                "ja-jp" => "ビデオフレームが遅すぎるため、スキップします";
                "fr-fr" => "Trame vidéo trop tard, saut";
                "de-de" => "Videoframe zu spät, überspringen";
                "es-es" => "Fotograma de video demasiado tarde, se omite";
                _       => "Video frame too late, skipping";
            );
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
                    error_l10n!(
                        "zh-cn" => "无法为视频帧创建缩放器";
                        "zh-tw" => "無法為視訊幀建立縮放器";
                        "ja-jp" => "ビデオフレームのスケーラーを作成できません";
                        "fr-fr" => "Impossible de créer un scaler pour la trame vidéo";
                        "de-de" => "Konnte keinen Skalierer für Videoframes erstellen";
                        "es-es" => "No se pudo crear un escalador para el fotograma de video";
                        _       => "Could not create scaler for video frame";
                    );
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
            if let Err(e) = scaler.run(&frame, &mut scaled) {
                error_l10n!(
                    "zh-cn" => "无法缩放视频帧: {e}";
                    "zh-tw" => "無法縮放視訊幀: {e}";
                    "ja-jp" => "ビデオフレームをスケーリングできません: {e}";
                    "fr-fr" => "Impossible de mettre à l'échelle la trame vidéo : {e}";
                    "de-de" => "Konnte Videoframe nicht skalieren: {e}";
                    "es-es" => "No se pudo escalar el fotograma de video: {e}";
                    _       => "Could not scale video frame: {e}";
                );
                break;
            }

            // 使用 if 防止卡死
            if !avsync::is_paused() && frametime > played_time_or_zero() + Duration::from_millis(5)
            {
                let remaining = frametime - played_time_or_zero();
                let max = Duration::from_micros(VIDEO_FRAMETIME.load(Ordering::SeqCst) * 2);
                if render::api_wait_frame_request_for(remaining.min(max)) {
                    if VIDEO_PIXELS.get() != (scaler_dst_width as usize, scaler_dst_height as usize)
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
