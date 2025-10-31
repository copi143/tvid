use anyhow::Result;
use parking_lot::Mutex;
use std::ffi::c_void;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use crate::term::TERM_QUIT;

// @ ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== @
// @ 读取输入 @

#[cfg(unix)]
pub fn scan(bytes: &mut [u8]) -> isize {
    use libc::STDIN_FILENO;
    unsafe { libc::read(STDIN_FILENO, bytes.as_mut_ptr() as *mut c_void, bytes.len()) }
}

#[cfg(windows)]
pub fn scan(bytes: &mut [u8]) -> isize {
    use winapi::shared::minwindef::DWORD;
    use winapi::um::consoleapi::ReadConsoleA;
    use winapi::um::processenv::GetStdHandle;
    use winapi::um::winbase::STD_INPUT_HANDLE;
    unsafe {
        let handle = GetStdHandle(STD_INPUT_HANDLE);
        let mut read = 0u32;
        let res = ReadConsoleA(
            handle,
            bytes.as_mut_ptr() as *mut c_void,
            bytes.len() as DWORD,
            &mut read,
            std::ptr::null_mut(),
        );
        if res == 0 { -1 } else { read as isize }
    }
}

static STDIN_QUIT: AtomicBool = AtomicBool::new(false);

#[allow(static_mut_refs)]
fn getc() -> Result<u8> {
    static mut STDIN_BUF: [u8; 4096] = [0; 4096];
    static mut STDIN_POS: usize = 0;
    static mut STDIN_LEN: usize = 0;
    unsafe {
        if STDIN_POS < STDIN_LEN {
            let c = *STDIN_BUF.get_unchecked(STDIN_POS);
            STDIN_POS += 1;
            Ok(c)
        } else {
            let mut n = scan(&mut STDIN_BUF);
            while n == 0 && STDIN_QUIT.load(Ordering::SeqCst) == false {
                std::thread::sleep(Duration::from_millis(10));
                n = scan(&mut STDIN_BUF);
            }
            if STDIN_QUIT.load(Ordering::SeqCst) {
                return Err(anyhow::anyhow!("stdin quit"));
            }
            if n > 0 {
                STDIN_POS = 1;
                STDIN_LEN = n as usize;
                Ok(STDIN_BUF[0])
            } else {
                send_error!("Failed to read from stdin, ret = {}", n);
                Err(anyhow::anyhow!("failed to read from stdin"))
            }
        }
    }
}

// @ ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== @
// @ 键盘事件 @

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Key {
    Normal(char),
    Lower(char),
    Upper(char),
    Ctrl(char),
    Alt(char),
    CtrlAlt(char),
    AltShift(char),
    Fn(i32),
    Up,
    Down,
    Left,
    Right,
    Home,
    End,
    PageUp,
    PageDown,
    Insert,
    Delete,
    Backspace,
    Tab,
    Enter,
}

