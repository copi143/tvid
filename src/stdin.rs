use anyhow::{Result, bail};
use data_classes::derive::*;
use parking_lot::Mutex;
use std::collections::BTreeMap;
use std::ffi::c_void;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use crate::term::TERM_QUIT;

// @ ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== @
// @ 读取输入 @

/// 尝试从标准输入读取字节到缓冲区，返回实际读取的字节数
#[cfg(unix)]
#[must_use]
pub fn scan(bytes: &mut [u8]) -> Option<usize> {
    use libc::{EAGAIN, EWOULDBLOCK, STDIN_FILENO};
    let res = unsafe { libc::read(STDIN_FILENO, bytes.as_mut_ptr() as *mut c_void, bytes.len()) };
    if res >= 0 {
        Some(res as usize)
    } else {
        #[allow(unreachable_patterns)]
        match unsafe { *libc::__errno_location() } {
            EAGAIN | EWOULDBLOCK => Some(0),
            _ => None,
        }
    }
}

/// 尝试从标准输入读取字节到缓冲区，返回实际读取的字节数
#[cfg(windows)]
#[must_use]
pub fn scan(bytes: &mut [u8]) -> Option<usize> {
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
        if res == 0 { None } else { Some(read as usize) }
    }
}

static STDIN_QUIT: AtomicBool = AtomicBool::new(false);

fn try_getc() -> Result<Option<u8>> {
    struct Stdin {
        buf: [u8; 4096],
        pos: usize,
        len: usize,
    }
    static STDIN: Mutex<Stdin> = Mutex::new(Stdin {
        buf: [0; 4096],
        pos: 0,
        len: 0,
    });
    let mut lock = STDIN.lock();
    if lock.pos < lock.len {
        let &c = unsafe { lock.buf.get_unchecked(lock.pos) };
        lock.pos += 1;
        Ok(Some(c))
    } else {
        let Some(n) = scan(&mut lock.buf) else {
            error!("Failed to read from stdin");
            bail!("failed to read from stdin");
        };
        if STDIN_QUIT.load(Ordering::SeqCst) {
            return Err(anyhow::anyhow!("stdin quit"));
        }
        if n > 0 {
            lock.pos = 1;
            lock.len = n;
            Ok(Some(lock.buf[0]))
        } else {
            Ok(None)
        }
    }
}

pub type GetcInner = Box<dyn FnMut() -> Result<Option<u8>> + Send + Sync>;

struct Getc {
    id: i32,
    inner: GetcInner,
}

impl Getc {
    fn main() -> Self {
        Self {
            id: 0,
            inner: Box::new(try_getc),
        }
    }

    fn new(id: i32, inner: GetcInner) -> Self {
        Self { id, inner }
    }

    fn tryget(&mut self) -> Result<Option<u8>> {
        (self.inner)()
    }

    async fn timeout(&mut self, timeout: Duration) -> Result<Option<u8>> {
        let start = std::time::Instant::now();
        while start.elapsed() < timeout && STDIN_QUIT.load(Ordering::SeqCst) == false {
            match self.tryget()? {
                Some(c) => return Ok(Some(c)),
                None => tokio::time::sleep(Duration::from_millis(1)).await,
            }
        }
        if STDIN_QUIT.load(Ordering::SeqCst) {
            return Err(anyhow::anyhow!("stdin quit"));
        }
        Ok(None)
    }

    async fn wait(&mut self) -> Result<u8> {
        while STDIN_QUIT.load(Ordering::SeqCst) == false {
            match self.tryget()? {
                Some(c) => return Ok(c),
                None => tokio::time::sleep(Duration::from_millis(10)).await,
            }
        }
        Err(anyhow::anyhow!("stdin quit"))
    }
}

// @ ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== @
// @ 键盘事件 @

#[data(copy)]
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
    Escape,

    FileSeparator,
    GroupSeparator,
    RecordSeparator,
    UnitSeparator,

    ShiftTab,
}

