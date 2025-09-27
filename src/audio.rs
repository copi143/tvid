use anyhow::{Context, Result};
use av::{
    ChannelLayout,
    software::resampling::context::Context as Resampler,
    util::{
        format::{Sample, sample::Type as SampleType},
        frame::Audio as AudioFrame,
    },
};
use cpal::SampleFormat;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use ffmpeg_next as av;
use std::collections::VecDeque;
use std::mem::MaybeUninit;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

use crate::{PAUSE, ffmpeg::FFMPEG_END};
use crate::{ffmpeg::DECODER_WAKEUP, term::TERM_QUIT};

static AUDIO_VSTARTTIME: Mutex<Option<Instant>> = Mutex::new(None);
static AUDIO_PLAYEDTIME: Mutex<Option<Duration>> = Mutex::new(None);

pub fn played_time() -> Duration {
    if PAUSE.load(Ordering::SeqCst) {
        if let Some(time) = *AUDIO_PLAYEDTIME.lock().unwrap() {
            time
        } else {
            Duration::from_millis(0)
        }
    } else {
        if let Some(time) = *AUDIO_VSTARTTIME.lock().unwrap() {
            time.elapsed()
        } else {
            Duration::from_millis(0)
        }
    }
}

pub fn played_time_or_none() -> Option<Duration> {
    if PAUSE.load(Ordering::SeqCst) {
        if let Some(time) = *AUDIO_PLAYEDTIME.lock().unwrap() {
            Some(time)
        } else {
            None
        }
    } else {
        if let Some(time) = *AUDIO_VSTARTTIME.lock().unwrap() {
            Some(time.elapsed())
        } else {
            None
        }
    }
}

static PLAYED_SAMPLES: AtomicU64 = AtomicU64::new(0);
static AUDIO_SAMPLERATE: AtomicU64 = AtomicU64::new(0);

fn update_vtime(add_samples: u64) {
    let samples = PLAYED_SAMPLES.fetch_add(add_samples, Ordering::SeqCst);
    let samplerate = AUDIO_SAMPLERATE.load(Ordering::SeqCst);
    if samplerate > 0 {
        let secs = samples / samplerate;
        let nanos = samples % samplerate * 1_000_000_000 / samplerate;
        let vtime = Duration::new(secs, nanos as u32);
        *AUDIO_PLAYEDTIME.lock().unwrap() = Some(vtime);
        *AUDIO_VSTARTTIME.lock().unwrap() = Some(Instant::now() - vtime);
    };
}

static AUDIO_BUFFER_LENGTH: AtomicUsize = AtomicUsize::new(0);
static AUDIO_CONSUMED: Condvar = Condvar::new();