impl Key {
    pub fn to_u16(self) -> u16 {
        match self {
            Key::Normal(c) => match c as u16 {
                0..128 => c as u16,
                _ => panic!("Invalid normal key: {}", c),
            },
            Key::Lower(c) => match c {
                'a'..='z' => c as u16 - b'a' as u16 + 128,
                'A'..='Z' => c as u16 - b'A' as u16 + 128,
                _ => panic!("Invalid lower key: {}", c),
            },
            Key::Upper(c) => match c {
                'a'..='z' => c as u16 - b'a' as u16 + 128 + 26,
                'A'..='Z' => c as u16 - b'A' as u16 + 128 + 26,
                _ => panic!("Invalid upper key: {}", c),
            },
            Key::Ctrl(c) => match c {
                'a'..='z' => c as u16 - b'a' as u16 + 128 + 26 * 2,
                'A'..='Z' => c as u16 - b'A' as u16 + 128 + 26 * 2,
                _ => panic!("Invalid ctrl key: {}", c),
            },
            Key::CtrlAlt(c) => match c {
                'a'..='z' => c as u16 - b'a' as u16 + 128 + 26 * 3,
                'A'..='Z' => c as u16 - b'A' as u16 + 128 + 26 * 3,
                _ => panic!("Invalid ctrl-alt key: {}", c),
            },
            Key::Alt(c) => match c {
                'a'..='z' => c as u16 - b'a' as u16 + 128 + 26 * 4,
                'A'..='Z' => c as u16 - b'A' as u16 + 128 + 26 * 4,
                _ => panic!("Invalid alt key: {}", c),
            },
            Key::AltShift(c) => match c {
                'a'..='z' => c as u16 - b'a' as u16 + 128 + 26 * 5,
                'A'..='Z' => c as u16 - b'A' as u16 + 128 + 26 * 5,
                _ => panic!("Invalid alt-shift key: {}", c),
            },
            Key::Fn(n) => match n {
                1..=12 => (n - 1) as u16 + 128 + 26 * 6,
                _ => panic!("Invalid function key: F{}", n),
            },
            _ => {
                384 + match self {
                    Key::Normal(_) => unreachable!(),
                    Key::Lower(_) => unreachable!(),
                    Key::Upper(_) => unreachable!(),
                    Key::Ctrl(_) => unreachable!(),
                    Key::CtrlAlt(_) => unreachable!(),
                    Key::Alt(_) => unreachable!(),
                    Key::AltShift(_) => unreachable!(),
                    Key::Fn(_) => unreachable!(),
                    Key::Up => 0,
                    Key::Down => 1,
                    Key::Left => 2,
                    Key::Right => 3,
                    Key::Home => 4,
                    Key::End => 5,
                    Key::PageUp => 6,
                    Key::PageDown => 7,
                    Key::Insert => 8,
                    Key::Delete => 9,
                    Key::Backspace => 10,
                    Key::Tab => 11,
                    Key::Enter => 12,
                }
            }
        }
    }
}

impl From<Key> for u16 {
    fn from(key: Key) -> Self {
        key.to_u16()
    }
}

impl From<Key> for usize {
    fn from(key: Key) -> Self {
        key.to_u16() as usize
    }
}

// @ ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== @
// @ 键盘回调 @

pub type KeypressCallback = Box<dyn Fn(Key) -> bool + Send + Sync>;

pub struct KeypressCallbacks {
    cb: Mutex<Vec<KeypressCallback>>,
}

impl KeypressCallbacks {
    pub const fn new() -> Self {
        KeypressCallbacks {
            cb: Mutex::new(Vec::new()),
        }
    }

    pub fn push(&self, f: KeypressCallback) {
        self.cb.lock().push(f);
    }

    pub fn call(&self, k: Key) -> bool {
        for f in self.cb.lock().iter().rev() {
            if f(k) {
                return true;
            }
        }
        false
    }
}

static KEYPRESS_CALLBACKS: [KeypressCallbacks; 512] = [const { KeypressCallbacks::new() }; 512];

pub fn register_keypress_callback(k: Key, f: impl Fn(Key) -> bool + Send + Sync + 'static) {
    KEYPRESS_CALLBACKS[usize::from(k)].push(Box::new(f));
}

pub fn call_keypress_callbacks(c: Key) {
    KEYPRESS_CALLBACKS[usize::from(c)].call(c);
}

