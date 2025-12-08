use parking_lot::Mutex;
use std::panic;
use std::process::exit;
use std::sync::atomic::{AtomicBool, AtomicI32, Ordering};

use crate::ffmpeg;
use crate::logging::print_messages;
use crate::util::*;
use crate::{stdin, stdout};

// @ ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== @

static NEXT_TERM_ID: AtomicI32 = AtomicI32::new(1);

pub fn next_term_id() -> i32 {
    NEXT_TERM_ID.fetch_add(1, Ordering::SeqCst)
}

// @ ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== @

#[derive(Debug, Clone, Copy)]
pub struct Winsize {
    pub row: u16,
    pub col: u16,
    pub xpixel: u16,
    pub ypixel: u16,
}

#[cfg(unix)]
pub fn get_winsize() -> Option<Winsize> {
    let mut winsize = std::mem::MaybeUninit::uninit();
    if unsafe { libc::ioctl(libc::STDOUT_FILENO, libc::TIOCGWINSZ, &mut winsize) } < 0 {
        return None;
    }
    let winsize: libc::winsize = unsafe { winsize.assume_init() };
    Some(Winsize {
        row: winsize.ws_row,
        col: winsize.ws_col,
        xpixel: winsize.ws_xpixel,
        ypixel: winsize.ws_ypixel,
    })
}

#[cfg(windows)]
pub fn get_winsize() -> Option<Winsize> {
    use winapi::shared::minwindef::BOOL;
    use winapi::um::processenv::GetStdHandle;
    use winapi::um::winbase::STD_OUTPUT_HANDLE;
    use winapi::um::wincon::{
        CONSOLE_FONT_INFOEX, CONSOLE_SCREEN_BUFFER_INFO, GetCurrentConsoleFontEx,
    };
    use winapi::um::winnt::HANDLE;
    unsafe extern "system" {
        pub fn GetConsoleScreenBufferInfo(
            hConsoleOutput: HANDLE,
            lpConsoleScreenBufferInfo: *mut CONSOLE_SCREEN_BUFFER_INFO,
        ) -> BOOL;
    }
    unsafe {
        let handle: HANDLE = GetStdHandle(STD_OUTPUT_HANDLE);
        let mut csbi: CONSOLE_SCREEN_BUFFER_INFO = std::mem::zeroed();
        if GetConsoleScreenBufferInfo(handle, &mut csbi) == 0 {
            return None;
        }

        let col = (csbi.srWindow.Right - csbi.srWindow.Left + 1) as u16;
        let row = (csbi.srWindow.Bottom - csbi.srWindow.Top + 1) as u16;

        // Try to get the actual font cell size in pixels. This works for classic conhost.
        // If the call fails (e.g. non-conhost terminals), fall back to a reasonable heuristic.
        let mut font_px_w: u16 = 8;
        let mut font_px_h: u16 = 16;
        let mut cfi: CONSOLE_FONT_INFOEX = std::mem::zeroed();
        cfi.cbSize = std::mem::size_of::<CONSOLE_FONT_INFOEX>() as u32;
        if GetCurrentConsoleFontEx as usize != 0 {
            // SAFETY: GetCurrentConsoleFontEx is available on modern Windows; it may still fail at runtime.
            if GetCurrentConsoleFontEx(handle, 0, &mut cfi) != 0 {
                // dwFontSize is a COORD (X=width, Y=height) in pixels
                font_px_w = cfi.dwFontSize.X as u16;
                font_px_h = cfi.dwFontSize.Y as u16;
                // guard against zero
                if font_px_w == 0 {
                    font_px_w = 8;
                }
                if font_px_h == 0 {
                    font_px_h = 16;
                }
            }
        }

        Some(Winsize {
            row,
            col,
            xpixel: col.saturating_mul(font_px_w),
            ypixel: row.saturating_mul(font_px_h),
        })
    }
}

// @ ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== @

pub const TERM_DEFAULT_FG: Color = Color::new(171, 178, 191);
pub const TERM_DEFAULT_BG: Color = Color::new(35, 39, 46);

pub static TERM_QUIT: AtomicBool = AtomicBool::new(false);

/// 初始化终端时开启的特性：
/// - 1049: 切换到备用缓冲区
/// - 25: 隐藏光标
/// - 1006: 启用 SGR 扩展的鼠标模式
/// - 1003: 启用所有鼠标移动事件
/// - 2004: 启用[括号粘贴](https://en.wikipedia.org/wiki/Bracketed-paste)模式
pub const TERM_INIT_SEQ: &[u8] = b"\x1b[?1049h\x1b[?25l\x1b[?1006h\x1b[?1003h\x1b[?2004h";
/// 关闭初始化时开启的特性，见 [`TERM_INIT_SEQ`]
pub const TERM_EXIT_SEQ: &[u8] = b"\x1b[?2004l\x1b[?1003l\x1b[?1006l\x1b[?25h\x1b[?1049l";

pub extern "C" fn request_quit() {
    TERM_QUIT.store(true, Ordering::SeqCst);
    ffmpeg::notify_quit();
    stdin::notify_quit();
    stdout::notify_quit();
}

#[cfg(unix)]
static ORIG_TERMIOS: Mutex<Option<libc::termios>> = Mutex::new(None);

