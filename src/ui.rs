use parking_lot::Mutex;
use std::cmp::min;
use std::fs::FileType;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use unicode_width::UnicodeWidthChar;

use crate::avsync;
use crate::logging::{MessageLevel, get_messages};
use crate::playlist::{PLAYLIST, PLAYLIST_SELECTED_INDEX, SHOW_PLAYLIST};
use crate::statistics::get_statistics;
use crate::stdin::{self, Key, MouseAction};
use crate::term::{RenderWrapper, TERM_DEFAULT_BG, TERM_DEFAULT_FG, TERM_PIXELS, TERM_SIZE};
use crate::util::{Cell, Color, TextBoxInfo, best_contrast_color};
use crate::{ffmpeg, term};

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
// @ 直接操作屏幕 @

/// 在指定区域绘制半透明叠加层
/// - `wrap`: 渲染包装器
/// - `x`, `y`: 起始位置 (字符)
/// - `w`, `h`: 宽度和高度 (字符)
/// - `border`: 可选的边框颜色
/// - `color`: 叠加层颜色
/// - `opacity`: 叠加层透明度
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

/// 在指定位置绘制文本，返回实际绘制的字符数和新的光标位置
/// - `wrap`: 渲染包装器
/// - `text`: 要绘制的文本
/// - `x`, `y`: 起始绘制位置 (字符)
/// - `w`, `h`: 可绘制区域的宽度和高度 (字符)
/// - `sx`, `sy`: 可绘制区域的起始位置 (字符)
/// - `fg`, `bg`: 前景色和背景色
/// - `autowrap`: 是否自动换行
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
    let mut cx = x; // 当前光标位置
    let mut cy = y; // 当前光标位置
    let mut pn = 0; // 实际打印的字符数
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
        // 如果覆盖了一个宽字符那么要清除整个宽字符，防止渲染爆炸
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
        // 对于空格就直接替换原本的字符为空格，如果原本什么都没有就不动
        if ch == ' ' && bg == None {
            if wrap.cells[p].c.is_some() {
                wrap.cells[p].c = Some(' ');
            }
            cx += 1;
            continue;
        }
        // 然后计算颜色并设置单元格
        let bg = bg.unwrap_or_else(|| {
            if wrap.cells[p].c == None {
                Color::halfhalf(wrap.cells[p].fg, wrap.cells[p].bg)
            } else {
                wrap.cells[p].bg
            }
        });
        let fg = fg.unwrap_or_else(|| {
            if wrap.cells[p].c == None {
                best_contrast_color(bg)
            } else {
                wrap.cells[p].fg
            }
        });
        wrap.cells[p] = Cell::new(ch, fg, bg);
        // 直接设置为占位符应该是没问题的，颜色应该不需要去动
        for i in 1..cw as usize {
            wrap.cells[p + i].c = Some('\0');
        }
        cx += cw;
    }
    (pn, cx, cy)
}

/// 直接在指定的位置开始贴上文本
macro_rules! putat {
    ($wrap:expr, $x:expr, $y:expr, $($arg:tt)*) => {
        crate::ui::putat($wrap, &format!($($arg)*), $x, $y, u16::MAX as usize, u16::MAX as usize, i16::MIN as isize, i16::MIN as isize, None, None, false);
    };
}

// @ ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== @

static TEXTBOX: TextBoxInfo = TextBoxInfo::new();
static TEXTBOX_DEFAULT_COLOR: Mutex<(Option<Color>, Option<Color>)> = Mutex::new((None, None));

pub fn textbox(x: isize, y: isize, w: usize, h: usize, autowrap: bool) {
    TEXTBOX.set(x, y, w, h, x, y);
    TEXTBOX.setwrap(autowrap);
    TEXTBOX_DEFAULT_COLOR.lock().clone_from(&(None, None));
}

pub fn textbox_default_color(fg: Option<Color>, bg: Option<Color>) {
    TEXTBOX_DEFAULT_COLOR.lock().clone_from(&(fg, bg));
}