impl Key {
    pub fn to_u16(self) -> u16 {
        match self {
            Key::Normal(c) => match c as u8 {
                b'a'..=b'z' => c as u16,
                b'A'..=b'Z' => c as u16 - b'A' as u16 + b'a' as u16,
                1..128 => c as u16,
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
                    Key::Escape => 13,

                    Key::FileSeparator => 14,
                    Key::GroupSeparator => 15,
                    Key::RecordSeparator => 16,
                    Key::UnitSeparator => 17,

                    Key::ShiftTab => 18,
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

/// 键盘回调函数类型
/// - `<- i32` 终端 ID
/// - `<- Key` 按键值
/// - `-> bool` 是否处理该按键事件，若返回 true 则停止后续回调调用
pub type KeypressCallback = Box<dyn Fn(i32, Key) -> bool + Send + Sync>;

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

    pub fn call(&self, id: i32, k: Key) -> bool {
        for f in self.cb.lock().iter().rev() {
            if f(id, k) {
                return true;
            }
        }
        false
    }
}

static KEYPRESS_CALLBACKS: [KeypressCallbacks; 512] = [const { KeypressCallbacks::new() }; 512];

pub fn register_keypress_callback(k: Key, f: impl Fn(i32, Key) -> bool + Send + Sync + 'static) {
    KEYPRESS_CALLBACKS[usize::from(k)].push(Box::new(f));
}

pub fn call_keypress_callbacks(id: i32, c: Key) {
    KEYPRESS_CALLBACKS[usize::from(c)].call(id, c);
}

/// 粘贴回调函数类型
/// - `<- i32` 终端 ID
/// - `<- &str` 粘贴的文本内容
/// - `-> bool` 是否处理该粘贴事件，若返回 true 则停止后续回调调用
pub type PasteCallback = Box<dyn Fn(i32, &str) -> bool + Send + Sync>;

static PASTE_CALLBACKS: Mutex<Vec<PasteCallback>> = Mutex::new(Vec::new());

pub fn register_paste_callback(f: impl Fn(i32, &str) -> bool + Send + Sync + 'static) {
    PASTE_CALLBACKS.lock().push(Box::new(f));
}

pub fn call_paste_callbacks(id: i32, data: &str) {
    for f in PASTE_CALLBACKS.lock().iter().rev() {
        if f(id, data) {
            break;
        }
    }
}

/// 输入回调函数类型
/// - `<- i32` 终端 ID
/// - `<- &str` 输入的文本内容
/// - `-> bool` 是否处理该输入事件，若返回 true 则停止后续回调调用
pub type InputCallback = Box<dyn Fn(i32, &str) -> bool + Send + Sync>;

static INPUT_CALLBACKS: Mutex<Vec<InputCallback>> = Mutex::new(Vec::new());

pub fn register_input_callback(f: impl Fn(i32, &str) -> bool + Send + Sync + 'static) {
    INPUT_CALLBACKS.lock().push(Box::new(f));
}

pub fn call_input_callbacks(id: i32, data: &str) {
    for f in INPUT_CALLBACKS.lock().iter().rev() {
        if f(id, data) {
            break;
        }
    }
}

// @ ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== @
// @ 鼠标事件 @

#[data(copy)]
pub enum MouseAction {
    Move,
    LeftDown,
    MiddleDown,
    RightDown,
    ScrollDown,
    Side1Down,
    Side2Down,
    Button8Down,
    Button9Down,
    Button10Down,
    Button11Down,
    LeftUp,
    MiddleUp,
    RightUp,
    ScrollUp,
    Side1Up,
    Side2Up,
    Button8Up,
    Button9Up,
    Button10Up,
    Button11Up,
}

impl MouseAction {
    pub const fn to_up(self) -> MouseAction {
        match self {
            MouseAction::LeftDown => MouseAction::LeftUp,
            MouseAction::MiddleDown => MouseAction::MiddleUp,
            MouseAction::RightDown => MouseAction::RightUp,
            MouseAction::Side1Down => MouseAction::Side1Up,
            MouseAction::Side2Down => MouseAction::Side2Up,
            MouseAction::Button8Down => MouseAction::Button8Up,
            MouseAction::Button9Down => MouseAction::Button9Up,
            MouseAction::Button10Down => MouseAction::Button10Up,
            MouseAction::Button11Down => MouseAction::Button11Up,
            _ => self,
        }
    }

    pub const fn to_down(self) -> MouseAction {
        match self {
            MouseAction::LeftUp => MouseAction::LeftDown,
            MouseAction::MiddleUp => MouseAction::MiddleDown,
            MouseAction::RightUp => MouseAction::RightDown,
            MouseAction::Side1Up => MouseAction::Side1Down,
            MouseAction::Side2Up => MouseAction::Side2Down,
            MouseAction::Button8Up => MouseAction::Button8Down,
            MouseAction::Button9Up => MouseAction::Button9Down,
            MouseAction::Button10Up => MouseAction::Button10Down,
            MouseAction::Button11Up => MouseAction::Button11Down,
            _ => self,
        }
    }

    pub const fn to(self, up: bool) -> MouseAction {
        if up { self.to_up() } else { self.to_down() }
    }
}

#[data(copy)]
pub struct Mouse {
    pub pos: (i32, i32),
    pub action: MouseAction,
    pub left: bool,
    pub middle: bool,
    pub right: bool,
    pub side1: bool,
    pub side2: bool,
    pub button8: bool,
    pub button9: bool,
    pub button10: bool,
    pub button11: bool,
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
}

impl Mouse {
    pub const fn new() -> Self {
        Mouse {
            pos: (0, 0),
            action: MouseAction::Move,
            left: false,
            middle: false,
            right: false,
            side1: false,
            side2: false,
            button8: false,
            button9: false,
            button10: false,
            button11: false,
            ctrl: false,
            alt: false,
            shift: false,
        }
    }

    pub const fn update(
        &mut self,
        pos: (i32, i32),
        action: MouseAction,
        keys: (bool, bool, bool),
    ) -> Self {
        self.pos = pos;
        self.action = action;
        self.ctrl = keys.0;
        self.alt = keys.1;
        self.shift = keys.2;
        match action {
            MouseAction::Move => {}
            MouseAction::LeftDown => self.left = true,
            MouseAction::LeftUp => self.left = false,
            MouseAction::MiddleDown => self.middle = true,
            MouseAction::MiddleUp => self.middle = false,
            MouseAction::RightDown => self.right = true,
            MouseAction::RightUp => self.right = false,
            MouseAction::ScrollUp => {}
            MouseAction::ScrollDown => {}
            MouseAction::Side1Down => self.side1 = true,
            MouseAction::Side1Up => self.side1 = false,
            MouseAction::Side2Down => self.side2 = true,
            MouseAction::Side2Up => self.side2 = false,
            MouseAction::Button8Down => self.button8 = true,
            MouseAction::Button8Up => self.button8 = false,
            MouseAction::Button9Down => self.button9 = true,
            MouseAction::Button9Up => self.button9 = false,
            MouseAction::Button10Down => self.button10 = true,
            MouseAction::Button10Up => self.button10 = false,
            MouseAction::Button11Down => self.button11 = true,
            MouseAction::Button11Up => self.button11 = false,
        }
        *self
    }
}

// @ ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== @
// @ 鼠标回调 @

/// 鼠标回调函数类型
/// - `<- i32` 终端 ID
/// - `<- Mouse` 鼠标事件
/// - `-> bool` 是否处理该鼠标事件，若返回 true 则停止
pub type MouseCallback = Box<dyn Fn(i32, Mouse) -> bool + Send + Sync>;

static MOUSE_CALLBACKS: Mutex<Vec<MouseCallback>> = Mutex::new(Vec::new());

pub fn register_mouse_callback(f: impl Fn(i32, Mouse) -> bool + Send + Sync + 'static) {
    MOUSE_CALLBACKS.lock().push(Box::new(f));
}

pub fn call_mouse_callbacks(id: i32, m: Mouse) {
    for f in MOUSE_CALLBACKS.lock().iter().rev() {
        if f(id, m) {
            break;
        }
    }
}

// @ ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== @
// @ 输入处理 @

async fn input_parsenum(getc: &mut Getc, mut c: u8, end: u8) -> Result<i64> {
    let mut num = 0i64;
    while c != end {
        if c < b'0' || c > b'9' {
            return Err(anyhow::anyhow!("Invalid number: {}", c as char));
        }
        num = num * 10 + (c - b'0') as i64;
        c = getc.wait().await?;
    }
    Ok(num)
}

async fn input_escape_square_number(getc: &mut Getc, num: i64) -> Result<()> {
    match num {
        1 => call_keypress_callbacks(getc.id, Key::Home),
        2 => call_keypress_callbacks(getc.id, Key::Insert),
        3 => call_keypress_callbacks(getc.id, Key::Delete),
        4 => call_keypress_callbacks(getc.id, Key::End),
        5 => call_keypress_callbacks(getc.id, Key::PageUp),
        6 => call_keypress_callbacks(getc.id, Key::PageDown),
        7 => call_keypress_callbacks(getc.id, Key::Home),
        8 => call_keypress_callbacks(getc.id, Key::End),
        11..=24 => {
            call_keypress_callbacks(
                getc.id,
                Key::Fn(match num {
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
                        error!("Unknown escape sequence: ESC [ {} ~", num);
                        return Ok(());
                    }
                }),
            );
        }
        200 => {
            let mut data = Vec::new();
            while !data.ends_with(b"\x1b[201~") {
                data.push(getc.wait().await?);
            }
            let data = &data[..data.len() - 6];
            if let Ok(s) = std::str::from_utf8(data) {
                call_paste_callbacks(getc.id, s);
            } else {
                error_l10n!(
                    "zh-cn" => "无效的粘贴数据（非 UTF-8 编码）";
                    "zh-tw" => "無效的貼上資料（非 UTF-8 編碼）";
                    "ja-jp" => "無効なペーストデータ（UTF-8 エンコードではありません）";
                    "fr-fr" => "Données collées invalides (non encodées en UTF-8)";
                    "de-de" => "Ungültige Einfügedaten (nicht UTF-8-codiert)";
                    "es-es" => "Datos pegados no válidos (no codificados en UTF-8)";
                    _       => "Invalid paste data (not UTF-8 encoded)";
                );
            }
        }
        _ => {
            error_l10n!(
                "zh-cn" => "未知的转义序列：ESC [ {num} ~";
                "zh-tw" => "未知的轉義序列：ESC [ {num} ~";
                "ja-jp" => "不明なエスケープシーケンス：ESC [ {num} ~";
                "fr-fr" => "Séquence d'échappement inconnue : ESC [ {num} ~";
                "de-de" => "Unbekannte Escape-Sequenz: ESC [ {num} ~";
                "es-es" => "Secuencia de escape desconocida: ESC [ {num} ~";
                _       => "Unknown escape sequence: ESC [ {num} ~";
            );
        }
    }
    Ok(())
}

/// 鼠标事件的二进制表示：
/// - `xxx m c a s bb`
/// - `x`: 扩展 3，额外按键，此时 `bb` 为 0 到 3 代表按钮 8 到 11 按下
/// - `x`: 扩展 2，侧键，此时 `bb` 为 0 (X1 返回) 或 1 (X2 前进)
/// - `x`: 扩展 1，滚动，此时 `bb` 为 0 (向上) 或 1 (向下)
/// - `m`: 移动
/// - `c`: ctrl 键是否按下
/// - `a`: alt 键是否按下
/// - `s`: shift 键是否按下
/// - `bb`: 按钮编号
async fn input_escape_square_angle(getc: &mut Getc) -> Result<()> {
    let (params, mouseup) = {
        let mut s = String::new();
        let mouseup = loop {
            match getc.wait().await? {
                c if (b'0' <= c && c <= b'9') || c == b';' => s.push(c as char),
                b'M' => break false,
                b'm' => break true,
                _ => {
                    error!("Invalid mouse sequence: ESC < ...");
                    return Ok(());
                }
            }
        };
        (s.split(';').map(String::from).collect::<Vec<_>>(), mouseup)
    };
    if params.len() != 3 {
        error!("Invalid mouse sequence: ESC < ...");
        return Ok(());
    }
    let Ok(params) = params
        .iter()
        .map(|s| s.parse::<i32>())
        .collect::<Result<Vec<_>, _>>()
    else {
        error!("Invalid mouse sequence: ESC < ...");
        return Ok(());
    };
    let pc = (params[0] & 0b10000) != 0;
    let pa = (params[0] & 0b1000) != 0;
    let ps = (params[0] & 0b100) != 0;
    let action = match params[0] & 0b111100011 {
        0 => MouseAction::LeftDown,
        1 => MouseAction::MiddleDown,
        2 => MouseAction::RightDown,
        32..36 => MouseAction::Move,
        64 => MouseAction::ScrollUp,
        65 => MouseAction::ScrollDown,
        128 => MouseAction::Side1Down,
        129 => MouseAction::Side2Down,
        256 => MouseAction::Button8Down,
        257 => MouseAction::Button9Down,
        258 => MouseAction::Button10Down,
        259 => MouseAction::Button11Down,
        _ => {
            error!(
                "Unknown mouse action: (0b{:09b}), button up: {}",
                params[0], mouseup
            );
            return Ok(());
        }
    };
    let action = action.to(mouseup);
    let state = {
        static MOUSE_STATE: Mutex<BTreeMap<i32, Mouse>> = Mutex::new(BTreeMap::new());
        let mut state = MOUSE_STATE.lock();
        let state = state.entry(getc.id).or_insert_with(Mouse::new);
        state.update((params[1] - 1, params[2] - 1), action, (pc, pa, ps))
    };
    call_mouse_callbacks(getc.id, state);
    Ok(())
}

#[allow(non_snake_case)]
async fn input_escape_square_M(getc: &mut Getc) -> Result<()> {
    let b1 = getc.wait().await? as i32;
    let b2 = getc.wait().await? as i32 - 32;
    let b3 = getc.wait().await? as i32 - 32;
    if b2 < 0 || b3 < 0 {
        error!("Invalid mouse sequence: ESC [ M {} {} {}", b1, b2, b3);
        return Ok(());
    }
    let pc = (b1 & 0b10000) != 0;
    let pa = (b1 & 0b1000) != 0;
    let ps = (b1 & 0b100) != 0;
    let mouseup = (b1 & 0b11) == 0b11;
    let action = match b1 & 0b11100011 {
        0..4 => MouseAction::Move,
        32 => MouseAction::LeftDown,
        33 => MouseAction::MiddleDown,
        34 => MouseAction::RightDown,
        35 => MouseAction::Move, // 这种情况会被解析成 mouseup 在下方处理
        64 => MouseAction::ScrollUp,
        65 => MouseAction::ScrollDown,
        128 => MouseAction::Side1Down,
        129 => MouseAction::Side2Down,
        _ => {
            error!("Unknown mouse action: (0b{:09b})", b1);
            return Ok(());
        }
    };
    let mut args = Vec::new();
    {
        static MOUSE_STATE: Mutex<BTreeMap<i32, Mouse>> = Mutex::new(BTreeMap::new());
        let mut state = MOUSE_STATE.lock();
        let state = state.entry(getc.id).or_insert_with(Mouse::new);
        if mouseup && state.left {
            args.push(state.update((b2 - 1, b3 - 1), MouseAction::LeftUp, (pc, pa, ps)));
        }
        if mouseup && state.middle {
            args.push(state.update((b2 - 1, b3 - 1), MouseAction::MiddleUp, (pc, pa, ps)));
        }
        if mouseup && state.right {
            args.push(state.update((b2 - 1, b3 - 1), MouseAction::RightUp, (pc, pa, ps)));
        }
        if mouseup && state.side1 {
            args.push(state.update((b2 - 1, b3 - 1), MouseAction::Side1Up, (pc, pa, ps)));
        }
        if mouseup && state.side2 {
            args.push(state.update((b2 - 1, b3 - 1), MouseAction::Side2Up, (pc, pa, ps)));
        }
        if !mouseup {
            args.push(state.update((b2 - 1, b3 - 1), action, (pc, pa, ps)));
        }
    }
    args.iter()
        .for_each(|&state| call_mouse_callbacks(getc.id, state));
    Ok(())
}

async fn input_escape_square(getc: &mut Getc) -> Result<()> {
    match getc.wait().await? {
        b'A' => call_keypress_callbacks(getc.id, Key::Up),
        b'B' => call_keypress_callbacks(getc.id, Key::Down),
        b'C' => call_keypress_callbacks(getc.id, Key::Right),
        b'D' => call_keypress_callbacks(getc.id, Key::Left),
        b'H' => call_keypress_callbacks(getc.id, Key::Home),
        b'F' => call_keypress_callbacks(getc.id, Key::End),
        b'Z' => call_keypress_callbacks(getc.id, Key::ShiftTab),
        c if b'0' <= c && c <= b'9' => {
            if let Ok(num) = input_parsenum(getc, c, b'~').await {
                input_escape_square_number(getc, num).await?;
            } else {
                error!("Invalid escape sequence: ESC [ <number> ~ (number parsing failed)");
            }
        }
        b'<' => input_escape_square_angle(getc).await?,
        b'M' => input_escape_square_M(getc).await?,
        c => {
            error!("Unknown escape sequence: ESC [ {} ({})", c as char, c);
            return Ok(());
        }
    }
    Ok(())
}

async fn input_escape(getc: &mut Getc) -> Result<()> {
    let Some(c) = getc.timeout(Duration::from_millis(20)).await? else {
        call_keypress_callbacks(getc.id, Key::Escape);
        return Ok(());
    };
    match c {
        c if 1 <= c && c <= 26 => {
            let c = (c - 1 + b'a') as char;
            call_keypress_callbacks(getc.id, Key::CtrlAlt(c));
        }
        c if b'a' <= c && c <= b'z' => {
            let c = c as char;
            call_keypress_callbacks(getc.id, Key::Alt(c));
        }
        c if b'A' <= c && c <= b'Z' => {
            let c = (c as char).to_ascii_lowercase();
            call_keypress_callbacks(getc.id, Key::AltShift(c));
        }
        b'[' => input_escape_square(getc).await?,
        c => {
            error_l10n!(
                "zh-cn" => "未知的转义序列：ESC {} ({})", (c as char), c;
                "zh-tw" => "未知的轉義序列：ESC {} ({})", (c as char), c;
                "ja-jp" => "不明なエスケープシーケンス：ESC {} ({})", (c as char), c;
                "fr-fr" => "Séquence d'échappement inconnue : ESC {} ({})", (c as char), c;
                "de-de" => "Unbekannte Escape-Sequenz: ESC {} ({})", (c as char), c;
                "es-es" => "Secuencia de escape desconocida: ESC {} ({})", (c as char), c;
                _ => "Unknown escape sequence: ESC {} ({})", (c as char), c;
            );
        }
    }
    Ok(())
}

async fn input(getc: &mut Getc) -> Result<()> {
    let c = getc.wait().await?;
    match c {
        0 => warning_l10n!(
            "zh-cn" => "未处理的按键：NUL";
            "zh-tw" => "未處理的按鍵：NUL";
            "ja-jp" => "未処理のキー：NUL";
            "fr-fr" => "Touche non gérée : NUL";
            "de-de" => "Unbehandelter Schlüssel: NUL";
            "es-es" => "Tecla no manejada: NUL";
            _       => "Unhandled key: NUL";
        ),
        0x1b => input_escape(getc).await?,
        b' ' => call_keypress_callbacks(getc.id, Key::Normal(' ')),
        0x7f => call_keypress_callbacks(getc.id, Key::Backspace),
        b'\n' | b'\r' => {
            call_keypress_callbacks(getc.id, Key::Normal('\n'));
        }
        b'a'..=b'z' => {
            call_keypress_callbacks(getc.id, Key::Lower(c as char));
            call_keypress_callbacks(getc.id, Key::Normal(c as char));
        }
        b'A'..=b'Z' => {
            call_keypress_callbacks(getc.id, Key::Upper(c as char));
            call_keypress_callbacks(getc.id, Key::Normal(c as char));
        }
        1..=26 => {
            let c = (c - 1 + b'a') as char;
            call_keypress_callbacks(getc.id, Key::Ctrl(c));
        }
        0x1c => call_keypress_callbacks(getc.id, Key::FileSeparator),
        0x1d => call_keypress_callbacks(getc.id, Key::GroupSeparator),
        0x1e => call_keypress_callbacks(getc.id, Key::RecordSeparator),
        0x1f => call_keypress_callbacks(getc.id, Key::UnitSeparator),
        33..=126 => {
            call_keypress_callbacks(getc.id, Key::Normal(c as char));
        }
        128.. => warning_l10n!(
            "zh-cn" => "未处理的按键：{} ({})", (c as char), c;
            "zh-tw" => "未處理的按鍵：{} ({})", (c as char), c;
            "ja-jp" => "未処理のキー：{} ({})", (c as char), c;
            "fr-fr" => "Touche non gérée : {} ({})", (c as char), c;
            "de-de" => "Unbehandelter Schlüssel: {} ({})", (c as char), c;
            "es-es" => "Tecla no manejada: {} ({})", (c as char), c;
            _       => "Unhandled key: {} ({})", (c as char), c;
        ),
    }
    Ok(())
}

/// 主输入处理循环（处理本地终端输入）
pub async fn input_main() {
    let mut getc = Getc::main();
    while TERM_QUIT.load(Ordering::SeqCst) == false {
        if input(&mut getc).await.is_err() {
            break;
        }
    }
}

/// 输入处理任务（通过回调获取输入）
pub async fn input_task(id: i32, getc: GetcInner) {
    let mut getc = Getc::new(id, getc);
    while TERM_QUIT.load(Ordering::SeqCst) == false {
        if input(&mut getc).await.is_err() {
            break;
        }
    }
}

pub fn notify_quit() {
    STDIN_QUIT.store(true, Ordering::SeqCst);
}
