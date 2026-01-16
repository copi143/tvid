use parking_lot::Mutex;
#[cfg(feature = "unicode")]
use unicode_width::UnicodeWidthChar;

use crate::render::ContextWrapper;
use crate::util::{Cell, Color, TextBoxInfo, best_contrast_color};

// @ ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== @

/// Unifont 字体数据（转换为盲文字符的点阵）
/// - 每个字符 8x16 或 16x16 点阵，填充为 16x16 点阵
/// - 转换为盲文后正好 32 字节每字符
/// - 具体生成方法见 README
#[cfg(feature = "unifont")]
const UNIFONT: *const [u8; 32] = {
    let bytes = include_bytes!("../../unifont-17.0.01.bin");
    assert!(bytes.len() == 65536 * 32, "unifont data size incorrect");
    bytes.as_ptr() as *const [u8; 32]
};

#[cfg(feature = "unifont")]
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
    wrap: &mut ContextWrapper,
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
                // ASSUME 这里 '\0' 只会来自同一行的宽字符占位，不会出现在行首。
                if wrap.cells[p].c == Some('\0') {
                    let mut i = p - 1;
                    while wrap.cells[i].c == Some('\0') {
                        wrap.cells[i].c = Some(' ');
                        i -= 1;
                    }
                    wrap.cells[i].c = Some(' ');
                }
                // ASSUME 向后搜索时因为有哨兵不会越界
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
                wrap.cells[p].c = {
                    #[cfg(feature = "unicode")]
                    if i == 0 && j == 0 {
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
                    }
                    #[cfg(not(feature = "unicode"))]
                    if i == 0 && j == 0 {
                        Some('+')
                    } else if i == w - 1 && j == 0 {
                        Some('+')
                    } else if i == 0 && j == h - 1 {
                        Some('+')
                    } else if i == w - 1 && j == h - 1 {
                        Some('+')
                    } else if i == 0 || i == w - 1 {
                        Some('|')
                    } else {
                        Some('-')
                    }
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
    wrap: &mut ContextWrapper,
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
        #[cfg(feature = "unicode")]
        let cw = ch.width().unwrap_or(0) as isize;
        // 跳过不可见字符
        #[cfg(feature = "unicode")]
        if cw == 0 {
            pn += 1; // 假装是打印了
            continue;
        }
        #[cfg(not(feature = "unicode"))]
        if ch == '\0' || ch == '\n' || ch == '\r' {
            pn += 1; // 假装是打印了
            continue;
        }
        #[cfg(not(feature = "unicode"))]
        let cw = 1;
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
        // ASSUME 这里 '\0' 只会来自同一行的宽字符占位，不会出现在行首。
        if wrap.cells[p].c == Some('\0') {
            let mut i = p - 1;
            while wrap.cells[i].c == Some('\0') {
                wrap.cells[i].c = Some(' ');
                i -= 1;
            }
            wrap.cells[i].c = Some(' ');
        }
        // ASSUME 向后搜索时因为有哨兵不会越界
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
        #[cfg(feature = "unicode")]
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
        crate::ui::helper::putat($wrap, &format!($($arg)*), $x, $y, u16::MAX as usize, u16::MAX as usize, i16::MIN as isize, i16::MIN as isize, None, None, false);
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

pub fn put(wrap: &mut ContextWrapper, text: &str, fg: Option<Color>, bg: Option<Color>) {
    let (def_fg, def_bg) = *TEXTBOX_DEFAULT_COLOR.lock();
    let (fg, bg) = (fg.or(def_fg), bg.or(def_bg));
    let (x, y, w, h, i, j) = TEXTBOX.get();
    let (_, cx, cy) = putat(wrap, text, i, j, w, h, x, y, fg, bg, TEXTBOX.getwrap());
    TEXTBOX.set(x, y, w, h, cx, cy);
}

macro_rules! put {
    // ($wrap:expr, $text:expr) => {
    //     crate::ui::put($wrap, &$text, None, None)
    // };
    ($wrap:expr, $($arg:tt)*) => {
        crate::ui::helper::put($wrap, &format!($($arg)*), None, None)
    };
}

pub fn putln(wrap: &mut ContextWrapper, text: &str, fg: Option<Color>, bg: Option<Color>) {
    let (def_fg, def_bg) = *TEXTBOX_DEFAULT_COLOR.lock();
    let (fg, bg) = (fg.or(def_fg), bg.or(def_bg));
    let (x, y, w, h, i, j) = TEXTBOX.get();
    let (_, _, cy) = putat(wrap, text, i, j, w, h, x, y, fg, bg, TEXTBOX.getwrap());
    TEXTBOX.set(x, y, w, h, x, cy + 1);
}

macro_rules! putln {
    ($wrap:expr, $($arg:tt)*) => {
        crate::ui::helper::putln($wrap, &format!($($arg)*), None, None)
    };
}

macro_rules! putlns {
    ($wrap:expr; $($(#[$meta:meta])* $fmt:literal $(, $args:expr)*);+ $(;)?) => {{
        $(
            $(
                #[$meta]
            )*
            putln!($wrap, $fmt $(, $args)*);
        )+
    }};
}

// @ ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== @

#[cfg(feature = "unifont")]
pub fn putufln(wrap: &mut ContextWrapper, text: &str, fg: Option<Color>, bg: Option<Color>) {
    let mut data = [const { String::new() }; 4];
    #[cfg(feature = "unicode")]
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
    #[cfg(not(feature = "unicode"))]
    for ch in text.chars() {
        let font = unifont_get(ch);
        for y in 0..4 {
            for x in 0..8 {
                data[y].push(char::from_u32(0x2800 + font[y * 8 + x] as u32).unwrap());
            }
        }
    }
    for text in data {
        putln(wrap, &text, fg, bg);
    }
}

#[cfg(feature = "unifont")]
macro_rules! putufln {
    ($wrap:expr, $($arg:tt)*) => {
        crate::ui::helper::putufln($wrap, &format!($($arg)*), None, None)
    };
}

#[cfg(feature = "unifont")]
macro_rules! putuflns {
    ($wrap:expr; $($(#[$meta:meta])* $fmt:literal $(, $args:expr)*);+ $(;)?) => {{
        $(
            $(
                #[$meta]
            )*
            putufln!($wrap, $fmt $(, $args)*);
        )+
    }};
}

// @ ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== @

pub const TERM_FONT_HEIGHT_THRESHOLD: f32 = 12.0;

pub fn font_large_enough(wrap: &ContextWrapper) -> bool {
    wrap.font_height > TERM_FONT_HEIGHT_THRESHOLD
}

pub fn putln_or_ufln(wrap: &mut ContextWrapper, text: &str, fg: Option<Color>, bg: Option<Color>) {
    #[cfg(feature = "unifont")]
    if font_large_enough(wrap) {
        putln(wrap, text, fg, bg);
    } else {
        putufln(wrap, text, fg, bg);
    }
    #[cfg(not(feature = "unifont"))]
    putln(wrap, text, fg, bg);
}

macro_rules! putln_or_ufln {
    ($wrap:expr, $($arg:tt)*) => {{
        #[cfg(feature = "unifont")]
        if crate::ui::helper::font_large_enough($wrap) {
            putln!($wrap, $($arg)*);
        } else {
            putufln!($wrap, $($arg)*);
        }
        #[cfg(not(feature = "unifont"))]
        putln!($wrap, $($arg)*);
    }};
}

macro_rules! putlns_or_uflns {
    ($wrap:expr; $($(#[$meta:meta])* $fmt:literal $(, $args:expr)*);+ $(;)?) => {{
        #[cfg(feature = "unifont")]
        if crate::ui::helper::font_large_enough($wrap) {
            putlns!($wrap; $($(#[$meta])* $fmt $(, $args)*);+);
        } else {
            putuflns!($wrap; $($(#[$meta])* $fmt $(, $args)*);+);
        }
        #[cfg(not(feature = "unifont"))]
        putlns!($wrap; $($(#[$meta])* $fmt $(, $args)*);+);
    }};
}
