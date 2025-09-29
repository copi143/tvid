use parking_lot::Mutex;
use std::{
    collections::VecDeque,
    process::exit,
    sync::atomic::{AtomicBool, Ordering},
    time::{Duration, Instant},
};
use unicode_width::UnicodeWidthChar;

use crate::{
    TOKIO_RUNTIME, audio,
    error::print_errors,
    ffmpeg,
    playlist::PLAYLIST,
    stdin,
    stdout::{self, pend_print, pending_frames, remove_pending_frames},
    util::*,
};

// @ ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== @

pub static RENDER_TIME: Mutex<VecDeque<Duration>> = Mutex::new(VecDeque::new());
pub static ESCAPE_STRING_ENCODE_TIME: Mutex<VecDeque<Duration>> = Mutex::new(VecDeque::new());

fn set_render_time(duration: Duration) {
    let mut lock = RENDER_TIME.lock();
    lock.push_back(duration);
    while lock.len() > 60 {
        lock.pop_front();
    }
}

fn set_escape_string_encode_time(duration: Duration) {
    let mut lock = ESCAPE_STRING_ENCODE_TIME.lock();
    lock.push_back(duration);
    while lock.len() > 60 {
        lock.pop_front();
    }
}

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

static mut THIS_FRAME: Vec<Cell> = Vec::new();
static mut LAST_FRAME: Vec<Cell> = Vec::new();
static mut RENDER_CALLBACKS: Vec<fn(&mut RenderWrapper)> = Vec::new();

#[allow(static_mut_refs)]
pub fn add_render_callback(callback: fn(&mut RenderWrapper<'_, '_>)) {
    unsafe { RENDER_CALLBACKS.push(callback) };
}

#[allow(static_mut_refs)]
pub fn render(frame: &[Color], pitch: usize) {
    static LAST_TIME: Mutex<Option<Duration>> = Mutex::new(None);

    let played_time = audio::played_time_or_none();

    let delta_time = LAST_TIME
        .lock()
        .map(|t1| played_time.map(|t2| t2.saturating_sub(t1)))
        .unwrap_or(None)
        .unwrap_or(Duration::from_millis(0));

    if let Some(played_time) = played_time {
        LAST_TIME.lock().replace(played_time);
    }

    let wrap = &mut RenderWrapper {
        frame,
        frame_width: VIDEO_PIXELS.x(),
        frame_height: VIDEO_PIXELS.y(),
        frame_pitch: pitch,
        cells: unsafe { THIS_FRAME.as_mut_slice() },
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
    for callback in unsafe { &RENDER_CALLBACKS } {
        callback(wrap);
    }
    set_render_time(instant.elapsed());

    TOKIO_RUNTIME.block_on(print_diff(TERM_RESIZED.swap(false, Ordering::SeqCst)));

    if pending_frames() > 3 {
        TOKIO_RUNTIME.block_on(print_diff(true));
    }

    unsafe { std::mem::swap(&mut THIS_FRAME, &mut LAST_FRAME) };
}

async fn print_diff_line(cells: &mut [Cell], lasts: &[Cell], force_flush: bool) -> Vec<u8> {
    let mut last_bg = Color::transparent();
    let mut last_fg = Color::transparent();
    let mut buf = Vec::with_capacity(1024);
    for (cell, last) in cells.iter().zip(lasts.iter()) {
        if cell.c == Some('\0') {
            continue;
        }

        let cw = cell.c.map_or(1, |c| c.width().unwrap_or(1).max(1));
        if !force_flush && cell == last && cw == 1 {
            buf.extend_from_slice("\x1b[C".as_bytes());
            continue;
        }

        let (fg, bg) = (some_if_ne(cell.fg, last_fg), some_if_ne(cell.bg, last_bg));

        buf.extend_from_slice(escape_set_color(fg, bg).as_bytes());
        buf.extend_from_slice(
            if let Some(c) = cell.c { c } else { '▄' }
                .to_string()
                .as_bytes(),
        );

        if let Some(fg) = fg {
            last_fg = fg;
        }
        if let Some(bg) = bg {
            last_bg = bg;
        }
    }
    buf
}

#[allow(static_mut_refs)]
async fn print_diff(force_flush: bool) {
    let (term_width, term_height) = TERM_SIZE.get();
    let instant = Instant::now();

    let cells = unsafe {
        THIS_FRAME
            .as_mut_slice()
            .split_at_mut(THIS_FRAME.len() - 1)
            .0
            .chunks_mut(term_width)
            .map(|chunk| std::mem::transmute(chunk))
            .collect::<Vec<_>>()
    };
    let lasts = unsafe {
        LAST_FRAME
            .as_slice()
            .split_at(LAST_FRAME.len() - 1)
            .0
            .chunks(term_width)
            .map(|chunk| std::mem::transmute(chunk))
            .collect::<Vec<_>>()
    };

    assert!(cells.len() == term_height, "cells length mismatch");
    assert!(lasts.len() == term_height, "lasts length mismatch");

    let result = (cells.into_iter().zip(lasts.into_iter()))
        .map(|(cell, last)| tokio::spawn(print_diff_line(cell, last, force_flush)))
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

    set_escape_string_encode_time(instant.elapsed());
    if force_flush {
        remove_pending_frames();
        assert!(buf.len() > 0, "force flush but buffer is empty");
        pend_print(buf);
        return;
    } else if buf.len() > 0 {
        pend_print(buf);
    }
}

// @ ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== @

pub static VIDEO_ORIGIN_PIXELS_NOW: XY = XY::new();
pub static VIDEO_ORIGIN_PIXELS: XY = XY::new();

pub static TERM_SIZE: XY = XY::new();
pub static TERM_PIXELS: XY = XY::new();
pub static VIDEO_PIXELS: XY = XY::new();
pub static VIDEO_PADDING: TBLR = TBLR::new();

static TERM_RESIZED: AtomicBool = AtomicBool::new(false);

#[allow(static_mut_refs)]
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

    unsafe {
        // 将大小加一作为哨兵
        THIS_FRAME.clear();
        THIS_FRAME.resize(xchars * ychars + 1, Cell::default())
    };
    unsafe {
        // 将大小加一作为哨兵
        LAST_FRAME.clear();
        LAST_FRAME.resize(xchars * ychars + 1, Cell::default())
    };

    remove_pending_frames();

    TERM_RESIZED.store(true, Ordering::SeqCst);
    return true;
}

