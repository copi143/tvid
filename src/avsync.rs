use parking_lot::Mutex;
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Copy)]
struct InnerState {
    /// 上次更新时间
    updatetime: Instant,
    /// 播放时间（用于暂停时的时间记录）
    playedtime: Duration,
    /// 播放开始时间（用于非暂停时的时间记录）
    ///
    /// 注意：
    /// - 该时间非实际播放开始时间，而是用于计算播放时间的参考时间点
    /// - 暂停时该时间会被冻结，不会更新
    vstarttime: Instant,
}

impl InnerState {
    fn new(vtime: Duration) -> Self {
        let now = Instant::now();
        Self {
            updatetime: now,
            playedtime: vtime,
            vstarttime: now - vtime,
        }
    }

    fn resume(&mut self, now: Instant) {
        *self = InnerState {
            updatetime: now,
            playedtime: self.playedtime,
            vstarttime: now - self.playedtime,
        };
    }
}

struct AVSyncState {
    duration: Duration,

    /// 是否暂停
    paused: bool,

    /// 解码是否应该结束（无论外边或内部导致）
    decode_end: bool,

    has_audio: bool,
    has_video: bool,

    sync: Option<InnerState>,
    audio: Option<InnerState>,
    video: Option<InnerState>,
}

impl AVSyncState {
    const fn new(duration: Duration, has_audio: bool, has_video: bool) -> Self {
        Self {
            duration,
            paused: false,
            decode_end: false,
            has_audio,
            has_video,
            sync: None,
            audio: None,
            video: None,
        }
    }

    fn switch_pause(&mut self) {
        let paused = !self.paused;
        self.set_pause(paused);
    }

    fn set_pause(&mut self, paused: bool) {
        self.paused = paused;
        if paused {
            let now = Instant::now();
            if let Some(sync) = self.sync.as_mut() {
                sync.playedtime = now.duration_since(sync.vstarttime);
            }
            if let Some(audio) = self.audio.as_mut() {
                audio.playedtime = now.duration_since(audio.vstarttime);
            }
            if let Some(video) = self.video.as_mut() {
                video.playedtime = now.duration_since(video.vstarttime);
            }
        } else {
            let now = Instant::now();
            if let Some(sync) = self.sync.as_mut() {
                sync.resume(now)
            }
            if let Some(audio) = self.audio.as_mut() {
                audio.resume(now)
            }
            if let Some(video) = self.video.as_mut() {
                video.resume(now)
            }
        }
    }

    fn set_time(&mut self, vtime: Duration) {
        self.sync.replace(InnerState::new(vtime));
        self.tick();
    }

    fn set_audio_time(&mut self, vtime: Duration) {
        self.audio.replace(InnerState::new(vtime));
        self.tick();
    }

    fn set_video_time(&mut self, vtime: Duration) {
        self.video.replace(InnerState::new(vtime));
        self.tick();
    }

    /// 临时用的同步逻辑
    fn tick(&mut self) {
        if self.paused || self.sync.is_none() {
            return;
        }
        let time = self.sync.map(|s| s.vstarttime.elapsed()).unwrap();
        let atime = self.audio.map(|s| s.vstarttime.elapsed());
        let vtime = self.video.map(|s| s.vstarttime.elapsed());
        match (atime, vtime) {
            (Some(atime), Some(vtime)) => {
                let adiff = (time.as_secs_f64() - atime.as_secs_f64()).abs();
                let vdiff = (time.as_secs_f64() - vtime.as_secs_f64()).abs();
                if adiff > 0.02 {
                    self.set_time(atime);
                }
            }
            (Some(atime), None) => {
                let adiff = (time.as_secs_f64() - atime.as_secs_f64()).abs();
                if adiff > 0.02 {
                    self.set_time(atime);
                }
            }
            (None, Some(vtime)) => {
                let vdiff = (time.as_secs_f64() - vtime.as_secs_f64()).abs();
                if vdiff > 0.02 {
                    self.set_time(vtime);
                }
            }
            (None, None) => {}
        }
    }
}

static STATE: Mutex<AVSyncState> = Mutex::new(AVSyncState::new(Duration::ZERO, false, false));

/// 重置 AV 同步状态
pub fn reset(duration: Duration, has_audio: bool, has_video: bool) {
    *STATE.lock() = AVSyncState::new(duration, has_audio, has_video);
}

pub fn playback_progress() -> f64 {
    let total = total_duration();
    if total.is_zero() {
        return 0.0;
    }
    played_time_or_zero().as_secs_f64() / total.as_secs_f64()
}

pub fn total_duration() -> Duration {
    STATE.lock().duration
}

pub fn is_paused() -> bool {
    STATE.lock().paused
}

pub fn pause() {
    STATE.lock().set_pause(true);
}

pub fn resume() {
    STATE.lock().set_pause(false);
}

pub fn switch_pause_state() {
    STATE.lock().switch_pause();
}

pub fn end_decode() {
    STATE.lock().decode_end = true;
}

pub fn decode_ended() -> bool {
    STATE.lock().decode_end
}

pub fn has_audio() -> bool {
    STATE.lock().has_audio
}

pub fn has_video() -> bool {
    STATE.lock().has_video
}

macro_rules! played_time {
    ($fn1:ident, $fn2:ident, $mb:ident) => {
        pub fn $fn1() -> Duration {
            $fn2().unwrap_or(Duration::ZERO)
        }

        pub fn $fn2() -> Option<Duration> {
            let mut state = STATE.lock();
            state.tick();
            if state.paused {
                state.$mb.map(|s| s.playedtime)
            } else {
                state.$mb.map(|s| s.vstarttime.elapsed())
            }
        }
    };
}

played_time!(played_time_or_zero, played_time_or_none, sync);
played_time!(audio_played_time_or_zero, audio_played_time_or_none, audio);
played_time!(video_played_time_or_zero, video_played_time_or_none, video);

/// 提示已经 seek 到指定时间点
pub fn hint_seeked(ts: Duration) {
    STATE.lock().set_time(ts);
}

/// 提示同步模块，尝试同步音频播放时间
pub fn hint_audio_played_time(ts: Duration) {
    STATE.lock().set_audio_time(ts);
}

/// 提示同步模块，尝试同步视频播放时间
pub fn hint_video_played_time(ts: Duration) {
    STATE.lock().set_video_time(ts);
}
