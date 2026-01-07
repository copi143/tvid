use anyhow::Result;
use chrono::{DateTime, Local};
use data_classes::derive::*;
use parking_lot::{Mutex, MutexGuard};
use std::collections::VecDeque;
use std::fmt::Write as _;
use std::time::{Duration, SystemTime};

use crate::term;
use crate::{stdout, util::Color};

pub const COLOR_DEBUG: Color = Color::new(128, 192, 255);
pub const COLOR_INFO: Color = Color::new(64, 192, 128);
pub const COLOR_WARN: Color = Color::new(255, 192, 0);
pub const COLOR_ERROR: Color = Color::new(255, 128, 64);
pub const COLOR_FATAL: Color = Color::new(255, 64, 64);

#[data(copy)]
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
            MessageLevel::Debug => "[Debug]",
            MessageLevel::Info => "[Info ]",
            MessageLevel::Warn => "[Warn ]",
            MessageLevel::Error => "[Error]",
            MessageLevel::Fatal => "[Fatal]",
        }
    }

    pub const fn level_color(&self) -> Color {
        match self {
            MessageLevel::Debug => COLOR_DEBUG,
            MessageLevel::Info => COLOR_INFO,
            MessageLevel::Warn => COLOR_WARN,
            MessageLevel::Error => COLOR_ERROR,
            MessageLevel::Fatal => COLOR_FATAL,
        }
    }
}