// @ ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== @

struct Winsize {
    pub row: u16,
    pub col: u16,
    pub xpixel: u16,
    pub ypixel: u16,
}

#[cfg(unix)]
fn get_winsize() -> Option<Winsize> {
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
fn get_winsize() -> Option<Winsize> {
    use winapi::shared::minwindef::BOOL;
    use winapi::um::processenv::GetStdHandle;
    use winapi::um::winbase::STD_OUTPUT_HANDLE;
    use winapi::um::wincon::CONSOLE_SCREEN_BUFFER_INFO;
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
        Some(Winsize {
            row,
            col,
            xpixel: col * 8,
            ypixel: row * 16,
        })
    }
}

// @ ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== @

pub const TERM_DEFAULT_FG: Color = Color::new(171, 178, 191);
pub const TERM_DEFAULT_BG: Color = Color::new(35, 39, 46);

pub static TERM_QUIT: AtomicBool = AtomicBool::new(false);

const TERM_INIT_SEQ: &[u8] = b"\x1b[?1049h\x1b[?25l\x1b[?1006h\x1b[?1003h\x1b[?2004h";
const TERM_EXIT_SEQ: &[u8] = b"\x1b[?2004l\x1b[?1003l\x1b[?1006l\x1b[?25h\x1b[?1049l";

pub extern "C" fn request_quit() {
    TERM_QUIT.store(true, Ordering::SeqCst);
    ffmpeg::notify_quit();
    stdin::notify_quit();
    stdout::notify_quit();
}

#[cfg(unix)]
static mut ORIG_TERMIOS: Option<libc::termios> = None;

/// 在初始化终端之前不能启动 stdin 和 stdout 线程
#[cfg(unix)]
pub fn init() {
    use libc::STDIN_FILENO;

    unsafe { libc::signal(libc::SIGINT, request_quit as usize) };

    stdout::print(TERM_INIT_SEQ);

    unsafe { libc::setlocale(libc::LC_CTYPE, "en_US.UTF-8".as_ptr() as *const i8) };

    let mut termios = std::mem::MaybeUninit::uninit();
    if unsafe { libc::tcgetattr(STDIN_FILENO, termios.as_mut_ptr()) } == 0 {
        let mut termios = unsafe { termios.assume_init() };
        unsafe { ORIG_TERMIOS = Some(termios) };
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
    use winapi::um::wincon::{ENABLE_PROCESSED_INPUT, ENABLE_VIRTUAL_TERMINAL_PROCESSING};
    use winapi::um::winnt::HANDLE;
    unsafe extern "system" {
        fn SetConsoleCP(wCodePageID: u32) -> i32;
        fn SetConsoleOutputCP(wCodePageID: u32) -> i32;
    }
    unsafe {
        SetConsoleCP(65001);
        SetConsoleOutputCP(65001);
        stdout::print(TERM_INIT_SEQ);
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

    if let Some(termios) = unsafe { ORIG_TERMIOS } {
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

// @ ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== @