pub fn put(wrap: &mut RenderWrapper, text: &str, fg: Option<Color>, bg: Option<Color>) {
    let (def_fg, def_bg) = *TEXTBOX_DEFAULT_COLOR.lock();
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
    let (def_fg, def_bg) = *TEXTBOX_DEFAULT_COLOR.lock();
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
    for text in data {
        putln(wrap, &text, fg, bg);
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

/// 是否已经开始渲染第一帧，防止事件在此之前触发
static FIRST_RENDERED: AtomicBool = AtomicBool::new(false);

pub fn render_ui(wrap: &mut RenderWrapper) {
    FIRST_RENDERED.store(true, Ordering::SeqCst);
    if wrap.cells_width < 4 || wrap.cells_height < 4 {
        return; // 防炸
    }
    render_progressbar(wrap);
    render_overlay_text(wrap);
    render_playlist(wrap);
    render_file_select(wrap);
    render_messages(wrap);
    render_help(wrap);
    render_quit_confirmation(wrap);
}

pub static SHOW_PROGRESSBAR: AtomicBool = AtomicBool::new(true);

/// 按像素计的进度条高度
static mut PROGRESSBAR_HEIGHT: f32 = 16.0;

fn calc_bar_size() -> (usize, usize) {
    let term_font_height = TERM_PIXELS.y() as f32 / TERM_SIZE.y() as f32;
    let bar_w = TERM_SIZE.x() as f64 * avsync::playback_progress() + 0.5;
    let bar_h = unsafe { PROGRESSBAR_HEIGHT } / term_font_height * 2.0;
    let bar_w = (bar_w as usize).clamp(0, TERM_SIZE.x());
    let bar_h = (bar_h as usize).clamp(1, TERM_SIZE.y() * 2);
    (bar_w, bar_h)
}

fn render_progressbar(wrap: &mut RenderWrapper) {
    if !SHOW_PROGRESSBAR.load(Ordering::SeqCst) {
        return;
    }

    let (bar_w, bar_h) = calc_bar_size();

    for y in wrap.cells_height * 2 - bar_h..wrap.cells_height * 2 {
        for x in 0..bar_w {
            let i = y / 2 * wrap.cells_pitch + x;
            if y % 2 == 0 {
                wrap.cells[i].bg = Color::halfhalf(wrap.cells[i].bg, Color::new(0, 128, 255));
            } else {
                wrap.cells[i].fg = Color::halfhalf(wrap.cells[i].fg, Color::new(0, 128, 255));
            }
        }
    }
}

fn register_input_callbacks_progressbar() {
    static mut DRAGGING_PROGRESSBAR: bool = false;
    stdin::register_mouse_callback(|m| {
        if !FIRST_RENDERED.load(Ordering::SeqCst) {
            return false;
        }

        let term_h = TERM_SIZE.y();
        let bar_h = calc_bar_size().1.div_ceil(2);

        if unsafe { DRAGGING_PROGRESSBAR } {
            if m.left {
                let p = m.pos.0 as f64 / TERM_SIZE.x() as f64;
                ffmpeg::seek_request_absolute(p * avsync::total_duration().as_secs_f64());
            } else {
                unsafe { DRAGGING_PROGRESSBAR = false };
            }
            true
        } else if (term_h - bar_h..term_h).contains(&(m.pos.1 as usize)) {
            if m.action != MouseAction::LeftDown {
                return false;
            }
            unsafe { DRAGGING_PROGRESSBAR = true };
            let p = m.pos.0 as f64 / TERM_SIZE.x() as f64;
            ffmpeg::seek_request_absolute(p * avsync::total_duration().as_secs_f64());
            true
        } else {
            false
        }
    });
}

pub static SHOW_HELP: AtomicBool = AtomicBool::new(false);

fn render_help(wrap: &mut RenderWrapper) {
    if wrap.cells_width < 8 || wrap.cells_height < 8 {
        return; // 防炸
    }

    if !SHOW_HELP.load(Ordering::SeqCst) {
        return;
    }

    let w = 50;
    let h = 12;
    let x = (wrap.cells_width as isize - w as isize) / 2;
    let y = (wrap.cells_height as isize - h as isize) / 2;
    mask(
        wrap,
        x,
        y,
        w,
        h,
        Some(TERM_DEFAULT_BG),
        TERM_DEFAULT_FG,
        0.7,
    );
    textbox(x + 2, y + 1, w - 4, h - 2, true);
    textbox_default_color(Some(TERM_DEFAULT_BG), None);
    putln!(wrap, "帮助信息 (按 h 关闭)");
    putln!(wrap, "------------------------------");
    putln!(wrap, "q: 退出程序");
    putln!(wrap, "n: 下一项");
    putln!(wrap, "l: 打开/关闭播放列表");
    putln!(wrap, "空格/回车: 选择文件");
    putln!(wrap, "w/s/↑/↓: 上/下移动");
    putln!(wrap, "a/d/←/→: 进入/返回目录");
    putln!(wrap, "h: 打开/关闭帮助");
    putln!(wrap, "------------------------------");
}

pub static SHOW_OVERLAY_TEXT: AtomicBool = AtomicBool::new(true);

fn render_overlay_text(wrap: &mut RenderWrapper) {
    if wrap.cells_width < 8 || wrap.cells_height < 8 {
        return; // 防炸
    }

    if !SHOW_OVERLAY_TEXT.load(Ordering::SeqCst) {
        return;
    }

    let playing_time_str = if let Some(t) = wrap.played_time {
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

    let audio_offset_str = format!(
        "{:+07.3}ms",
        (avsync::audio_played_time_or_zero().as_secs_f64()
            - avsync::played_time_or_zero().as_secs_f64())
            * 1000.0
    );

    let video_offset_str = format!(
        "{:+07.3}ms",
        (avsync::video_played_time_or_zero().as_secs_f64()
            - avsync::played_time_or_zero().as_secs_f64())
            * 1000.0
    );

    let app_time_str = {
        let t = wrap.app_time;
        format!(
            "{:02}h {:02}m {:02}s {:03}ms",
            t.as_secs() / 3600,
            (t.as_secs() % 3600) / 60,
            t.as_secs() % 60,
            t.subsec_millis()
        )
    };

    let playing_or_paused_str = if avsync::is_paused() {
        "Paused"
    } else {
        "Playing"
    };

    textbox(2, 1, wrap.cells_width - 4, wrap.cells_height - 2, true);

    let statistics = get_statistics();

    if wrap.term_font_height > 12.0 {
        putln!(wrap, "tvid v{}", env!("CARGO_PKG_VERSION"));
        putln!(
            wrap,
            "Press 'q' to quit, 'n' to skip to next, 'l' for playlist"
        );
        putln!(wrap, "{}: {}", playing_or_paused_str, wrap.playing);
        putln!(
            wrap,
            "Video Time: {playing_time_str} (a: {audio_offset_str}, v: {video_offset_str})",
        );
        putln!(wrap, "App Time: {}", app_time_str);
        putln!(
            wrap,
            "Escape String Encode Time: {:.2?} (avg over last 60)",
            statistics.escape_string_encode_time.avg(),
        );
        putln!(
            wrap,
            "Render Time: {:.2?} (avg over last 60)",
            statistics.render_time.avg(),
        );
        putln!(
            wrap,
            "Output Time: {:.2?} (avg over last 60)",
            statistics.output_time.avg(),
        );
        putln!(
            wrap,
            "Video Skipped Frames: {}",
            statistics.video_skipped_frames,
        );
    } else {
        putunifont!(wrap, "tvid v{}", env!("CARGO_PKG_VERSION"));
        putunifont!(
            wrap,
            "Press 'q' to quit, 'n' to skip to next, 'l' for playlist"
        );
        putunifont!(wrap, "{}: {}", playing_or_paused_str, wrap.playing);
        putunifont!(
            wrap,
            "Video Time: {playing_time_str} (a: {audio_offset_str}, v: {video_offset_str})",
        );
        putunifont!(wrap, "App Time: {}", app_time_str);
        putunifont!(
            wrap,
            "Escape String Encode Time: {:.2?} (avg over last 60)",
            statistics.escape_string_encode_time.avg(),
        );
        putunifont!(
            wrap,
            "Render Time: {:.2?} (avg over last 60)",
            statistics.render_time.avg(),
        );
        putunifont!(
            wrap,
            "Output Time: {:.2?} (avg over last 60)",
            statistics.output_time.avg(),
        );
        putunifont!(
            wrap,
            "Video Skipped Frames: {}",
            statistics.video_skipped_frames,
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

    if wrap.cells_width < 8 || wrap.cells_height < 8 {
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

    textbox_default_color(Some(TERM_DEFAULT_BG), None);

    putln!(wrap, "Playlist ({} items):", PLAYLIST.lock().len());

    let selected_index = *PLAYLIST_SELECTED_INDEX.lock();
    let playing_index = PLAYLIST.lock().get_pos();
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

fn render_messages(wrap: &mut RenderWrapper) {
    if wrap.cells_width < 8 || wrap.cells_height < 8 {
        return; // 防炸
    }

    let width = (wrap.cells_width * 4 / 10).max(50);

    for (i, message) in get_messages().queue.iter().rev().enumerate() {
        let y = wrap.cells_height as isize - i as isize - 1;
        if y < 0 {
            continue;
        }
        let color = match message.lv {
            MessageLevel::Debug => Color::new(150, 150, 150),
            MessageLevel::Info => Color::new(21, 137, 238),
            MessageLevel::Warn => Color::new(237, 201, 21),
            MessageLevel::Error => Color::new(237, 21, 21),
            MessageLevel::Fatal => Color::new(180, 0, 0),
        };
        mask(wrap, 0, y, width, 1, None, color, 0.5);
        textbox(0, y, width, 1, false);
        textbox_default_color(Some(TERM_DEFAULT_BG), None);
        putln(wrap, &message.msg, message.fg, message.bg);
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

    if wrap.cells_width < 8 || wrap.cells_height < 8 {
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

    textbox_default_color(Some(TERM_DEFAULT_BG), None);

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

fn register_file_select_keypress_callbacks() {
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

// @ ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== @

pub static QUIT_CONFIRMATION: AtomicBool = AtomicBool::new(false);

fn render_quit_confirmation(wrap: &mut RenderWrapper) {
    if !QUIT_CONFIRMATION.load(Ordering::SeqCst) {
        return;
    }

    if wrap.cells_width < 8 || wrap.cells_height < 8 {
        return; // 防炸
    }

    let w = 25;
    let h = 3;
    let x = (wrap.cells_width as isize - w as isize) / 2;
    let y = (wrap.cells_height as isize - h as isize) / 2;
    mask(
        wrap,
        x - 10,
        y - 2,
        w + 20,
        h + 4,
        Some(TERM_DEFAULT_BG),
        TERM_DEFAULT_FG,
        0.5,
    );
    textbox(x, y, w, h, false);
    textbox_default_color(Some(TERM_DEFAULT_BG), None);
    putln!(wrap, "      Confirm Quit?      ");
    putln!(wrap, "-------------------------");
    putln!(wrap, "        q   /   c        ");
}

// @ ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== @

pub fn register_input_callbacks() {
    register_input_callbacks_progressbar();

    stdin::register_keypress_callback(Key::Normal('h'), |_| {
        SHOW_HELP.store(!SHOW_HELP.load(Ordering::SeqCst), Ordering::SeqCst);
        true
    });

    stdin::register_keypress_callback(Key::Normal('q'), |_| {
        if !QUIT_CONFIRMATION.load(Ordering::SeqCst) {
            return false;
        }
        term::request_quit();
        true
    });

    stdin::register_keypress_callback(Key::Normal('c'), |_| {
        if !QUIT_CONFIRMATION.load(Ordering::SeqCst) {
            return false;
        }
        QUIT_CONFIRMATION.store(false, Ordering::SeqCst);
        true
    });

    stdin::register_keypress_callback(Key::Normal('o'), |_| {
        SHOW_OVERLAY_TEXT.fetch_xor(true, Ordering::SeqCst);
        true
    });

    stdin::register_keypress_callback(Key::Normal('t'), |_| {
        send_debug!("This is a test debug message.");
        send_info!("This is a test info message.");
        send_warn!("This is a test warn message.");
        send_error!("This is a test error message.");
        true
    });

    register_file_select_keypress_callbacks();
}