// @ ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== @
// @ 鼠标事件 @

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum MouseAction {
    Move,
    LeftDown,
    MiddleDown,
    RightDown,
    ScrollDown,
    LeftUp,
    MiddleUp,
    RightUp,
    ScrollUp,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Mouse {
    pub pos: (i32, i32),
    pub action: MouseAction,
    pub left: bool,
    pub middle: bool,
    pub right: bool,
}

impl Mouse {
    pub const fn new() -> Self {
        Mouse {
            pos: (0, 0),
            action: MouseAction::Move,
            left: false,
            middle: false,
            right: false,
        }
    }

    pub const fn update(&mut self, pos: (i32, i32), action: MouseAction) -> Self {
        self.pos = pos;
        self.action = action;
        match action {
            MouseAction::Move => {}
            MouseAction::LeftDown => {
                self.left = true;
            }
            MouseAction::LeftUp => {
                self.left = false;
            }
            MouseAction::MiddleDown => {
                self.middle = true;
            }
            MouseAction::MiddleUp => {
                self.middle = false;
            }
            MouseAction::RightDown => {
                self.right = true;
            }
            MouseAction::RightUp => {
                self.right = false;
            }
            MouseAction::ScrollUp => {}
            MouseAction::ScrollDown => {}
        }
        *self
    }
}

// @ ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== @
// @ 鼠标回调 @

pub type MouseCallback = Box<dyn Fn(Mouse) -> bool + Send + Sync>;

pub struct MouseCallbacks {
    cb: Mutex<Vec<MouseCallback>>,
}

impl MouseCallbacks {
    pub const fn new() -> Self {
        MouseCallbacks {
            cb: Mutex::new(Vec::new()),
        }
    }

    pub fn push(&self, f: MouseCallback) {
        self.cb.lock().push(f);
    }

    pub fn call(&self, m: Mouse) -> bool {
        for f in self.cb.lock().iter().rev() {
            if f(m) {
                return true;
            }
        }
        false
    }
}

static MOUSE_CALLBACKS: MouseCallbacks = MouseCallbacks::new();

pub fn register_mouse_callback(f: impl Fn(Mouse) -> bool + Send + Sync + 'static) {
    MOUSE_CALLBACKS.push(Box::new(f));
}

pub fn call_mouse_callbacks(m: Mouse) {
    MOUSE_CALLBACKS.call(m);
}

// @ ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== @
// @ 输入处理 @

fn input_parsenum(mut c: u8, end: u8) -> Result<i64> {
    let mut num = 0i64;
    while c != end {
        if c < b'0' || c > b'9' {
            return Err(anyhow::anyhow!("Invalid number: {}", c as char));
        }
        num = num * 10 + (c - b'0') as i64;
        c = getc()?;
    }
    Ok(num)
}

fn input_escape_square_number(num: i64) -> Result<()> {
    match num {
        1 => call_keypress_callbacks(Key::Home),
        2 => call_keypress_callbacks(Key::Insert),
        3 => call_keypress_callbacks(Key::Delete),
        4 => call_keypress_callbacks(Key::End),
        5 => call_keypress_callbacks(Key::PageUp),
        6 => call_keypress_callbacks(Key::PageDown),
        7 => call_keypress_callbacks(Key::Home),
        8 => call_keypress_callbacks(Key::End),
        11..=24 => {
            call_keypress_callbacks(Key::Fn(match num {
                11 => 1,
                12 => 2,
                13 => 3,
                14 => 4,
                15 => 5,
                17 => 6,
                18 => 7,
                19 => 8,
                20 => 9,
                21 => 10,
                23 => 11,
                24 => 12,
                _ => {
                    send_error!("Unknown escape sequence: ESC [ {} ~", num);
                    return Ok(());
                }
            }));
        }
        200 => {
            let mut data = Vec::new();
            while !data.ends_with(b"\x1b[201~") {
                data.push(getc()?);
            }
        }
        _ => {
            send_error!("Unknown escape sequence: ESC [ {} ~", num);
        }
    }
    Ok(())
}

fn input_escape_square_angle() -> Result<()> {
    let (params, mouseup) = {
        let mut s = String::new();
        let mouseup = loop {
            match getc()? {
                c if (b'0' <= c && c <= b'9') || c == b';' => s.push(c as char),
                b'M' => break false,
                b'm' => break true,
                _ => {
                    send_error!("Invalid mouse sequence: ESC < ...");
                    return Ok(());
                }
            }
        };
        (s.split(';').map(String::from).collect::<Vec<_>>(), mouseup)
    };
    if params.len() != 3 {
        send_error!("Invalid mouse sequence: ESC < ...");
        return Ok(());
    }
    let Ok(params) = params
        .iter()
        .map(|s| s.parse::<i32>())
        .collect::<Result<Vec<_>, _>>()
    else {
        send_error!("Invalid mouse sequence: ESC < ...");
        return Ok(());
    };
    let action = match params[0] {
        0 => {
            if mouseup {
                MouseAction::LeftUp
            } else {
                MouseAction::LeftDown
            }
        }
        1 => {
            if mouseup {
                MouseAction::MiddleUp
            } else {
                MouseAction::MiddleDown
            }
        }
        2 => {
            if mouseup {
                MouseAction::RightUp
            } else {
                MouseAction::RightDown
            }
        }
        32..36 => MouseAction::Move,
        64 => MouseAction::ScrollUp,
        65 => MouseAction::ScrollDown,
        _ => {
            send_error!(
                "Unknown mouse action: {}, button up: {}",
                params[0],
                mouseup
            );
            return Ok(());
        }
    };
    static mut MOUSE_STATE: Mouse = Mouse::new();
    #[allow(static_mut_refs)]
    call_mouse_callbacks(unsafe { MOUSE_STATE.update((params[1] - 1, params[2] - 1), action) });
    Ok(())
}

#[allow(non_snake_case)]
fn input_escape_square_M() -> Result<()> {
    let b1 = getc()?;
    let b2 = getc()?;
    let b3 = getc()?;
    let action = match b1 {
        0 => MouseAction::LeftDown,
        1 => MouseAction::MiddleDown,
        2 => MouseAction::RightDown,
        3 => MouseAction::LeftUp,
        4 => MouseAction::MiddleUp,
        5 => MouseAction::RightUp,
        64 => MouseAction::ScrollUp,
        65 => MouseAction::ScrollDown,
        _ => {
            // send_error!("Unknown mouse action: {}", b1);
            return Ok(());
        }
    };
    call_mouse_callbacks(Mouse {
        pos: (b2 as i32 - 33, b3 as i32 - 33),
        action,
        left: b1 == 0 || b1 == 3,
        middle: b1 == 1 || b1 == 4,
        right: b1 == 2 || b1 == 5,
    });
    Ok(())
}

fn input_escape_square() -> Result<()> {
    match getc()? {
        b'A' => call_keypress_callbacks(Key::Up),
        b'B' => call_keypress_callbacks(Key::Down),
        b'C' => call_keypress_callbacks(Key::Right),
        b'D' => call_keypress_callbacks(Key::Left),
        b'H' => call_keypress_callbacks(Key::Home),
        b'F' => call_keypress_callbacks(Key::End),
        c if b'0' <= c && c <= b'9' => {
            if let Ok(num) = input_parsenum(c, b'~') {
                input_escape_square_number(num)?;
            } else {
                send_error!("Invalid escape sequence: ESC [ <number> ~ (number parsing failed)");
            }
        }
        b'<' => input_escape_square_angle()?,
        b'M' => input_escape_square_M()?,
        c => {
            send_error!("Unknown escape sequence: ESC [ {} ({})", c as char, c);
            return Ok(());
        }
    }
    Ok(())
}

fn input_escape() -> Result<()> {
    match getc()? {
        c if 1 <= c && c <= 26 => {
            let c = (c - 1 + b'a') as char;
            call_keypress_callbacks(Key::CtrlAlt(c));
        }
        c if b'a' <= c && c <= b'z' => {
            let c = c as char;
            call_keypress_callbacks(Key::Alt(c));
        }
        c if b'A' <= c && c <= b'Z' => {
            let c = (c as char).to_ascii_lowercase();
            call_keypress_callbacks(Key::AltShift(c));
        }
        b'[' => input_escape_square()?,
        c => {
            send_error!("Unknown escape sequence: ESC {} ({})", c as char, c);
        }
    }
    Ok(())
}

fn input() -> Result<()> {
    match getc()? {
        0x1b => input_escape()?,
        b' ' => call_keypress_callbacks(Key::Normal(' ')),
        0x7f => call_keypress_callbacks(Key::Backspace),
        b'\n' | b'\r' => {
            call_keypress_callbacks(Key::Normal('\n'));
        }
        c if c >= b'a' && c <= b'z' => {
            call_keypress_callbacks(Key::Lower(c as char));
            call_keypress_callbacks(Key::Normal(c as char));
        }
        c if c >= b'A' && c <= b'Z' => {
            call_keypress_callbacks(Key::Upper(c as char));
            call_keypress_callbacks(Key::Normal(c as char));
        }
        c if c >= 1 && c <= 26 => {
            let c = (c - 1 + b'a') as char;
            call_keypress_callbacks(Key::Ctrl(c));
        }
        c => {
            send_error!("Unknown key: {} ({})", c as char, c);
        }
    }
    Ok(())
}

pub fn input_main() {
    while TERM_QUIT.load(Ordering::SeqCst) == false {
        if input().is_err() {
            break;
        }
    }
}

pub fn notify_quit() {
    STDIN_QUIT.store(true, Ordering::SeqCst);
}
