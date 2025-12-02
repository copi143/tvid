use anyhow::{Context, Result};
use av::util::format::{Sample, sample::Type as SampleType};
use av::util::frame::Audio as AudioFrame;
use av::{ChannelLayout, software::resampling::context::Context as Resampler};
use cpal::SampleFormat;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use ffmpeg_next as av;
use parking_lot::{Condvar, Mutex};
use std::collections::VecDeque;
use std::mem::MaybeUninit;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::time::Duration;

use crate::ffmpeg::{AUDIO_TIME_BASE, DECODER_WAKEUP, DECODER_WAKEUP_MUTEX};
use crate::term::TERM_QUIT;
use crate::{avsync, ffmpeg};

static PLAYED_SAMPLES: AtomicU64 = AtomicU64::new(0);
static AUDIO_SAMPLERATE: AtomicU64 = AtomicU64::new(0);

fn update_vtime(add_samples: u64) {
    if add_samples == 0 {
        return;
    }
    let sn = PLAYED_SAMPLES.fetch_add(add_samples, Ordering::SeqCst);
    let sr = AUDIO_SAMPLERATE.load(Ordering::SeqCst);
    assert!(sr > 0, "samplerate should be set before update_vtime");
    let secs = sn / sr;
    let nanos = sn % sr * 1_000_000_000 / sr;
    let vtime = Duration::new(secs, nanos as u32);
    avsync::hint_audio_played_time(vtime);
}

fn set_vtime(vtime: Duration) {
    let sr = AUDIO_SAMPLERATE.load(Ordering::SeqCst);
    assert!(sr > 0, "samplerate should be set before set_vtime");
    let sn = vtime.as_secs() * sr + vtime.subsec_nanos() as u64 * sr / 1_000_000_000;
    PLAYED_SAMPLES.store(sn, Ordering::SeqCst);
    avsync::hint_audio_played_time(vtime);
}

static HINT_SEEKED: AtomicBool = AtomicBool::new(false);

/// 提示音频模块已经 seek 到指定时间点
pub fn hint_seeked() {
    HINT_SEEKED.store(true, Ordering::SeqCst);
}

struct AudioFrameWrapper {
    ts: Duration,
    af: AudioFrame,
    cons: usize,
    prev_ts: Option<Duration>,
    next_ts: Option<Duration>,
}

impl AudioFrameWrapper {
    fn new(ts: Duration, af: AudioFrame) -> Self {
        Self {
            ts,
            af,
            cons: 0,
            prev_ts: None,
            next_ts: None,
        }
    }

    fn timestamp(&self) -> Option<Duration> {
        if self.cons == 0 { Some(self.ts) } else { None }
    }

    fn slice(&self) -> &[f32] {
        let data = self.af.data(0);
        let nb_samples = self.af.samples();
        let channels = self.af.channel_layout().channels() as usize;
        let len = nb_samples * channels;
        let slice = unsafe { std::slice::from_raw_parts(data.as_ptr() as *const f32, len) };
        &slice[self.cons..]
    }

    fn full_len(&self) -> usize {
        self.af.samples() * self.af.channel_layout().channels() as usize
    }

    fn consume(&mut self, n: usize) {
        self.cons += n;
    }

    fn calc_volume(&self) -> f32 {
        let data = self.af.data(0);
        let nb_samples = self.af.samples();
        let channels = self.af.channel_layout().channels() as usize;
        let len = nb_samples * channels;
        let slice = unsafe { std::slice::from_raw_parts(data.as_ptr() as *const f32, len) };
        let mut max = 0.0f32;
        for &v in slice.iter() {
            let av = v.abs();
            if av > max {
                max = av;
            }
        }
        max * max
    }
}

pub static AUDIO_VOLUME_STATISTICS: Mutex<VecDeque<f32>> = Mutex::new(VecDeque::new());
pub const AUDIO_VOLUME_STATISTICS_LEN: usize = 128;

static AUDIO_BUFFER: Mutex<VecDeque<AudioFrameWrapper>> = Mutex::new(VecDeque::new());
static AUDIO_CONSUMED: Condvar = Condvar::new();

