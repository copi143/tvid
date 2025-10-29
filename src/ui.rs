use parking_lot::Mutex;
use std::cmp::min;
use std::fs::FileType;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use unicode_width::UnicodeWidthChar;

use crate::error::get_errors;
use crate::ffmpeg;
use crate::playlist::{PLAYLIST, PLAYLIST_SELECTED_INDEX, SHOW_PLAYLIST};
use crate::stdin::{self, Key};
use crate::stdout::OUTPUT_TIME;
use crate::term::{
    ESCAPE_STRING_ENCODE_TIME, RENDER_TIME, RenderWrapper, TERM_DEFAULT_BG, TERM_DEFAULT_FG,
};
use crate::util::{Cell, Color, TextBoxInfo, avg_duration, best_contrast_color};

// @ ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== @

const UNIFONT: *const [u8; 32] =
    include_bytes!("../unifont-17.0.01.bin").as_ptr() as *const [u8; 32];

pub fn unifont_get(ch: char) -> &'static [u8; 32] {
    let ch = ch as u32;
    if ch < 65536 {
        unsafe { &*UNIFONT.add(ch as usize) }
    } else {
        unsafe { &*UNIFONT.add(' ' as usize) }
    }
}

// @ ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== @

#[allow(clippy::too_many_arguments)]
pub fn mask(
    wrap: &mut RenderWrapper,
    x: isize,
    y: isize,
    w: usize,
    h: usize,
    border: Option<Color>,
    color: Color,
    opacity: f32,
) {
    for j in 0..h {
        for i in 0..w {
            let (x, y) = (x + i as isize, y + j as isize);
            if x < 0 || x >= wrap.cells_width as isize || y < 0 || y >= wrap.cells_height as isize {
                continue;
            }
            let (x, y) = (x as usize, y as usize);
            let p = y * wrap.cells_pitch + x;
            wrap.cells[p].fg = Color::mix(color, wrap.cells[p].fg, opacity);
            wrap.cells[p].bg = Color::mix(color, wrap.cells[p].bg, opacity);
            if let Some(border) = border
                && (i == 0 || i == w - 1 || j == 0 || j == h - 1)
            {
                if wrap.cells[p].c == Some('\0') {
                    let mut i = p - 1;
                    while wrap.cells[i].c == Some('\0') {
                        wrap.cells[i].c = Some(' ');
                        i -= 1;
                    }
                    wrap.cells[i].c = Some(' ');
                }
                if wrap.cells[p + 1].c == Some('\0') {
                    let mut i = p + 1;
                    while wrap.cells[i].c == Some('\0') {
                        wrap.cells[i].c = Some(' ');
                        i += 1;
                    }
                }
                if wrap.cells[p].c == None {
                    wrap.cells[p].bg = Color::halfhalf(wrap.cells[p].fg, wrap.cells[p].bg)
                }
                wrap.cells[p].fg = border;
                wrap.cells[p].c = if i == 0 && j == 0 {
                    Some('┌')
                } else if i == w - 1 && j == 0 {
                    Some('┐')
                } else if i == 0 && j == h - 1 {
                    Some('└')
                } else if i == w - 1 && j == h - 1 {
                    Some('┘')
                } else if i == 0 || i == w - 1 {
                    Some('│')
                } else {
                    Some('─')
                };
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub fn putat(
    wrap: &mut RenderWrapper,
    text: &str,
    x: isize,
    y: isize,
    w: usize,
    h: usize,
    sx: isize,
    sy: isize,
    fg: Option<Color>,
    bg: Option<Color>,
    autowrap: bool,
) -> (usize, isize, isize) {
    let (cells_width, cells_height) = (wrap.cells_width as isize, wrap.cells_height as isize);
    if w == 0 || h == 0 {
        return (0, x, y);
    }
    let mut cx = x;
    let mut cy = y;
    let mut pn = 0;
    for ch in text.chars() {
        let cw = ch.width().unwrap_or(0) as isize;
        // 跳过不可见字符
        if cw == 0 {
            pn += 1; // 假装是打印了
            continue;
        }
        // 检查是否超出参数指定的区域
        if cy >= sy + h as isize {
            break;
        }
        if cx + cw > sx + w as isize {
            if autowrap {
                cx = sx;
                cy += 1;
                if cy >= sy + h as isize {
                    break;
                }
            } else {
                break;
            }
        }
        // 不管怎样我们认为字符已经被打印，毕竟它在参数给出的区域内
        pn += 1;
        // 检查是否超出屏幕范围
        if cx < 0 || cx + cw > cells_width || cy < 0 || cy >= cells_height {
            cx += cw;
            continue;
        }
        // 计算索引
        let p = cy as usize * wrap.cells_pitch + cx as usize;
        if wrap.cells[p].c == Some('\0') {
            let mut i = p - 1;
            while wrap.cells[i].c == Some('\0') {
                wrap.cells[i].c = Some(' ');
                i -= 1;
            }
            wrap.cells[i].c = Some(' ');
        }
        if wrap.cells[p + 1].c == Some('\0') {
            let mut i = p + 1;
            while wrap.cells[i].c == Some('\0') {
                wrap.cells[i].c = Some(' ');
                i += 1;
            }
        }
        if ch == ' ' && bg == None {
            if wrap.cells[p].c.is_some() {
                wrap.cells[p].c = Some(' ');
            }
            cx += 1;
            continue;
        }
        let bg = bg.unwrap_or(if wrap.cells[p].c == None {
            Color::halfhalf(wrap.cells[p].fg, wrap.cells[p].bg)
        } else {
            wrap.cells[p].bg
        });
        let fg = fg.unwrap_or(if wrap.cells[p].c == None {
            best_contrast_color(bg)
        } else {
            wrap.cells[p].fg
        });
        wrap.cells[p] = Cell::new(ch, fg, bg);
        for i in 1..cw as usize {
            wrap.cells[p + i] = Cell {
                c: Some('\0'),
                ..Default::default()
            };
        }
        cx += cw;
    }
    return (pn, cx, cy);
}

macro_rules! putat {
    ($wrap:expr, $x:expr, $y:expr, $($arg:tt)*) => {
        crate::ui::putat($wrap, &format!($($arg)*), $x, $y, u16::MAX as usize, u16::MAX as usize, i16::MIN as isize, i16::MIN as isize, None, None, false)
    };
}

static TEXTBOX: TextBoxInfo = TextBoxInfo::new();
static TEXTBOX_DEFAULT_COLOR: Mutex<(Option<Color>, Option<Color>)> = Mutex::new((None, None));

pub fn textbox(x: isize, y: isize, w: usize, h: usize, autowrap: bool) {
    TEXTBOX.set(x, y, w, h, x, y);
    TEXTBOX.setwrap(autowrap);
}

pub fn put(wrap: &mut RenderWrapper, text: &str, fg: Option<Color>, bg: Option<Color>) {
    let (def_fg, def_bg) = TEXTBOX_DEFAULT_COLOR.lock().clone();
    let (fg, bg) = (fg.or(def_fg), bg.or(def_bg));
    let (x, y, w, h, i, j) = TEXTBOX.get();
    let (_, cx, cy) = putat(wrap, text, i, j, w, h, x, y, fg, bg, TEXTBOX.getwrap());
    TEXTBOX.set(x, y, w, h, cx, cy);
}

macro_rules! put {
    ($wrap:expr, $text:expr) => {
        crate::ui::put($wrap, &$text, None, None)
    };
    ($wrap:expr, $($arg:tt)*) => {
        crate::ui::put($wrap, &format!($($arg)*), None, None)
    };
}

pub fn putln(wrap: &mut RenderWrapper, text: &str, fg: Option<Color>, bg: Option<Color>) {
    let (def_fg, def_bg) = TEXTBOX_DEFAULT_COLOR.lock().clone();
    let (fg, bg) = (fg.or(def_fg), bg.or(def_bg));
    let (x, y, w, h, i, j) = TEXTBOX.get();
    let (_, _, cy) = putat(wrap, text, i, j, w, h, x, y, fg, bg, TEXTBOX.getwrap());
    TEXTBOX.set(x, y, w, h, x, cy + 1);
}

macro_rules! putln {
    ($wrap:expr, $text:expr) => {
        crate::ui::putln($wrap, &$text, None, None)
    };
    ($wrap:expr, $($arg:tt)*) => {
        crate::ui::putln($wrap, &format!($($arg)*), None, None)
    };
}

// @ ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== @

pub fn putunifont(wrap: &mut RenderWrapper, text: &str, fg: Option<Color>, bg: Option<Color>) {
    let mut data = [const { String::new() }; 4];
    for ch in text.chars() {
        let font = unifont_get(ch);
        let cw = ch.width().unwrap_or(0);
        if cw == 1 {
            for y in 0..4 {
                for x in 0..4 {
                    data[y].push(char::from_u32(0x2800 + font[y * 8 + x] as u32).unwrap());
                }
            }
        }
        if cw == 2 {
            for y in 0..4 {
                for x in 0..8 {
                    data[y].push(char::from_u32(0x2800 + font[y * 8 + x] as u32).unwrap());
                }
            }
        }
    }
    for y in 0..4 {
        putln(wrap, &data[y], fg, bg);
    }
}

macro_rules! putunifont {
    ($wrap:expr, $text:expr) => {
        crate::ui::putunifont($wrap, &$text, None, None)
    };
    ($wrap:expr, $($arg:tt)*) => {
        crate::ui::putunifont($wrap, &format!($($arg)*), None, None)
    };
}

// @ ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== @

pub fn render_ui(wrap: &mut RenderWrapper) {
    if wrap.cells_width < 4 && wrap.cells_height < 4 {
        return; // 防炸
    }
    render_overlay_text(wrap);
    render_playlist(wrap);
    render_file_select(wrap);
    render_errors(wrap);
}

fn render_overlay_text(wrap: &mut RenderWrapper) {
    if wrap.cells_width < 8 && wrap.cells_height < 8 {
        return; // 防炸
    }

    let time_str = if let Some(t) = wrap.played_time {
        format!(
            "{:02}h {:02}m {:02}s {:03}ms",
            t.as_secs() / 3600,
            (t.as_secs() % 3600) / 60,
            t.as_secs() % 60,
            t.subsec_millis()
        )
    } else {
        "N/A".to_string()
    };

    textbox(2, 1, wrap.cells_width - 4, wrap.cells_height - 2, true);

    TEXTBOX_DEFAULT_COLOR.lock().clone_from(&(None, None));

    if wrap.term_font_height > 12.0 {
        putln!(wrap, "tvid v{}", env!("CARGO_PKG_VERSION"));
        putln!(
            wrap,
            "Press 'q' to quit, 'n' to skip to next, 'l' for playlist"
        );
        putln!(wrap, "Playing: {}", wrap.playing);
        putln!(wrap, "Time: {}", time_str);
        putln!(
            wrap,
            "Escape String Encode Time: {:.2?} (avg over last 60)",
            avg_duration(&ESCAPE_STRING_ENCODE_TIME)
        );
        putln!(
            wrap,
            "Render Time: {:.2?} (avg over last 60)",
            avg_duration(&RENDER_TIME)
        );
        putln!(
            wrap,
            "Output Time: {:.2?} (avg over last 60)",
            avg_duration(&OUTPUT_TIME)
        );
    } else {
        putunifont!(wrap, "tvid v{}", env!("CARGO_PKG_VERSION"));
        putunifont!(
            wrap,
            "Press 'q' to quit, 'n' to skip to next, 'l' for playlist"
        );
        putunifont!(wrap, "Playing: {}", wrap.playing);
        putunifont!(wrap, "Time: {}", time_str);
        putunifont!(
            wrap,
            "Escape String Encode Time: {:.2?} (avg over last 60)",
            avg_duration(&ESCAPE_STRING_ENCODE_TIME)
        );
        putunifont!(
            wrap,
            "Render Time: {:.2?} (avg over last 60)",
            avg_duration(&RENDER_TIME)
        );
        putunifont!(
            wrap,
            "Output Time: {:.2?} (avg over last 60)",
            avg_duration(&OUTPUT_TIME)
        );
    }
}

fn render_playlist(wrap: &mut RenderWrapper) {
    static mut PLAYLIST_POS: f32 = 0.0;
    const PLAYLIST_WIDTH: usize = 62;

    let mut playlist_pos = unsafe { PLAYLIST_POS };
    if SHOW_PLAYLIST.load(Ordering::SeqCst) {
        playlist_pos += wrap.delta_time.as_secs_f32() * 300.0;
    } else {
        playlist_pos -= wrap.delta_time.as_secs_f32() * 300.0;
    }
    let playlist_pos = playlist_pos.clamp(0.0, PLAYLIST_WIDTH as f32);
    unsafe { PLAYLIST_POS = playlist_pos };

    let playlist_pos = playlist_pos as usize;
    if playlist_pos == 0 {
        return;
    }

    if wrap.cells_width < 8 && wrap.cells_height < 8 {
        return; // 防炸
    }

    mask(
        wrap,
        wrap.cells_width.saturating_sub(playlist_pos) as isize,
        0,
        PLAYLIST_WIDTH,
        wrap.cells_height,
        Some(TERM_DEFAULT_BG),
        TERM_DEFAULT_FG,
        0.5,
    );

    textbox(
        wrap.cells_width.saturating_sub(playlist_pos) as isize + 1,
        1,
        PLAYLIST_WIDTH - 2,
        wrap.cells_height - 2,
        false,
    );

    TEXTBOX_DEFAULT_COLOR
        .lock()
        .clone_from(&(Some(TERM_DEFAULT_BG), None));

    putln!(wrap, "Playlist ({} items):", PLAYLIST.lock().len());

    let selected_index = *PLAYLIST_SELECTED_INDEX.lock();
    let playing_index = PLAYLIST.lock().get_pos().clone();
    for (i, item) in PLAYLIST.lock().get_items().iter().enumerate() {
        // 这边的 U+2000 是故意占位的，因为 ▶ 符号在终端上渲染宽度是 2
        let icon = if i == playing_index { "▶ " } else { "  " };
        if i as isize == selected_index {
            putln(
                wrap,
                &format!("{}{}", icon, item),
                Some(TERM_DEFAULT_FG),
                Some(TERM_DEFAULT_BG),
            );
        } else {
            putln!(wrap, "{}{}", icon, item);
        }
    }
}

fn render_errors(wrap: &mut RenderWrapper) {
    if wrap.cells_width < 8 && wrap.cells_height < 8 {
        return; // 防炸
    }

    let errors = get_errors();

    mask(
        wrap,
        0,
        wrap.cells_height as isize - errors.len() as isize,
        50,
        errors.len(),
        None,
        Color::new(237, 21, 21),
        0.5,
    );

    textbox(
        0,
        wrap.cells_height as isize - errors.len() as isize,
        50,
        errors.len(),
        false,
    );

    TEXTBOX_DEFAULT_COLOR
        .lock()
        .clone_from(&(Some(TERM_DEFAULT_BG), None));

    for error in errors.iter() {
        putln(wrap, &error.msg, error.fg, error.bg);
    }
}

// @ ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== @

pub static FILE_SELECT: AtomicBool = AtomicBool::new(false);
pub static FILE_SELECT_PATH: Mutex<String> = Mutex::new(String::new());
pub static FILE_SELECT_LIST: Mutex<Vec<(FileType, String)>> = Mutex::new(Vec::new());
pub static FILE_SELECT_INDEX: Mutex<usize> = Mutex::new(0);

fn render_file_select(wrap: &mut RenderWrapper) {
    static mut FILE_SELECT_SHOWN: f32 = 0.0;
    static mut FILE_SELECT_ALPHA: f32 = 0.0;

    let mut file_select_alpha = unsafe { FILE_SELECT_ALPHA };
    if FILE_SELECT.load(Ordering::SeqCst) {
        file_select_alpha += wrap.delta_time.as_secs_f32() * 2.0;
    } else {
        file_select_alpha -= wrap.delta_time.as_secs_f32() * 2.0;
    }
    let file_select_alpha = file_select_alpha.clamp(0.0, 1.0);
    unsafe { FILE_SELECT_ALPHA = file_select_alpha };

    if file_select_alpha == 0.0 {
        return;
    }

    if wrap.cells_width < 8 && wrap.cells_height < 8 {
        return; // 防炸
    }

    let (w, h) = (wrap.cells_width / 2, wrap.cells_height / 2);
    let (x, y) = (
        (wrap.cells_width as isize - w as isize) / 2,
        (wrap.cells_height as isize - h as isize) / 2,
    );

    mask(
        wrap,
        x,
        y,
        w,
        h,
        Some(TERM_DEFAULT_BG),
        TERM_DEFAULT_FG,
        file_select_alpha * 0.5,
    );

    textbox(x + 1, y + 1, w - 2, h - 2, false);

    TEXTBOX_DEFAULT_COLOR
        .lock()
        .clone_from(&(Some(TERM_DEFAULT_BG), None));

    let mut path = FILE_SELECT_PATH.lock();
    let mut list = FILE_SELECT_LIST.lock();
    let index = FILE_SELECT_INDEX.lock();

    let mut file_select_shown = unsafe { FILE_SELECT_SHOWN };
    if FILE_SELECT.load(Ordering::SeqCst) {
        file_select_shown += wrap.delta_time.as_secs_f32() * 60.0;
    } else {
        file_select_shown -= wrap.delta_time.as_secs_f32() * 60.0;
    }
    let file_select_shown = file_select_shown.clamp(0.0, min(h - 5, list.len()) as f32);
    unsafe { FILE_SELECT_SHOWN = file_select_shown };

    putln!(wrap, "File Select: {}", path);
    putln!(
        wrap,
        "    Use arrow keys to navigate, Space to select, Q to cancel."
    );
    for _ in 0..w - 2 {
        put!(wrap, "-");
    }
    putln!(wrap, "");

    if path.is_empty() {
        *path = "/".to_string();
        list.clear();
        if let Ok(entries) = std::fs::read_dir(&*path) {
            for entry in entries.flatten() {
                if let Ok(file_type) = entry.file_type() {
                    let file_name = entry.file_name().to_string_lossy().to_string();
                    list.push((file_type, file_name));
                }
            }
        }
    }

    let max_show = h - 5;
    let mut show_cnt = 0;
    for (i, (file_type, file_name)) in list.iter().enumerate() {
        if i + max_show / 2 < *index && i + max_show < list.len() {
            continue;
        }
        show_cnt += 1;
        if show_cnt as f32 > file_select_shown {
            break;
        }
        let text = format!(
            " {} {} ",
            if file_type.is_dir() {
                "📁"
            } else if file_type.is_file() {
                "📄"
            } else if file_type.is_symlink() {
                "🔗"
            } else {
                "❓"
            },
            file_name
        );
        if i == *index {
            putln(wrap, &text, Some(TERM_DEFAULT_FG), Some(TERM_DEFAULT_BG));
        } else {
            putln!(wrap, text);
        }
    }
}

pub fn register_keypress_callbacks() {
    stdin::register_keypress_callback(Key::Normal('q'), |_| {
        if !FILE_SELECT.load(Ordering::SeqCst) {
            return false;
        }
        FILE_SELECT.store(false, Ordering::SeqCst);
        true
    });

    let cb = |_| {
        if !FILE_SELECT.load(Ordering::SeqCst) {
            return false;
        }
        let dir = FILE_SELECT_PATH.lock();
        let list = FILE_SELECT_LIST.lock();
        if list.is_empty() {
            return true;
        }
        let index = *FILE_SELECT_INDEX.lock();
        let (file_type, file_name) = &list[index];
        let path = format!("{}/{}", dir, file_name);
        let mut is_file = file_type.is_file();
        if file_type.is_symlink() {
            if let Ok(target_type) = std::fs::metadata(&path).map(|m| m.file_type()) {
                is_file = target_type.is_file();
            }
        }
        if is_file {
            FILE_SELECT.store(false, Ordering::SeqCst);
            PLAYLIST.lock().push_and_setnext(&path);
            ffmpeg::notify_quit();
        } else {
            send_error!("Cannot open non-file: {}", path);
        }
        true
    };
    stdin::register_keypress_callback(Key::Normal(' '), cb);
    stdin::register_keypress_callback(Key::Enter, cb);

    let cb = |_| {
        if !FILE_SELECT.load(Ordering::SeqCst) {
            return false;
        }
        let len = FILE_SELECT_LIST.lock().len();
        let mut lock = FILE_SELECT_INDEX.lock();
        *lock = lock.clamp(1, len) - 1;
        true
    };
    stdin::register_keypress_callback(Key::Normal('w'), cb);
    stdin::register_keypress_callback(Key::Up, cb);

    let cb = |_| {
        if !FILE_SELECT.load(Ordering::SeqCst) {
            return false;
        }
        let len = FILE_SELECT_LIST.lock().len();
        let mut lock = FILE_SELECT_INDEX.lock();
        *lock = (*lock + 1).clamp(0, len - 1);
        true
    };
    stdin::register_keypress_callback(Key::Normal('s'), cb);
    stdin::register_keypress_callback(Key::Down, cb);

    let cb = |_| {
        if !FILE_SELECT.load(Ordering::SeqCst) {
            return false;
        }
        let mut path = FILE_SELECT_PATH.lock();
        let mut list = FILE_SELECT_LIST.lock();
        let mut index = FILE_SELECT_INDEX.lock();
        let filename = path.rsplit('/').next().unwrap_or("").to_string();
        *path = std::fs::canonicalize(&*path)
            .unwrap_or_else(|_| PathBuf::from("/"))
            .parent()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|| "/".to_string());
        list.clear();
        if let Ok(entries) = std::fs::read_dir(&*path) {
            for entry in entries.flatten() {
                if let Ok(file_type) = entry.file_type() {
                    let file_name = entry.file_name().to_string_lossy().to_string();
                    list.push((file_type, file_name));
                }
            }
        }
        *index = list
            .iter()
            .enumerate()
            .find(|(_, (_, name))| *name == filename)
            .map(|(i, _)| i)
            .unwrap_or(0);
        true
    };
    stdin::register_keypress_callback(Key::Normal('a'), cb);
    stdin::register_keypress_callback(Key::Left, cb);

    let cb = |_| {
        if !FILE_SELECT.load(Ordering::SeqCst) {
            return false;
        }
        let mut path = FILE_SELECT_PATH.lock();
        let mut list = FILE_SELECT_LIST.lock();
        let mut index = FILE_SELECT_INDEX.lock();
        if list.is_empty() {
            return true;
        }
        let (file_type, file_name) = &list[*index];
        if file_type.is_dir() {
            if path.ends_with('/') {
                path.push_str(file_name);
            } else {
                path.push('/');
                path.push_str(file_name);
            }
            list.clear();
            *index = 0;
            if let Ok(entries) = std::fs::read_dir(&*path) {
                for entry in entries.flatten() {
                    if let Ok(file_type) = entry.file_type() {
                        let file_name = entry.file_name().to_string_lossy().to_string();
                        list.push((file_type, file_name));
                    }
                }
            }
        }
        true
    };
    stdin::register_keypress_callback(Key::Normal('d'), cb);
    stdin::register_keypress_callback(Key::Right, cb);
}
