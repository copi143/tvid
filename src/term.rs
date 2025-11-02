use parking_lot::Mutex;
use std::panic;
use std::process::exit;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};
use unicode_width::UnicodeWidthChar;

use crate::audio;
use crate::error::print_errors;
use crate::ffmpeg;
use crate::playlist::PLAYLIST;
use crate::stdin;
use crate::stdout::{self, pend_print, pending_frames, remove_pending_frames};
use crate::util::*;
use crate::{TOKIO_RUNTIME, statistics};

// @ ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== @

pub struct RenderWrapper<'frame, 'cells> {
    pub frame: &'frame [Color],
    pub frame_width: usize,
    pub frame_height: usize,
    pub frame_pitch: usize,

    pub cells: &'cells mut [Cell],
    pub cells_width: usize,
    pub cells_height: usize,
    pub cells_pitch: usize,

    pub padding_left: usize,
    pub padding_right: usize,
    pub padding_top: usize,
    pub padding_bottom: usize,

    pub pixels_width: usize,
    pub pixels_height: usize,

    pub playing: String,
    pub played_time: Option<Duration>,
    pub delta_time: Duration,

    pub term_font_width: f32,
    pub term_font_height: f32,
}

impl RenderWrapper<'_, '_> {
    pub fn pixel_at(&self, x: usize, y: usize) -> Color {
        self.frame[y * self.frame_pitch + x]
    }

    pub fn cell_at(&self, x: usize, y: usize) -> &Cell {
        &self.cells[y * self.cells_pitch + x]
    }

    pub fn cell_mut_at(&mut self, x: usize, y: usize) -> &mut Cell {
        &mut self.cells[y * self.cells_pitch + x]
    }
}

/// frames.0 - 当前帧, frames.1 - 上一帧
static FRAMES: Mutex<(Vec<Cell>, Vec<Cell>)> = Mutex::new((Vec::new(), Vec::new()));
static RENDER_CALLBACKS: Mutex<Vec<fn(&mut RenderWrapper)>> = Mutex::new(Vec::new());

pub static COLOR_MODE: Mutex<ColorMode> = Mutex::new(ColorMode::new());

pub fn add_render_callback(callback: fn(&mut RenderWrapper<'_, '_>)) {
    RENDER_CALLBACKS.lock().push(callback);
}

pub fn render(frame: &[Color], pitch: usize) {
    static LAST_TIME: Mutex<Option<Duration>> = Mutex::new(None);

    let played_time = audio::played_time_or_none();

    let delta_time = LAST_TIME
        .lock()
        .map(|t1| played_time.map(|t2| t2.saturating_sub(t1)))
        .flatten()
        .unwrap_or(Duration::ZERO);

    if let Some(played_time) = played_time {
        LAST_TIME.lock().replace(played_time);
    }

    render_frame(frame, pitch, played_time, delta_time);

    let mut force_flush = FORCEFLUSH_NEXT.swap(false, Ordering::SeqCst);
    if pending_frames() > 3 {
        send_error!("Too many pending frames: {}", pending_frames());
        force_flush = true;
    }
    print_diff(force_flush);

    let (this_frame, last_frame) = &mut *FRAMES.lock();
    std::mem::swap(this_frame, last_frame);
}

fn render_frame(
    frame: &[Color],
    pitch: usize,
    played_time: Option<Duration>,
    delta_time: Duration,
) {
    let (this_frame, _) = &mut *FRAMES.lock();

    let wrap = &mut RenderWrapper {
        frame,
        frame_width: VIDEO_PIXELS.x(),
        frame_height: VIDEO_PIXELS.y(),
        frame_pitch: pitch,
        cells: this_frame.as_mut_slice(),
        cells_width: TERM_SIZE.x(),
        cells_height: TERM_SIZE.y(),
        cells_pitch: TERM_SIZE.x(),
        padding_left: VIDEO_PADDING.left(),
        padding_right: VIDEO_PADDING.right(),
        padding_top: VIDEO_PADDING.top(),
        padding_bottom: VIDEO_PADDING.bottom(),
        pixels_width: VIDEO_PIXELS.x(),
        pixels_height: VIDEO_PIXELS.y(),
        playing: { PLAYLIST.lock().current().cloned() }.unwrap_or_default(),
        played_time,
        delta_time,
        term_font_width: TERM_PIXELS.x() as f32 / TERM_SIZE.x() as f32,
        term_font_height: TERM_PIXELS.y() as f32 / TERM_SIZE.y() as f32,
    };

    let instant = Instant::now();
    for callback in RENDER_CALLBACKS.lock().iter() {
        callback(wrap);
    }
    statistics::set_render_time(instant.elapsed());
}