/// 当前 CPAL 音频缓冲区长度（采样点数）
static CPAL_BUFFER_LEN: AtomicUsize = AtomicUsize::new(0);
/// 当前音频缓冲区长度（采样点数）
static AUDIO_BUFFER_LEN: AtomicUsize = AtomicUsize::new(0);

static mut VOLUME_K: f32 = 0.25;

macro_rules! data_callback {
    ($channels:expr, $ty:ty, $default:expr, $expr:expr) => {
        move |data: &mut [$ty], _| {
            let channels = $channels;
            assert!(data.len() != 0);
            assert!(data.len() % channels as usize == 0);
            CPAL_BUFFER_LEN.store(data.len() / channels as usize, Ordering::SeqCst);
            if avsync::is_paused() {
                data.fill($default);
                return;
            }
            let sr = AUDIO_SAMPLERATE.load(Ordering::SeqCst);
            let mut dur = None;
            let mut add = None;
            let mut i = 0;
            let mut buf = AUDIO_BUFFER.lock();
            while let Some(mut wrap) = buf.pop_front() {
                let wrap_ptr = &mut wrap as *mut AudioFrameWrapper;
                if let Some(d) = wrap.timestamp() {
                    dur = Some(d);
                    add = None;
                }
                let slice_full_len = wrap.full_len();
                assert!(slice_full_len % channels as usize == 0);
                let slice_time = slice_full_len as f64 / channels as f64 / sr as f64;
                // 上一个帧到当前是否被跳过了一些内容
                let prev_skiped = if wrap.prev_ts.is_some() {
                    (wrap.ts.as_secs_f64() - wrap.prev_ts.unwrap().as_secs_f64()).abs()
                        > slice_time * 2.0
                } else {
                    false
                };
                // 当前帧到下一个是否会跳过一些内容
                let next_skiped = if wrap.next_ts.is_some() {
                    (wrap.ts.as_secs_f64() - wrap.next_ts.unwrap().as_secs_f64()).abs()
                        > slice_time * 2.0
                } else {
                    false
                };
                let slice_begin = wrap.cons;
                assert!(slice_begin % channels as usize == 0);
                if prev_skiped || next_skiped {
                    for (j, &v) in wrap.slice().iter().enumerate() {
                        let k = (slice_begin + j) as f32 / slice_full_len as f32;
                        let mut v = v * unsafe { VOLUME_K };
                        if prev_skiped {
                            v *= k;
                        }
                        if next_skiped {
                            v *= 1.0 - k;
                        }
                        data[i] = ($expr)(v);
                        i += 1;
                        if i == data.len() {
                            let n = j + 1;
                            wrap.consume(n);
                            add = Some(n as u64 / channels as u64);
                            buf.push_front(wrap);
                            break;
                        }
                    }
                } else {
                    for (j, &v) in wrap.slice().iter().enumerate() {
                        data[i] = ($expr)(v * unsafe { VOLUME_K });
                        i += 1;
                        if i == data.len() {
                            let n = j + 1;
                            wrap.consume(n);
                            add = Some(n as u64 / channels as u64);
                            buf.push_front(wrap);
                            break;
                        }
                    }
                }
                if i == data.len() {
                    break;
                }
                let vol = unsafe { (*wrap_ptr).calc_volume() };
                let mut stat = AUDIO_VOLUME_STATISTICS.lock();
                while stat.len() >= AUDIO_VOLUME_STATISTICS_LEN {
                    stat.pop_front();
                }
                stat.push_back(vol);
            }
            assert!(i <= data.len() && i % channels as usize == 0);
            AUDIO_BUFFER_LEN.fetch_sub(i / channels as usize, Ordering::SeqCst);
            while i < data.len() {
                data[i] = $default;
                i += 1;
            }
            dur.map(|dur| set_vtime(dur));
            add.map(|add| update_vtime(add));
            AUDIO_CONSUMED.notify_one();
        }
    };
}

/// 构建 CPAL 音频输出流（辅助宏）
macro_rules! build_output_stream {
    ($device:expr, $config:expr, $ty:ty, $default:expr, $expr:expr) => {{
        let channels = $config.channels();
        let config = &$config.config();
        $device.build_output_stream(
            config,
            data_callback!(channels, $ty, $default, $expr),
            |_| { /* ignore */ },
            None,
        )
    }};
}