/// 在初始化终端之前不能启动 stdin 和 stdout 线程
#[cfg(unix)]
pub fn init() {
    use libc::{STDIN_FILENO, STDOUT_FILENO};

    unsafe { libc::signal(libc::SIGINT, request_quit as usize) };

    stdout::print_all_sync(TERM_INIT_SEQ);

    unsafe { libc::setlocale(libc::LC_CTYPE, c"en_US.UTF-8".as_ptr() as *const _) };

    let mut termios = std::mem::MaybeUninit::uninit();
    if unsafe { libc::tcgetattr(STDIN_FILENO, termios.as_mut_ptr()) } == 0 {
        let mut termios = unsafe { termios.assume_init() };
        ORIG_TERMIOS.lock().replace(termios);
        termios.c_lflag &= !(libc::ECHO | libc::ICANON);
        termios.c_cc[libc::VMIN] = 0;
        termios.c_cc[libc::VTIME] = 1;
        unsafe { libc::tcsetattr(STDIN_FILENO, libc::TCSANOW, &termios) };
    }

    let flags = unsafe { libc::fcntl(STDOUT_FILENO, libc::F_GETFL, 0) };
    unsafe { libc::fcntl(STDOUT_FILENO, libc::F_SETFL, flags | libc::O_NONBLOCK) };

    unsafe { libc::tcflush(STDIN_FILENO, libc::TCIFLUSH) };

    setup_panic_handler();
}

/// 在初始化终端之前不能启动 stdin 和 stdout 线程
#[cfg(windows)]
pub fn init() {
    use winapi::um::consoleapi::{GetConsoleMode, SetConsoleMode};
    use winapi::um::processenv::GetStdHandle;
    use winapi::um::winbase::{STD_INPUT_HANDLE, STD_OUTPUT_HANDLE};
    use winapi::um::wincon::{CTRL_BREAK_EVENT, CTRL_C_EVENT, CTRL_CLOSE_EVENT};
    use winapi::um::wincon::{CTRL_LOGOFF_EVENT, CTRL_SHUTDOWN_EVENT};
    use winapi::um::wincon::{ENABLE_PROCESSED_INPUT, ENABLE_VIRTUAL_TERMINAL_PROCESSING};
    use winapi::um::winnt::HANDLE;
    unsafe extern "system" {
        fn SetConsoleCP(wCodePageID: u32) -> i32;
        fn SetConsoleOutputCP(wCodePageID: u32) -> i32;
        fn SetConsoleCtrlHandler(
            handler: Option<unsafe extern "system" fn(u32) -> i32>,
            add: i32,
        ) -> i32;
    }
    unsafe {
        SetConsoleCP(65001);
        SetConsoleOutputCP(65001);
        stdout::print(TERM_INIT_SEQ);

        unsafe extern "system" fn console_handler(ctrl_type: u32) -> i32 {
            match ctrl_type {
                CTRL_C_EVENT | CTRL_BREAK_EVENT | CTRL_CLOSE_EVENT | CTRL_LOGOFF_EVENT
                | CTRL_SHUTDOWN_EVENT => {
                    request_quit();
                    1
                }
                _ => 0,
            }
        }
        SetConsoleCtrlHandler(Some(console_handler), 1);

        let h_in: HANDLE = GetStdHandle(STD_INPUT_HANDLE);
        let mut mode: u32 = 0;
        if GetConsoleMode(h_in, &mut mode) != 0 {
            SetConsoleMode(h_in, ENABLE_PROCESSED_INPUT);
        }
        let h_out: HANDLE = GetStdHandle(STD_OUTPUT_HANDLE);
        let mut out_mode: u32 = 0;
        if GetConsoleMode(h_out, &mut out_mode) != 0 {
            SetConsoleMode(h_out, out_mode | ENABLE_VIRTUAL_TERMINAL_PROCESSING);
        }
    }
}

/// 在退出前必须终止 stdin 和 stdout 线程
#[cfg(unix)]
pub fn quit() -> ! {
    use libc::{STDIN_FILENO, STDOUT_FILENO};

    let flags = unsafe { libc::fcntl(STDOUT_FILENO, libc::F_GETFL, 0) };
    unsafe { libc::fcntl(STDOUT_FILENO, libc::F_SETFL, flags & !libc::O_NONBLOCK) };

    if let Some(termios) = ORIG_TERMIOS.lock().take() {
        unsafe { libc::tcsetattr(STDIN_FILENO, libc::TCSANOW, &termios) };
    }

    stdout::print_all_sync(TERM_EXIT_SEQ);

    print_messages().ok();

    unsafe { libc::tcflush(STDIN_FILENO, libc::TCIFLUSH) };

    exit(0);
}

/// 在退出前必须终止 stdin 和 stdout 线程
#[cfg(windows)]
pub fn quit() -> ! {
    stdout::print(TERM_EXIT_SEQ);
    print_messages().ok();
    exit(0);
}

fn setup_panic_handler() {
    panic::set_hook(Box::new(|info| {
        let msg = if let Some(s) = info.payload().downcast_ref::<&str>() {
            *s
        } else if let Some(s) = info.payload().downcast_ref::<String>() {
            s.as_str()
        } else {
            "Unknown panic"
        };
        let location = info
            .location()
            .map(|l| format!("{}:{}", l.file(), l.line()))
            .unwrap_or_default();
        fatal!("[panic] {msg} at {location}");
    }));
}

// @ ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== @
