use parking_lot::{Mutex, MutexGuard};
use std::collections::VecDeque;
use std::fmt::Debug;
use std::ops::{Add, Div};
use std::time::Duration;

// @ ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== @

pub struct MaxSizedQueue<T, const SIZE: usize> {
    queue: VecDeque<T>,
}

impl<T, const SIZE: usize> MaxSizedQueue<T, SIZE> {
    pub const fn new() -> Self {
        Self {
            queue: VecDeque::new(),
        }
    }

    pub fn push_back(&mut self, item: T) {
        self.queue.push_back(item);
        while self.queue.len() > SIZE {
            self.queue.pop_front();
        }
    }

    pub fn len(&self) -> usize {
        self.queue.len()
    }

    pub fn iter(&self) -> impl Iterator<Item = &T> {
        self.queue.iter()
    }

    pub fn iter_mut(&mut self) -> impl Iterator<Item = &mut T> {
        self.queue.iter_mut()
    }

    pub fn last(&self) -> T
    where
        T: Copy + Default,
    {
        self.queue.back().copied().unwrap_or_default()
    }

    pub fn last_or_none(&self) -> Option<T>
    where
        T: Copy,
    {
        self.queue.back().copied()
    }

    pub fn avg<U>(&self) -> T
    where
        T: Add<Output = T> + Div<U, Output = T> + Copy + Default,
        U: TryFrom<usize>,
        U::Error: Debug,
    {
        if self.queue.is_empty() {
            return T::default();
        }
        self.queue.iter().fold(T::default(), |acc, x| acc + *x)
            / U::try_from(self.queue.len()).unwrap()
    }

    pub fn avg_or_none<U>(&self) -> Option<T>
    where
        T: Add<Output = T> + Div<U, Output = T> + Copy + Default,
        U: TryFrom<usize>,
        U::Error: Debug,
    {
        if self.queue.is_empty() {
            return None;
        }
        Some(
            self.queue.iter().fold(T::default(), |acc, x| acc + *x)
                / U::try_from(self.queue.len()).unwrap(),
        )
    }
}

pub struct Statistics {
    pub render_time: MaxSizedQueue<Duration, 60>,
    pub escape_string_encode_time: MaxSizedQueue<Duration, 60>,
    pub output_time: MaxSizedQueue<Duration, 60>,
    pub video_skipped_frames: usize,
}

impl Statistics {
    pub const fn new() -> Self {
        Self {
            render_time: MaxSizedQueue::new(),
            escape_string_encode_time: MaxSizedQueue::new(),
            output_time: MaxSizedQueue::new(),
            video_skipped_frames: 0,
        }
    }
}

pub static STATISTICS: Mutex<Statistics> = Mutex::new(Statistics::new());

pub fn get_statistics() -> MutexGuard<'static, Statistics> {
    STATISTICS.lock()
}

pub fn set_render_time(duration: Duration) {
    STATISTICS.lock().render_time.push_back(duration);
}

pub fn set_escape_string_encode_time(duration: Duration) {
    STATISTICS
        .lock()
        .escape_string_encode_time
        .push_back(duration);
}

pub fn set_output_time(duration: Duration) {
    STATISTICS.lock().output_time.push_back(duration);
}

pub fn increment_video_skipped_frames() {
    STATISTICS.lock().video_skipped_frames += 1;
}