fn build_cpal_stream(
    device: &cpal::Device,
    config: &cpal::SupportedStreamConfig,
    audio_buffer: Arc<Mutex<VecDeque<f32>>>,
) -> Result<cpal::Stream> {
    let err_fn = |_| { /* ignore */ };
    let sample_format = config.sample_format();
    let channels = config.channels();
    let config = &config.config();
    match sample_format {
        SampleFormat::F32 => device.build_output_stream(
            config,
            move |data: &mut [f32], _| {
                if PAUSE.load(Ordering::SeqCst) {
                    data.fill(0.0);
                    return;
                }
                AUDIO_BUFFER_LENGTH.store(data.len(), Ordering::SeqCst);
                let mut samples_to_add = data.len() as u64;
                let mut buf = audio_buffer.lock().unwrap();
                for sample in data {
                    *sample = buf.remove(0).unwrap_or_else(|| {
                        samples_to_add -= 1;
                        0.0
                    });
                }
                update_vtime(samples_to_add / channels as u64);
                AUDIO_CONSUMED.notify_all();
            },
            err_fn,
            None,
        ),
        SampleFormat::F64 => device.build_output_stream(
            config,
            move |data: &mut [f64], _| {
                if PAUSE.load(Ordering::SeqCst) {
                    data.fill(0.0);
                    return;
                }
                AUDIO_BUFFER_LENGTH.store(data.len(), Ordering::SeqCst);
                let mut samples_to_add = data.len() as u64;
                let mut buf = audio_buffer.lock().unwrap();
                for sample in data {
                    *sample = buf.remove(0).unwrap_or_else(|| {
                        samples_to_add -= 1;
                        0.0
                    }) as f64;
                }
                update_vtime(samples_to_add / channels as u64);
                AUDIO_CONSUMED.notify_all();
            },
            err_fn,
            None,
        ),
        SampleFormat::I16 => device.build_output_stream(
            config,
            move |data: &mut [i16], _| {
                if PAUSE.load(Ordering::SeqCst) {
                    data.fill(0);
                    return;
                }
                AUDIO_BUFFER_LENGTH.store(data.len(), Ordering::SeqCst);
                let mut samples_to_add = data.len() as u64;
                let mut buf = audio_buffer.lock().unwrap();
                for sample in data {
                    let v = buf.remove(0).unwrap_or_else(|| {
                        samples_to_add -= 1;
                        0.0
                    });
                    *sample = (v * std::i16::MAX as f32) as i16;
                }
                update_vtime(samples_to_add / channels as u64);
                AUDIO_CONSUMED.notify_all();
            },
            err_fn,
            None,
        ),
        SampleFormat::U16 => device.build_output_stream(
            config,
            move |data: &mut [u16], _| {
                if PAUSE.load(Ordering::SeqCst) {
                    data.fill(128);
                    return;
                }
                AUDIO_BUFFER_LENGTH.store(data.len(), Ordering::SeqCst);
                let mut samples_to_add = data.len() as u64;
                let mut buf = audio_buffer.lock().unwrap();
                for sample in data {
                    let v = buf.remove(0).unwrap_or_else(|| {
                        samples_to_add -= 1;
                        0.0
                    });
                    *sample = ((v * 0.5 + 0.5) * std::u16::MAX as f32) as u16;
                }
                update_vtime(samples_to_add / channels as u64);
                AUDIO_CONSUMED.notify_all();
            },
            err_fn,
            None,
        ),
        _ => unimplemented!("不支持的采样格式"),
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
    let audio_buffer: Arc<Mutex<VecDeque<f32>>> = Arc::new(Mutex::new(VecDeque::<f32>::new()));
    let cpal_stream = build_cpal_stream(&device, &config, audio_buffer.clone()).unwrap();
    PLAYED_SAMPLES.store(0, Ordering::SeqCst);
    AUDIO_SAMPLERATE.store(config.sample_rate().0 as u64, Ordering::SeqCst);
    cpal_stream.play().unwrap();

    let mut resampler = MaybeUninit::uninit();

    let mut resampler_format = None;
    let mut resampler_layout = None;
    let mut resampler_rate = None;

    while TERM_QUIT.load(Ordering::SeqCst) == false {
        let frame = {
            let mut lock = AUDIO_FRAME.lock().unwrap();
            while lock.is_none() && TERM_QUIT.load(Ordering::SeqCst) == false {
                if FFMPEG_END.load(Ordering::SeqCst) {
                    break;
                }
                lock = AUDIO_FRAME_SIG.wait(lock).unwrap();
            }
            if lock.is_none() {
                break;
            }
            lock.take().unwrap()
        };
        AUDIO_FRAME_SIG.notify_all();
        DECODER_WAKEUP.notify_all();

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
                    match config.channels() {
                        1 => ChannelLayout::MONO,
                        2 => ChannelLayout::STEREO,
                        3 => ChannelLayout::SURROUND,
                        4 => ChannelLayout::QUAD,
                        5 => ChannelLayout::_4POINT1,
                        6 => ChannelLayout::_5POINT1,
                        7 => ChannelLayout::_6POINT1,
                        8 => ChannelLayout::_7POINT1,
                        _ => {
                            send_error!("Unsupported channel count: {}", config.channels());
                            return;
                        }
                    },
                    config.sample_rate().0,
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

        let mut buf = audio_buffer.lock().unwrap();
        let data = converted.data(0);
        let nb_samples = converted.samples();
        let sample_count = nb_samples * target_channels as usize;
        let slice =
            unsafe { std::slice::from_raw_parts(data.as_ptr() as *const f32, sample_count) };
        buf.extend(slice);

        while buf.len() > AUDIO_BUFFER_LENGTH.load(Ordering::SeqCst) * 2
            && TERM_QUIT.load(Ordering::SeqCst) == false
        {
            buf = AUDIO_CONSUMED.wait(buf).unwrap();
        }
    }

    while audio_buffer.lock().unwrap().len() > 0 && TERM_QUIT.load(Ordering::SeqCst) == false {
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
}