async fn print_diff_line(
    cells: &mut [Cell],
    lasts: &[Cell],
    force_flush: bool,
    color_mode: ColorMode,
) -> Vec<u8> {
    let mut last_bg = Color::transparent();
    let mut last_fg = Color::transparent();
    let mut buf = Vec::with_capacity(1024);
    let mut skip_count = 0u32;
    for (cell, last) in cells.iter().zip(lasts.iter()) {
        if cell.c == Some('\0') {
            continue;
        }

        let cw = cell.c.map_or(1, |c| c.width().unwrap_or(1).max(1));
        if !force_flush && cell == last && cw == 1 {
            skip_count += 1;
            continue;
        } else if skip_count == 1 {
            buf.extend_from_slice(b"\x1b[C");
            skip_count = 0;
        } else if skip_count > 1 {
            buf.extend_from_slice(format!("\x1b[{skip_count}C").as_bytes());
            skip_count = 0;
        }

        let (fg, bg) = (some_if_ne(cell.fg, last_fg), some_if_ne(cell.bg, last_bg));

        buf.extend_from_slice(escape_set_color(fg, bg, color_mode).as_bytes());
        buf.extend_from_slice(cell.c.unwrap_or('▄').to_string().as_bytes());

        last_fg = cell.fg;
        last_bg = cell.bg;
    }
    buf
}

async fn print_diff_async(
    force_flush: bool,
    cells: Vec<&'static mut [Cell]>,
    lasts: Vec<&'static [Cell]>,
) {
    let color_mode = *COLOR_MODE.lock();
    let instant = Instant::now();

    let result = (cells.into_iter().zip(lasts.into_iter()))
        .map(|(cell, last)| tokio::spawn(print_diff_line(cell, last, force_flush, color_mode)))
        .collect::<Vec<_>>()
        .join_all()
        .await;

    let mut buf = Vec::with_capacity(65536);
    buf.extend_from_slice(b"\x1b[H");
    for (i, line) in result.into_iter().enumerate() {
        if i != 0 {
            buf.extend_from_slice(b"\x1b[E");
        }
        buf.extend_from_slice(&line);
    }

    statistics::set_escape_string_encode_time(instant.elapsed());
    if force_flush {
        remove_pending_frames();
        assert!(buf.len() > 0, "force flush but buffer is empty");
        pend_print(buf);
    } else if buf.len() > 0 {
        pend_print(buf);
    }
}

fn print_diff(force_flush: bool) {
    let (term_width, term_height) = TERM_SIZE.get();
    let (this_frame, last_frame) = &mut *FRAMES.lock();

    let cells = unsafe {
        let real_len = this_frame.len() - 1;
        this_frame
            .as_mut_slice()
            .split_at_mut(real_len)
            .0
            .chunks_mut(term_width)
            .map(|chunk| std::mem::transmute(chunk))
            .collect::<Vec<_>>()
    };
    let lasts = unsafe {
        let real_len = last_frame.len() - 1;
        last_frame
            .as_slice()
            .split_at(real_len)
            .0
            .chunks(term_width)
            .map(|chunk| std::mem::transmute(chunk))
            .collect::<Vec<_>>()
    };

    assert!(cells.len() == term_height, "cells length mismatch");
    assert!(lasts.len() == term_height, "lasts length mismatch");

    TOKIO_RUNTIME.block_on(print_diff_async(force_flush, cells, lasts));
}

// @ ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== @

pub static VIDEO_ORIGIN_PIXELS_NOW: XY = XY::new();
pub static VIDEO_ORIGIN_PIXELS: XY = XY::new();

pub static TERM_SIZE: XY = XY::new();
pub static TERM_PIXELS: XY = XY::new();
pub static VIDEO_PIXELS: XY = XY::new();
pub static VIDEO_PADDING: TBLR = TBLR::new();

/// 强制下一帧全屏刷新
pub static FORCEFLUSH_NEXT: AtomicBool = AtomicBool::new(false);

