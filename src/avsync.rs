use parking_lot::Mutex;
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Copy)]
pub struct InnerState {
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

pub struct AVSyncState {
    duration: Duration,

    /// 是否暂停
    paused: bool,

    sync: Option<InnerState>,
    audio: Option<InnerState>,
    video: Option<InnerState>,
}

impl AVSyncState {
    pub const fn new(duration: Duration) -> Self {
        Self {
            duration,
            paused: false,
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
        if !paused {
            let now = Instant::now();
            if let Some(sync) = &self.sync {
                self.sync.replace(InnerState {
                    updatetime: now,
                    playedtime: sync.playedtime,
                    vstarttime: now - sync.playedtime,
                });
            }
            if let Some(audio) = &self.audio {
                self.audio.replace(InnerState {
                    updatetime: now,
                    playedtime: audio.playedtime,
                    vstarttime: now - audio.playedtime,
                });
            }
            if let Some(video) = &self.video {
                self.video.replace(InnerState {
                    updatetime: now,
                    playedtime: video.playedtime,
                    vstarttime: now - video.playedtime,
                });
            }
        }
    }

    fn set_vitme(&mut self, vtime: Duration) {
        let now = Instant::now();
        self.sync.replace(InnerState {
            updatetime: now,
            playedtime: vtime,
            vstarttime: now - vtime,
        });
    }

    fn set_audio_vitme(&mut self, vtime: Duration) {
        let now = Instant::now();
        self.audio.replace(InnerState {
            updatetime: now,
            playedtime: vtime,
            vstarttime: now - vtime,
        });
    }

    fn set_video_vitme(&mut self, vtime: Duration) {
        let now = Instant::now();
        self.video.replace(InnerState {
            updatetime: now,
            playedtime: vtime,
            vstarttime: now - vtime,
        });
    }
}

pub static STATE: Mutex<AVSyncState> = Mutex::new(AVSyncState::new(Duration::ZERO));

/// 重置 AV 同步状态
pub fn reset(duration: Duration) {
    *STATE.lock() = AVSyncState::new(duration);
}

pub fn playback_progress() -> f64 {
    played_time_or_zero().as_secs_f64() / total_duration().as_secs_f64()
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

pub fn played_time_or_zero() -> Duration {
    played_time_or_none().unwrap_or(Duration::ZERO)
}

pub fn played_time_or_none() -> Option<Duration> {
    let state = STATE.lock();
    if state.paused {
        state.sync.map(|s| s.playedtime)
    } else {
        state.sync.map(|s| s.vstarttime.elapsed())
    }
}

pub fn audio_played_time_or_zero() -> Duration {
    audio_played_time_or_none().unwrap_or(Duration::ZERO)
}

pub fn audio_played_time_or_none() -> Option<Duration> {
    let state = STATE.lock();
    if state.paused {
        state.audio.map(|a| a.playedtime)
    } else {
        state.audio.map(|a| a.vstarttime.elapsed())
    }
}

pub fn video_played_time_or_zero() -> Duration {
    video_played_time_or_none().unwrap_or(Duration::ZERO)
}

pub fn video_played_time_or_none() -> Option<Duration> {
    let state = STATE.lock();
    if state.paused {
        state.video.map(|v| v.playedtime)
    } else {
        state.video.map(|v| v.vstarttime.elapsed())
    }
}

/// 提示已经 seek 到指定时间点
pub fn hint_seeked(ts: Duration) {
    STATE.lock().set_vitme(ts);
}

/// 提示同步模块，尝试同步音频播放时间
pub fn hint_audio_played_time(ts: Duration) {
    STATE.lock().set_vitme(ts);
    STATE.lock().set_audio_vitme(ts);
}

/// 提示同步模块，尝试同步视频播放时间
pub fn hint_video_played_time(ts: Duration) {
    STATE.lock().set_video_vitme(ts);
}
