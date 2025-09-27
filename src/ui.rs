use std::sync::{
    Mutex,
    atomic::{AtomicBool, Ordering},
};

use unicode_width::UnicodeWidthChar;

use crate::{
    playlist::PLAYLIST,
    term::{RenderWrapper, TERM_DEFAULT_BG, TERM_DEFAULT_FG},
    util::{Cell, Color, TextBoxInfo},
};

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
            wrap.cells[p].fg = Color::mix(wrap.cells[p].fg, color, opacity);
            wrap.cells[p].bg = Color::mix(wrap.cells[p].bg, color, opacity);
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
        if ch == ' ' {
            if wrap.cells[p].c.is_some() {
                wrap.cells[p].c = Some(' ');
            }
            cx += 1;
            continue;
        }
        wrap.cells[p] = Cell {
            c: Some(ch),
            fg: fg.unwrap_or(if wrap.cells[p].c == None {
                TERM_DEFAULT_FG
            } else {
                wrap.cells[p].fg
            }),
            bg: bg.unwrap_or(if wrap.cells[p].c == None {
                Color::halfhalf(wrap.cells[p].fg, wrap.cells[p].bg)
            } else {
                wrap.cells[p].bg
            }),
        };
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
    let (def_fg, def_bg) = TEXTBOX_DEFAULT_COLOR.lock().unwrap().clone();
    let (fg, bg) = (fg.or(def_fg), bg.or(def_bg));
    let (x, y, w, h, i, j) = TEXTBOX.get();
    let (_, cx, cy) = putat(wrap, text, i, j, w, h, x, y, fg, bg, TEXTBOX.getwrap());
    TEXTBOX.set(x, y, w, h, cx, cy);
}

pub fn putln(wrap: &mut RenderWrapper, text: &str, fg: Option<Color>, bg: Option<Color>) {
    let (def_fg, def_bg) = TEXTBOX_DEFAULT_COLOR.lock().unwrap().clone();
    let (fg, bg) = (fg.or(def_fg), bg.or(def_bg));
    let (x, y, w, h, i, j) = TEXTBOX.get();
    let (_, _, cy) = putat(wrap, text, i, j, w, h, x, y, fg, bg, TEXTBOX.getwrap());
    TEXTBOX.set(x, y, w, h, x, cy + 1);
}

macro_rules! put {
    ($wrap:expr, $($arg:tt)*) => {
        crate::ui::put($wrap, &format!($($arg)*), None, None)
    };
}

pub fn render_ui(wrap: &mut RenderWrapper) {
    render_overlay_text(wrap);
    render_playlist(wrap);
}

macro_rules! putln {
    ($wrap:expr, $($arg:tt)*) => {
        crate::ui::putln($wrap, &format!($($arg)*), None, None)
    };
}

pub static SHOW_PLAYLIST: AtomicBool = AtomicBool::new(false);

#[rustfmt::skip]
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

    putln!(wrap, "tvid v{}", env!("CARGO_PKG_VERSION"));
    putln!(wrap, "Press 'q' to quit, 'n' to skip to next, 'l' for playlist");
    putln!(wrap, "Playing: {}", wrap.playing);
    putln!(wrap, "Time: {}", time_str);
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
        .unwrap()
        .clone_from(&(Some(TERM_DEFAULT_BG), None));

    putln!(wrap, "Playlist ({} items):", PLAYLIST.lock().unwrap().len());

    let playing_index = PLAYLIST.lock().unwrap().get_pos().clone();
    for (i, item) in PLAYLIST.lock().unwrap().get_items().iter().enumerate() {
        let icon = if i == playing_index { "▶" } else { " " };
        putln!(wrap, "{} {}", icon, item);
    }
}