/// 构建 CPAL 音频输出流
fn build_cpal_stream(
    device: &cpal::Device,
    config: &cpal::SupportedStreamConfig,
) -> Result<cpal::Stream> {
    macro_rules! inorm {
        ($v:expr) => {
            $v.clamp(-1.0, 1.0)
        };
    }
    macro_rules! unorm {
        ($v:expr) => {
            $v.clamp(-1.0, 1.0) * 0.5 + 0.5
        };
    }
    match config.sample_format() {
        SampleFormat::F32 => build_output_stream!(device, config, f32, 0.0, |v: f32| v),
        SampleFormat::F64 => build_output_stream!(device, config, f64, 0.0, |v: f32| v as f64),
        SampleFormat::I8 => build_output_stream!(device, config, i8, 0, |v: f32| {
            (inorm!(v) * i8::MAX as f32) as i8
        }),
        SampleFormat::U8 => build_output_stream!(device, config, u8, 128, |v: f32| {
            (unorm!(v) * u8::MAX as f32) as u8
        }),
        SampleFormat::I16 => build_output_stream!(device, config, i16, 0, |v: f32| {
            (inorm!(v) * i16::MAX as f32) as i16
        }),
        SampleFormat::U16 => build_output_stream!(device, config, u16, 32768, |v: f32| {
            (unorm!(v) * u16::MAX as f32) as u16
        }),
        SampleFormat::I32 => build_output_stream!(device, config, i32, 0, |v: f32| {
            (inorm!(v) * i32::MAX as f32) as i32
        }),
        SampleFormat::U32 => build_output_stream!(device, config, u32, 2147483648, |v: f32| {
            (unorm!(v) * u32::MAX as f32) as u32
        }),
        _ => unimplemented!("Unsupported sample format: {:?}", config.sample_format()),
    }
    .map_err(|e| e.into())
}

pub static AUDIO_FRAME: Mutex<Option<AudioFrame>> = Mutex::new(None);
pub static AUDIO_FRAME_SIG: Condvar = Condvar::new();

