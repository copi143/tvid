use chrono::{DateTime, Local};
use parking_lot::{Mutex, MutexGuard};
use std::collections::VecDeque;
use std::time::{Duration, Instant, SystemTime};

use crate::{stdout, util::Color};

pub struct Error {
    pub msg: String,
    pub fg: Option<Color>,
    pub bg: Option<Color>,
    pub ts: Instant,
}

static ERRORS: Mutex<VecDeque<Error>> = Mutex::new(VecDeque::new());
static mut ERRMSG_TIMEOUT: Duration = Duration::from_secs(5);

pub fn remove_expired_errors() {
    let mut lock = ERRORS.lock();
    let now = Instant::now();
    while let Some(err) = lock.front() {
        if now.duration_since(err.ts) >= unsafe { ERRMSG_TIMEOUT } {
            lock.pop_front();
        } else {
            break;
        }
    }
}

pub fn send_error(msg: &str, fg: Option<Color>, bg: Option<Color>) {
    let err = Error {
        msg: msg.to_string(),
        fg,
        bg,
        ts: Instant::now(),
    };
    let mut lock = ERRORS.lock();
    lock.push_back(err);
}

pub fn get_errors<'mutex>() -> MutexGuard<'mutex, VecDeque<Error>> {
    remove_expired_errors();
    ERRORS.lock()
}

pub fn print_errors() {
    let lock = ERRORS.lock();
    for err in lock.iter() {
        let mut text = String::new();
        let system_time = SystemTime::now().checked_sub(err.ts.elapsed()).unwrap();
        let datetime: DateTime<Local> = system_time.into();
        text.push_str(&format!("[{}] ", datetime.format("%Y-%m-%d %H:%M:%S")));
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

macro_rules! send_error {
    ($($arg:tt)*) => {
        crate::error::send_error(&format!($($arg)*), None, None)
    };
}