#[data]
pub struct Message {
    pub lv: MessageLevel,
    pub msg: String,
    pub fg: Option<Color>,
    pub bg: Option<Color>,
    pub ts: SystemTime,
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
    let now = SystemTime::now();
    let mut lock = MESSAGES.lock();
    while let Some(err) = lock.queue.front() {
        if now.duration_since(err.ts).unwrap_or_default() >= lock.timeout {
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

pub fn print_messages() -> Result<()> {
    let lock = MESSAGES.lock();
    if lock.queue.is_empty() {
        return Ok(());
    }
    let mut text = String::new();
    for err in lock.queue.iter() {
        let datetime = DateTime::<Local>::from(err.ts).format("%Y-%m-%d %H:%M:%S");
        let lv = err.lv.level_color();
        text.write_char('[')?;
        write!(text, "\x1b[38;2;{};{};{}m", lv.r, lv.g, lv.b)?;
        write!(text, "{}", datetime)?;
        write!(text, "\x1b[0m")?;
        text.write_char(']')?;
        text.write_char(' ')?;
        write!(text, "\x1b[48;2;{};{};{}m", lv.r, lv.g, lv.b)?;
        write!(text, "{}", err.lv.level_str())?;
        write!(text, "\x1b[0m")?;
        text.write_char(' ')?;
        if let Some(fg) = err.fg {
            write!(text, "\x1b[38;2;{};{};{}m", fg.r, fg.g, fg.b)?;
        } else {
            write!(text, "\x1b[38;2;{};{};{}m", lv.r, lv.g, lv.b)?;
        }
        if let Some(bg) = err.bg {
            write!(text, "\x1b[48;2;{};{};{}m", bg.r, bg.g, bg.b)?;
        }
        write!(text, "{}", err.msg)?;
        write!(text, "\x1b[0m\n")?;
    }
    let bytes = text.as_bytes();
    if stdout::print(bytes) == Some(bytes.len()) {
        Ok(())
    } else {
        Err(anyhow::anyhow!("Failed to print all log messages"))
    }
}

pub fn send_message(lv: MessageLevel, msg: &str, fg: Option<Color>, bg: Option<Color>) {
    let err = Message {
        lv,
        msg: msg.to_string(),
        fg,
        bg,
        ts: SystemTime::now(),
    };
    let mut lock = MESSAGES.lock();
    lock.queue.push_back(err);
}

pub fn debug(msg: &str, fg: Option<Color>, bg: Option<Color>) {
    send_message(MessageLevel::Debug, msg, fg, bg);
}

pub fn info(msg: &str, fg: Option<Color>, bg: Option<Color>) {
    send_message(MessageLevel::Info, msg, fg, bg);
}

pub fn warning(msg: &str, fg: Option<Color>, bg: Option<Color>) {
    send_message(MessageLevel::Warn, msg, fg, bg);
}

pub fn error(msg: &str, fg: Option<Color>, bg: Option<Color>) {
    send_message(MessageLevel::Error, msg, fg, bg);
}

pub fn fatal(msg: &str, fg: Option<Color>, bg: Option<Color>) -> ! {
    send_message(MessageLevel::Fatal, msg, fg, bg);
    term::request_quit();
    term::quit();
}

macro_rules! debug {
    ($($arg:tt)*) => {
        crate::logging::debug(&format!($($arg)*), None, None)
    };
}

macro_rules! info {
    ($($arg:tt)*) => {
        crate::logging::info(&format!($($arg)*), None, None)
    };
}

macro_rules! warning {
    ($($arg:tt)*) => {
        crate::logging::warning(&format!($($arg)*), None, None)
    };
}

macro_rules! error {
    ($($arg:tt)*) => {
        crate::logging::error(&format!($($arg)*), None, None)
    };
}

macro_rules! fatal {
    ($($arg:tt)*) => {
        crate::logging::fatal(&format!($($arg)*), None, None)
    };
}

#[cfg(feature = "i18n")]
macro_rules! debug_l10n {
    ($($lang:tt => $($arg:tt),+);+ $(;)?) => {
        match crate::LOCALE.as_str() {
            $(
                $lang => debug!($($arg),+),
            )+
        }
    };
}

#[cfg(not(feature = "i18n"))]
macro_rules! debug_l10n {
    ($_a:tt => $($_b:tt),+ ; $($rest:tt)+) => {
        debug_l10n!($($rest)+)
    };
    (_ => $($arg:tt),+ $(;)?) => {
        debug!($($arg),+)
    };
}

#[cfg(feature = "i18n")]
macro_rules! info_l10n {
    ($($lang:tt => $($arg:tt),+);+ $(;)?) => {
        match crate::LOCALE.as_str() {
            $(
                $lang => info!($($arg),+),
            )+
        }
    };
}

#[cfg(not(feature = "i18n"))]
macro_rules! info_l10n {
    ($_a:tt => $($_b:tt),+ ; $($rest:tt)+) => {
        info_l10n!($($rest)+)
    };
    (_ => $($arg:tt),+ $(;)?) => {
        info!($($arg),+)
    };
}

#[cfg(feature = "i18n")]
macro_rules! warning_l10n {
    ($($lang:tt => $($arg:tt),+);+ $(;)?) => {
        match crate::LOCALE.as_str() {
            $(
                $lang => warning!($($arg),+),
            )+
        }
    };
}

#[cfg(not(feature = "i18n"))]
macro_rules! warning_l10n {
    ($_a:tt => $($_b:tt),+ ; $($rest:tt)+) => {
        warning_l10n!($($rest)+)
    };
    (_ => $($arg:tt),+ $(;)?) => {
        warning!($($arg),+)
    };
}

#[cfg(feature = "i18n")]
macro_rules! error_l10n {
    ($($lang:tt => $($arg:tt),+);+ $(;)?) => {
        match crate::LOCALE.as_str() {
            $(
                $lang => error!($($arg),+),
            )+
        }
    };
}

#[cfg(not(feature = "i18n"))]
macro_rules! error_l10n {
    ($_a:tt => $($_b:tt),+ ; $($rest:tt)+) => {
        error_l10n!($($rest)+)
    };
    (_ => $($arg:tt),+ $(;)?) => {
        error!($($arg),+)
    };
}

#[cfg(feature = "i18n")]
macro_rules! fatal_l10n {
    ($($lang:tt => $($arg:tt),+);+ $(;)?) => {
        match crate::LOCALE.as_str() {
            $(
                $lang => fatal!($($arg),+),
            )+
        }
    };
}

#[cfg(not(feature = "i18n"))]
macro_rules! fatal_l10n {
    ($_a:tt => $($_b:tt),+ ; $($rest:tt)+) => {
        fatal_l10n!($($rest)+)
    };
    (_ => $($arg:tt),+ $(;)?) => {
        fatal!($($arg),+)
    };
}

#[cfg(feature = "i18n")]
macro_rules! eprintln_l10n {
    ($($lang:tt => $($arg:tt),+);+ $(;)?) => {
        match crate::LOCALE.as_str() {
            $(
                $lang => eprintln!($($arg),+),
            )+
        }
    };
}

#[cfg(not(feature = "i18n"))]
macro_rules! eprintln_l10n {
    ($_a:tt => $($_b:tt),+ ; $($rest:tt)+) => {
        eprintln_l10n!($($rest)+)
    };
    (_ => $($arg:tt),+ $(;)?) => {
        eprintln!($($arg),+)
    };
}