pub fn audio_main() {
    let host = cpal::default_host();
    let device = host
        .default_output_device()
        .context("No default output audio device")
        .unwrap();
    let config = device.default_output_config().unwrap();
    let target_channels = config.channels();
    let target_sample_fmt = Sample::F32(SampleType::Packed);
    let target_sample_rate = config.sample_rate().0;
    let cpal_stream = build_cpal_stream(&device, &config).unwrap();
    if target_sample_rate == 0 {
        error_l10n!(
            "zh-cn" => "无效的音频采样率: 0";
            "zh-tw" => "無效的音訊取樣率: 0";
            "ja-jp" => "無効なオーディオサンプルレート: 0";
            "fr-fr" => "taux d'échantillonnage audio invalide : 0";
            "de-de" => "Ungültige Audio-Samplerate: 0";
            "es-es" => "frecuencia de muestreo de audio no válida: 0";
            _       => "Invalid audio sample rate: 0";
        );
        error_l10n!(
            "zh-cn" => "退出音频线程";
            "zh-tw" => "退出音訊執行緒";
            "ja-jp" => "オーディオスレッドを終了します";
            "fr-fr" => "quitter le thread audio";
            "de-de" => "Beenden des Audiothreads";
            "es-es" => "saliendo del hilo de audio";
            _       => "Quiting audio thread";
        );
        ffmpeg::notify_quit();
        return;
    }
    PLAYED_SAMPLES.store(0, Ordering::SeqCst);
    AUDIO_SAMPLERATE.store(target_sample_rate as u64, Ordering::SeqCst);
    AUDIO_BUFFER.lock().clear();
    AUDIO_BUFFER_LEN.store(0, Ordering::SeqCst);
    cpal_stream.play().unwrap();
    set_vtime(Duration::ZERO);

    let target_channel_layout = match target_channels {
        1 => ChannelLayout::MONO,
        2 => ChannelLayout::STEREO,
        3 => ChannelLayout::SURROUND,
        4 => ChannelLayout::QUAD,
        5 => ChannelLayout::_4POINT1,
        6 => ChannelLayout::_5POINT1,
        7 => ChannelLayout::_6POINT1,
        8 => ChannelLayout::_7POINT1,
        _ => {
            error_l10n!(
                "zh-cn" => "不支持的声道数: {target_channels}";
                "zh-tw" => "不支援的聲道數: {target_channels}";
                "ja-jp" => "サポートされていないチャンネル数: {target_channels}";
                "fr-fr" => "nombre de canaux non pris en charge : {target_channels}";
                "de-de" => "Nicht unterstützte Kanalanzahl: {target_channels}";
                "es-es" => "número de canales no compatible: {target_channels}";
                _       => "Unsupported channel count: {target_channels}";
            );
            error_l10n!(
                "zh-cn" => "退出音频线程";
                "zh-tw" => "退出音訊執行緒";
                "ja-jp" => "オーディオスレッドを終了します";
                "fr-fr" => "quitter le thread audio";
                "de-de" => "Beenden des Audiothreads";
                "es-es" => "saliendo del hilo de audio";
                _       => "Quiting audio thread";
            );
            ffmpeg::notify_quit();
            return;
        }
    };

    let mut resampler = MaybeUninit::uninit();

    let mut resampler_format = None;
    let mut resampler_layout = None;
    let mut resampler_rate = None;

    let mut last_frametime = None;

    while TERM_QUIT.load(Ordering::SeqCst) == false {
        let frame = {
            let mut lock = AUDIO_FRAME.lock();
            while lock.is_none() && TERM_QUIT.load(Ordering::SeqCst) == false {
                if avsync::decode_ended() {
                    break;
                }
                AUDIO_FRAME_SIG.wait_for(&mut lock, Duration::from_millis(100));
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
            let base = AUDIO_TIME_BASE.lock().unwrap();
            Duration::new(
                pts as u64 * base.0 as u64 / base.1 as u64,
                (pts as u64 * base.0 as u64 % base.1 as u64 * 1_000_000_000 / base.1 as u64) as u32,
            )
        };

        if HINT_SEEKED.swap(false, Ordering::SeqCst) {
            AUDIO_BUFFER.lock().clear();
            AUDIO_BUFFER_LEN.store(0, Ordering::SeqCst);
        }

        if Some(frame.format()) != resampler_format
            || Some(frame.channel_layout()) != resampler_layout
            || Some(frame.rate()) != resampler_rate
        {
            resampler = MaybeUninit::new(
                Resampler::get(
                    frame.format(),
                    frame.channel_layout(),
                    frame.rate(),
                    target_sample_fmt,
                    target_channel_layout,
                    target_sample_rate,
                )
                .context("Could not create resampler")
                .unwrap(),
            );
            resampler_format = Some(frame.format());
            resampler_layout = Some(frame.channel_layout());
            resampler_rate = Some(frame.rate());
        }

        let mut converted = AudioFrame::empty();
        unsafe { resampler.assume_init_mut() }
            .run(&frame, &mut converted)
            .context("resampler run failed")
            .unwrap();

        AUDIO_BUFFER_LEN.fetch_add(converted.samples(), Ordering::SeqCst);

        let mut buf = AUDIO_BUFFER.lock();
        buf.back_mut().map(|w| w.next_ts = Some(frametime));
        buf.push_back(AudioFrameWrapper::new(frametime, converted));
        buf.back_mut().map(|w| w.prev_ts = last_frametime);
        last_frametime = Some(frametime);

        let buflen = || AUDIO_BUFFER_LEN.load(Ordering::SeqCst);
        let maxbuf = || (CPAL_BUFFER_LEN.load(Ordering::SeqCst) * 2).max(1024);
        while buflen() > maxbuf() && TERM_QUIT.load(Ordering::SeqCst) == false {
            AUDIO_CONSUMED.wait_for(&mut buf, Duration::from_millis(20));
        }
    }

    while AUDIO_BUFFER.lock().len() > 0 && TERM_QUIT.load(Ordering::SeqCst) == false {
        std::thread::sleep(Duration::from_millis(100));
    }
}
