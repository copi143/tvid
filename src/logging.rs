use chrono::{DateTime, Local};
use parking_lot::{Mutex, MutexGuard};
use std::collections::VecDeque;
use std::time::{Duration, Instant, SystemTime};

use crate::{stdout, util::Color};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum MessageLevel {
    Debug,
    Info,
    Warn,
    Error,
    Fatal,
}

impl MessageLevel {
    pub const fn level_str(&self) -> &'static str {
        match self {
            MessageLevel::Debug => "[Debug] ",
            MessageLevel::Info => "[Info ] ",
            MessageLevel::Warn => "[Warn ] ",
            MessageLevel::Error => "[Error] ",
            MessageLevel::Fatal => "[Fatal] ",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Message {
    pub lv: MessageLevel,
    pub msg: String,
    pub fg: Option<Color>,
    pub bg: Option<Color>,
    pub ts: Instant,
}

pub struct Messages {
    pub queue: VecDeque<Message>,
    pub timeout: Duration,
}

static MESSAGES: Mutex<Messages> = Mutex::new(Messages {
    queue: VecDeque::new(),
    timeout: Duration::from_secs(5),
});

pub fn remove_expired_messages() {
    let now = Instant::now();
    let mut lock = MESSAGES.lock();
    while let Some(err) = lock.queue.front() {
        if now.duration_since(err.ts) >= lock.timeout {
            lock.queue.pop_front();
        } else {
            break;
        }
    }
}

pub fn get_messages<'mutex>() -> MutexGuard<'mutex, Messages> {
    remove_expired_messages();
    MESSAGES.lock()
}

pub fn print_messages() {
    let lock = MESSAGES.lock();
    for err in lock.queue.iter() {
        let mut text = String::new();
        let system_time = SystemTime::now().checked_sub(err.ts.elapsed()).unwrap();
        let datetime: DateTime<Local> = system_time.into();
        text.push_str(&format!("[{}] ", datetime.format("%Y-%m-%d %H:%M:%S")));
        text.push_str(err.lv.level_str());
        if let Some(fg) = err.fg {
            text.push_str(&format!("\x1b[38;2;{};{};{}m", fg.r, fg.g, fg.b));
        }
        if let Some(bg) = err.bg {
            text.push_str(&format!("\x1b[48;2;{};{};{}m", bg.r, bg.g, bg.b));
        }
        text.push_str(&err.msg);
        if err.fg.is_some() || err.bg.is_some() {
            text.push_str("\x1b[0m");
        }
        text.push('\n');
        stdout::print(text.as_bytes());
    }
}

pub fn send_message(lv: MessageLevel, msg: &str, fg: Option<Color>, bg: Option<Color>) {
    let err = Message {
        lv,
        msg: msg.to_string(),
        fg,
        bg,
        ts: Instant::now(),
    };
    let mut lock = MESSAGES.lock();
    lock.queue.push_back(err);
}

pub fn send_debug(msg: &str, fg: Option<Color>, bg: Option<Color>) {
    send_message(MessageLevel::Debug, msg, fg, bg);
}

pub fn send_info(msg: &str, fg: Option<Color>, bg: Option<Color>) {
    send_message(MessageLevel::Info, msg, fg, bg);
}

pub fn send_warn(msg: &str, fg: Option<Color>, bg: Option<Color>) {
    send_message(MessageLevel::Warn, msg, fg, bg);
}

pub fn send_error(msg: &str, fg: Option<Color>, bg: Option<Color>) {
    send_message(MessageLevel::Error, msg, fg, bg);
}

macro_rules! send_debug {
    ($($arg:tt)*) => {
        crate::logging::send_debug(&format!($($arg)*), None, None)
    };
}

macro_rules! send_info {
    ($($arg:tt)*) => {
        crate::logging::send_info(&format!($($arg)*), None, None)
    };
}

macro_rules! send_warn {
    ($($arg:tt)*) => {
        crate::logging::send_warn(&format!($($arg)*), None, None)
    };
}

macro_rules! send_error {
    ($($arg:tt)*) => {
        crate::logging::send_error(&format!($($arg)*), None, None)
    };
}
