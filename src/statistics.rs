use parking_lot::Mutex;
use std::collections::{BTreeMap, VecDeque};
use std::fmt::Debug;
use std::ops::{Add, Div};
use std::sync::Arc;
use std::time::Duration;

// @ ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== @

pub struct MaxSizedQueue<T, const SIZE: usize> {
    queue: VecDeque<T>,
}

#[allow(unused)]
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
    pub output_bytes: MaxSizedQueue<usize, 60>,
    pub video_skipped_frames: usize,
    pub total_output_bytes: usize,
}

impl Statistics {
    pub const fn new() -> Self {
        Self {
            render_time: MaxSizedQueue::new(),
            escape_string_encode_time: MaxSizedQueue::new(),
            output_time: MaxSizedQueue::new(),
            output_bytes: MaxSizedQueue::new(),
            video_skipped_frames: 0,
            total_output_bytes: 0,
        }
    }
}

static STATISTICS: Mutex<BTreeMap<i32, Arc<Mutex<Statistics>>>> = Mutex::new(BTreeMap::new());

pub fn get(id: i32) -> Arc<Mutex<Statistics>> {
    STATISTICS
        .lock()
        .entry(id)
        .or_insert_with(|| Arc::new(Mutex::new(Statistics::new())))
        .clone()
}

pub fn set_render_time(id: i32, duration: Duration) {
    get(id).lock().render_time.push_back(duration);
}

pub fn set_escape_string_encode_time(id: i32, duration: Duration) {
    get(id).lock().escape_string_encode_time.push_back(duration);
}

pub fn set_output_time(id: i32, duration: Duration) {
    get(id).lock().output_time.push_back(duration);
}

pub fn set_output_bytes(id: i32, num: usize) {
    get(id).lock().output_bytes.push_back(num);
}

pub fn increment_video_skipped_frames(id: i32, num: usize) {
    get(id).lock().video_skipped_frames += num;
}

pub fn increment_total_output_bytes(id: i32, num: usize) {
    get(id).lock().total_output_bytes += num;
}