pub fn updatesize() -> bool {
    let Some(winsize) = get_winsize() else {
        return false;
    };
    let (xchars, ychars) = (winsize.col as usize, winsize.row as usize);
    let (xpixels, ypixels) = (winsize.xpixel as usize, winsize.ypixel as usize);
    let (xvideo, yvideo) = VIDEO_ORIGIN_PIXELS_NOW.get();
    assert!(xvideo > 0 && yvideo > 0, "video size is zero");
    let (xchars, ychars) = if xchars == 0 || ychars == 0 {
        (80, 24)
    } else {
        (xchars, ychars)
    };
    let (xpixels, ypixels) = if xpixels == 0 || ypixels == 0 {
        (xchars * 8, ychars * 16)
    } else {
        (xpixels, ypixels)
    };

    if TERM_SIZE.x() == xchars && TERM_SIZE.y() == ychars {
        if TERM_PIXELS.x() == xpixels && TERM_PIXELS.y() == ypixels {
            if VIDEO_ORIGIN_PIXELS.x() == xvideo && VIDEO_ORIGIN_PIXELS.y() == yvideo {
                return false;
            }
        }
    }

    TERM_SIZE.set(xchars, ychars);
    TERM_PIXELS.set(xpixels, ypixels);
    VIDEO_ORIGIN_PIXELS.set(xvideo, yvideo);

    match xvideo as i64 * ypixels as i64 - yvideo as i64 * xpixels as i64 {
        0 => {
            VIDEO_PADDING.set(0, 0, 0, 0);
            VIDEO_PIXELS.set(xchars, ychars * 2);
        }
        1..=i64::MAX => {
            let y = (ychars * xpixels * yvideo) / (xvideo * ypixels);
            let p = (ychars - y) / 2;
            VIDEO_PADDING.set(p, p, 0, 0);
            VIDEO_PIXELS.set(xchars, (ychars - p * 2) * 2);
        }
        i64::MIN..=-1 => {
            let x = (xchars * ypixels * xvideo) / (yvideo * xpixels);
            let p = (xchars - x) / 2;
            VIDEO_PADDING.set(0, 0, p, p);
            VIDEO_PIXELS.set(xchars - p * 2, ychars * 2);
        }
    }

    {
        let (this_frame, last_frame) = &mut *FRAMES.lock();

        // 将大小加一作为哨兵
        this_frame.clear();
        this_frame.resize(xchars * ychars + 1, Cell::default());

        // 将大小加一作为哨兵
        last_frame.clear();
        last_frame.resize(xchars * ychars + 1, Cell::default());
    }

    remove_pending_frames();

    FORCEFLUSH_NEXT.store(true, Ordering::SeqCst);
    true
}

// @ ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== @

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

/// - 1049: 切换到备用缓冲区
/// - 25: 隐藏光标
/// - 1006: 启用 SGR 扩展的鼠标模式
/// - 1003: 启用所有鼠标移动事件
/// - 2004: 启用括号粘贴模式
const TERM_INIT_SEQ: &[u8] = b"\x1b[?1049h\x1b[?25l\x1b[?1006h\x1b[?1003h\x1b[?2004h";
const TERM_EXIT_SEQ: &[u8] = b"\x1b[?2004l\x1b[?1003l\x1b[?1006l\x1b[?25h\x1b[?1049l";

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
    use libc::STDIN_FILENO;
    use std::ffi::c_char;

    unsafe { libc::signal(libc::SIGINT, request_quit as usize) };

    stdout::print(TERM_INIT_SEQ);

    unsafe { libc::setlocale(libc::LC_CTYPE, "en_US.UTF-8".as_ptr() as *const c_char) };

    let mut termios = std::mem::MaybeUninit::uninit();
    if unsafe { libc::tcgetattr(STDIN_FILENO, termios.as_mut_ptr()) } == 0 {
        let mut termios = unsafe { termios.assume_init() };
        ORIG_TERMIOS.lock().replace(termios);
        termios.c_lflag &= !(libc::ECHO | libc::ICANON);
        termios.c_cc[libc::VMIN] = 0;
        termios.c_cc[libc::VTIME] = 1;
        unsafe { libc::tcsetattr(STDIN_FILENO, libc::TCSANOW, &termios) };
    }
}

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
    use libc::STDIN_FILENO;

    if let Some(termios) = ORIG_TERMIOS.lock().take() {
        unsafe { libc::tcsetattr(STDIN_FILENO, libc::TCSANOW, &termios) };
    }
    stdout::print(TERM_EXIT_SEQ);
    print_errors();
    unsafe { libc::tcflush(STDIN_FILENO, libc::TCIFLUSH) };
    exit(0);
}

#[cfg(windows)]
pub fn quit() -> ! {
    stdout::print(TERM_EXIT_SEQ);
    print_errors();
    exit(0);
}

pub fn setup_panic_handler() {
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
        send_error!("[panic] {} at {}", msg, location);
        quit();
    }));
}

// @ ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== @
