use data_classes::{ToNext, ToPrev};
use parking_lot::Mutex;
use std::cmp::min;
use std::fs::FileType;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use unicode_width::UnicodeWidthChar;

use crate::logging::get_messages;
use crate::playlist::{PLAYLIST, PLAYLIST_SELECTED_INDEX, SHOW_PLAYLIST};
use crate::render::ContextWrapper;
use crate::statistics;
use crate::stdin::{self, Key, MouseAction};
use crate::term::{TERM_DEFAULT_BG, TERM_DEFAULT_FG};
use crate::util::{Cell, Color, TextBoxInfo, best_contrast_color};
use crate::{avsync, render};
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
        crate::ui::put($wrap, &format!($($arg)*), None, None)
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
        crate::ui::putln($wrap, &format!($($arg)*), None, None)
    };
}

macro_rules! putlns {
    ($wrap:expr; $($fmt:expr $(, $args:expr)*);+ $(;)?) => {{
        $(
            putln!($wrap, $fmt $(, $args)*);
        )+
    }};
}

// @ ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== @

pub fn putufln(wrap: &mut ContextWrapper, text: &str, fg: Option<Color>, bg: Option<Color>) {
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

macro_rules! putufln {
    ($wrap:expr, $($arg:tt)*) => {
        crate::ui::putufln($wrap, &format!($($arg)*), None, None)
    };
}

macro_rules! putuflns {
    ($wrap:expr; $($fmt:expr $(, $args:expr)*);+ $(;)?) => {{
        $(
            putufln!($wrap, $fmt $(, $args)*);
        )+
    }};
}

// @ ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== @

const TERM_FONT_HEIGHT_THRESHOLD: f32 = 12.0;

fn font_large_enough(wrap: &ContextWrapper) -> bool {
    wrap.font_height > TERM_FONT_HEIGHT_THRESHOLD
}

pub fn putln_or_ufln(wrap: &mut ContextWrapper, text: &str, fg: Option<Color>, bg: Option<Color>) {
    if font_large_enough(wrap) {
        putln(wrap, text, fg, bg);
    } else {
        putufln(wrap, text, fg, bg);
    }
}

macro_rules! putln_or_ufln {
    ($wrap:expr, $($arg:tt)*) => {
        if font_large_enough($wrap) {
            putln!($wrap, $($arg)*);
        } else {
            putufln!($wrap, $($arg)*);
        }
    };
}

macro_rules! putlns_or_uflns {
    ($wrap:expr; $($fmt:expr $(, $args:expr)*);+ $(;)?) => {
        if font_large_enough($wrap) {
            putlns!($wrap; $($fmt $(, $args)*);+);
        } else {
            putuflns!($wrap; $($fmt $(, $args)*);+);
        }
    };
}

// @ ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== @

/// 是否已经开始渲染第一帧，防止事件在此之前触发
static FIRST_RENDERED: AtomicBool = AtomicBool::new(false);

pub fn render_ui(wrap: &mut ContextWrapper) {
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

fn calc_bar_size(cells_width: usize, cells_height: usize, font_height: f32) -> (usize, usize) {
    let bar_w = cells_width as f64 * avsync::playback_progress() + 0.5;
    let bar_h = unsafe { PROGRESSBAR_HEIGHT } / font_height * 2.0;
    let bar_w = (bar_w as usize).clamp(0, cells_width);
    let bar_h = (bar_h as usize).clamp(1, cells_height * 2);
    (bar_w, bar_h)
}

fn render_progressbar(wrap: &mut ContextWrapper) {
    if !SHOW_PROGRESSBAR.load(Ordering::SeqCst) {
        return;
    }

    let (bar_w, bar_h) = calc_bar_size(wrap.cells_width, wrap.cells_height, wrap.font_height);

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
    stdin::register_mouse_callback(|_, m| {
        if !FIRST_RENDERED.load(Ordering::SeqCst) {
            return false;
        }
        let ctx = render::RENDER_CONTEXT.lock();

        let term_h = ctx.cells_height;
        let (_, bar_h) = calc_bar_size(ctx.cells_width, ctx.cells_height, ctx.font_height);
        let bar_h = bar_h.div_ceil(2);

        if unsafe { DRAGGING_PROGRESSBAR } {
            if m.left {
                let p = m.pos.0 as f64 / ctx.cells_width as f64;
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
            let p = m.pos.0 as f64 / ctx.cells_width as f64;
            ffmpeg::seek_request_absolute(p * avsync::total_duration().as_secs_f64());
            true
        } else {
            false
        }
    });
}

pub static SHOW_HELP: AtomicBool = AtomicBool::new(false);

fn render_help(wrap: &mut ContextWrapper) {
    if wrap.cells_width < 8 || wrap.cells_height < 8 {
        return; // 防炸
    }

    if !SHOW_HELP.load(Ordering::SeqCst) {
        return;
    }

    let w = if font_large_enough(wrap) { 54 } else { 204 };
    let h = if font_large_enough(wrap) { 12 } else { 42 };
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
    textbox(x + 2, y + 1, w - 4, h - 2, false);
    textbox_default_color(Some(TERM_DEFAULT_BG), None);
    match locale!() {
        "zh-cn" => putlns_or_uflns!(wrap;
            "               帮助信息 (按 h 关闭)               ";
            "--------------------------------------------------";
            "     q:           退出程序                        ";
            "     n:           下一项                          ";
            "     l:           打开/关闭播放列表               ";
            "     空格/回车:   选择文件                        ";
            "     w/s/↑/↓:     上/下移动                       ";
            "     a/d/←/→:     进入/返回目录                   ";
            "     h:           打开/关闭帮助                   ";
            "--------------------------------------------------";
        ),
        "zh-tw" => putlns_or_uflns!(wrap;
            "               幫助資訊 (按 h 關閉)               ";
            "--------------------------------------------------";
            "     q:           離開程式                        ";
            "     n:           下一項                          ";
            "     l:           開啟/關閉播放清單               ";
            "     空格/Enter:  選擇檔案                        ";
            "     w/s/↑/↓:     上/下移動                       ";
            "     a/d/←/→:     進入/返回目錄                   ";
            "     h:           開啟/關閉幫助                   ";
            "--------------------------------------------------";
        ),
        "ja-jp" => putlns_or_uflns!(wrap;
            "            ヘルプ情報 (hキーで閉じる)            ";
            "--------------------------------------------------";
            "     q:           プログラムを終了                ";
            "     n:           次の項目                        ";
            "     l:           プレイリストを開く/閉じる       ";
            "     スペース/エンター: ファイルを選択            ";
            "     w/s/↑/↓:     上/下に移動                     ";
            "     a/d/←/→:     ディレクトリに入る/戻る         ";
            "     h:           ヘルプを開く/閉じる             ";
            "--------------------------------------------------";
        ),
        "fr-fr" => putlns_or_uflns!(wrap;
            "  Informations d'aide (appuyez sur h pour fermer) ";
            "--------------------------------------------------";
            "   q:            Quitter le programme             ";
            "   n:            Élément suivant                  ";
            "   l:            Ouvrir/fermer la liste de lecture";
            "   Espace/Entrée: Sélectionner un fichier         ";
            "   w/s/↑/↓:      Déplacer vers le haut/bas        ";
            "   a/d/←/→:      Entrer/revenir au répertoire     ";
            "   h:            Ouvrir/fermer l'aide             ";
            "--------------------------------------------------";
        ),
        "de-de" => putlns_or_uflns!(wrap;
            " Hilfeinformationen (drücken Sie h zum Schließen) ";
            "--------------------------------------------------";
            "   q:           Programm beenden                  ";
            "   n:           Nächstes Element                  ";
            "   l:           Wiedergabeliste öffnen/schließen  ";
            "   Leertaste/Eingabetaste: Datei auswählen        ";
            "   w/s/↑/↓:     Nach oben/unten bewegen           ";
            "   a/d/←/→:     Verzeichnis betreten/zurückkehren ";
            "   h:           Hilfe öffnen/schließen            ";
            "--------------------------------------------------";
        ),
        "es-es" => putlns_or_uflns!(wrap;
            "   Información de ayuda (presione h para cerrar)  ";
            "--------------------------------------------------";
            "   q:           Salir del programa                ";
            "   n:           Siguiente elemento                ";
            "   l:           Abrir/cerrar lista de reproducción";
            "   Espacio/Enter: Seleccionar archivo             ";
            "   w/s/↑/↓:     Mover arriba/abajo                ";
            "   a/d/←/→:     Entrar/volver al directorio       ";
            "   h:           Abrir/cerrar ayuda                ";
            "--------------------------------------------------";
        ),
        _ => putlns_or_uflns!(wrap;
            "        Help Information (press h to close)       ";
            "--------------------------------------------------";
            "     q:            Quit the program               ";
            "     n:            Next item                      ";
            "     l:            Open/close playlist            ";
            "     Space/Enter:  Select file                    ";
            "     w/s/↑/↓:      Move up/down                   ";
            "     a/d/←/→:      Enter/return directory         ";
            "     h:            Open/close help                ";
            "--------------------------------------------------";
        ),
    }
}

pub static SHOW_OVERLAY_TEXT: AtomicBool = AtomicBool::new(true);

fn format_time(time: Option<Duration>) -> String {
    if let Some(t) = time {
        format!(
            "{:02}h {:02}m {:02}s {:03}ms",
            t.as_secs() / 3600,
            (t.as_secs() % 3600) / 60,
            t.as_secs() % 60,
            t.subsec_millis()
        )
    } else {
        "       N/A       ".to_string()
    }
}

// fn format_delay(dur: Option<Duration>) -> String {
//     let Some(dur) = dur else {
//         return "   N/A   ".to_string();
//     };
// }

fn format_bytes_count(count: usize) -> String {
    match count {
        _ if count >= (1 << 30) * 100 => format!("{:5.1} GiB", (count >> 20) as f64 / 1024.0),
        _ if count >= (1 << 30) * 10 => format!("{:5.2} GiB", (count >> 20) as f64 / 1024.0),
        _ if count >= (1 << 20) * 1000 => format!("{:5.3} GiB", (count >> 20) as f64 / 1024.0),
        _ if count >= (1 << 20) * 100 => format!("{:5.1} MiB", (count >> 10) as f64 / 1024.0),
        _ if count >= (1 << 20) * 10 => format!("{:5.2} MiB", (count >> 10) as f64 / 1024.0),
        _ if count >= (1 << 10) * 1000 => format!("{:5.3} MiB", (count >> 10) as f64 / 1024.0),
        _ if count >= (1 << 10) * 100 => format!("{:5.1} KiB", count as f64 / 1024.0),
        _ if count >= (1 << 10) * 10 => format!("{:5.2} KiB", count as f64 / 1024.0),
        _ if count >= 1000 => format!("{:5.3} KiB", count as f64 / 1024.0),
        _ => format!("{count:3} Bytes"),
    }
}

fn render_overlay_text(wrap: &mut ContextWrapper) {
    if wrap.cells_width < 8 || wrap.cells_height < 8 {
        return; // 防炸
    }

    if !SHOW_OVERLAY_TEXT.load(Ordering::SeqCst) {
        return;
    }

    let playing_time_str = format_time(wrap.played_time);

    let audio_offset_str = if avsync::has_audio() {
        format!(
            "{:+07.3}ms",
            (avsync::audio_played_time_or_zero().as_secs_f64()
                - avsync::played_time_or_zero().as_secs_f64())
                * 1000.0
        )
    } else {
        "   N/A   ".to_string()
    };

    let video_offset_str = if avsync::has_video() {
        format!(
            "{:+07.3}ms",
            (avsync::video_played_time_or_zero().as_secs_f64()
                - avsync::played_time_or_zero().as_secs_f64())
                * 1000.0
        )
    } else {
        "   N/A   ".to_string()
    };

    let app_time_str = format_time(Some(wrap.app_time));

    // 这边关闭 autowrap，防止 unifont 渲染出问题
    textbox(2, 1, wrap.cells_width - 4, wrap.cells_height - 2, false);

    let statistics = statistics::get(0);
    let statistics = statistics.lock();

    match locale!() {
        "zh-cn" => putlns_or_uflns!(wrap;
            "tvid v{}", env!("CARGO_PKG_VERSION");
            "按 'q' 退出，'n' 跳到下一项，'l' 打开播放列表";
            "{}: {}", if avsync::is_paused() { "暂停中" } else { "播放中" }, wrap.playing;
            "视频时间: {playing_time_str} (音频偏移: {audio_offset_str}, 视频偏移: {video_offset_str})";
            "应用开启时间: {app_time_str}";
            "转义字符串编码时间: {:.2?} (最近 60 次平均)", statistics.escape_string_encode_time.avg();
            "渲染时间: {:.2?} (最近 60 次平均)", statistics.render_time.avg();
            "输出时间: {:.2?} (最近 60 次平均)", statistics.output_time.avg();
            "输出字节数: {}", format_bytes_count(statistics.output_bytes.avg::<usize>());
            "视频跳过帧数: {}", statistics.video_skipped_frames;
            "总输出字节数: {}", format_bytes_count(statistics.total_output_bytes);
            "颜色模式: {}", wrap.color_mode;
            "绿幕模式: {}", wrap.chroma_mode;
        ),
        "zh-tw" => putlns_or_uflns!(wrap;
            "tvid v{}", env!("CARGO_PKG_VERSION");
            "按 'q' 離開，'n' 跳到下一項，'l' 打開播放清單";
            "{}: {}", if avsync::is_paused() { "暫停中" } else { "播放中" }, wrap.playing;
            "視頻時間: {playing_time_str} (音頻偏移: {audio_offset_str}, 視頻偏移: {video_offset_str})";
            "應用開啟時間: {app_time_str}";
            "轉義字串編碼時間: {:.2?} (最近 60 次平均)", statistics.escape_string_encode_time.avg();
            "渲染時間: {:.2?} (最近 60 次平均)", statistics.render_time.avg();
            "輸出時間: {:.2?} (最近 60 次平均)", statistics.output_time.avg();
            "輸出位元組數: {}", format_bytes_count(statistics.output_bytes.avg::<usize>());
            "視頻跳過幀數: {}", statistics.video_skipped_frames;
            "總輸出位元組數: {}", format_bytes_count(statistics.total_output_bytes);
            "顏色模式: {}", wrap.color_mode;
            "綠幕模式: {}", wrap.chroma_mode;
        ),
        "ja-jp" => putlns_or_uflns!(wrap;
            "tvid v{}", env!("CARGO_PKG_VERSION");
            "'q'で終了、'n'で次へ、'l'でプレイリストを開く";
            "{}: {}", if avsync::is_paused() { "一時停止中" } else { "再生中" }, wrap.playing;
            "ビデオ時間: {playing_time_str} (オーディオオフセット: {audio_offset_str}, ビデオオフセット: {video_offset_str})";
            "アプリ起動時間: {app_time_str}";
            "エスケープ文字列エンコード時間: {:.2?} (直近 60 回の平均)", statistics.escape_string_encode_time.avg();
            "レンダリング時間: {:.2?} (直近 60 回の平均)", statistics.render_time.avg();
            "出力時間: {:.2?} (直近 60 回の平均)", statistics.output_time.avg();
            "出力バイト数: {}", format_bytes_count(statistics.output_bytes.avg::<usize>());
            "ビデオスキップフレーム数: {}", statistics.video_skipped_frames;
            "総出力バイト数: {}", format_bytes_count(statistics.total_output_bytes);
            "カラーモード: {}", wrap.color_mode;
            "クロマモード: {}", wrap.chroma_mode;
        ),
        "fr-fr" => putlns_or_uflns!(wrap;
            "tvid v{}", env!("CARGO_PKG_VERSION");
            "Appuyez sur 'q' pour quitter, 'n' pour passer au suivant, 'l' pour la liste de lecture";
            "{}: {}", if avsync::is_paused() { "En pause" } else { "Lecture" }, wrap.playing;
            "Temps vidéo: {playing_time_str} (décalage audio: {audio_offset_str}, décalage vidéo: {video_offset_str})";
            "Temps d'exécution de l'application: {app_time_str}";
            "Temps d'encodage de la chaîne d'échappement: {:.2?} (moyenne des 60 dernières)", statistics.escape_string_encode_time.avg();
            "Temps de rendu: {:.2?} (moyenne des 60 dernières)", statistics.render_time.avg();
            "Temps de sortie: {:.2?} (moyenne des 60 dernières)", statistics.output_time.avg();
            "Nombre de bytes de sortie: {}", format_bytes_count(statistics.output_bytes.avg::<usize>());
            "Images vidéo sautées: {}", statistics.video_skipped_frames;
            "Nombre total de bytes de sortie: {}", format_bytes_count(statistics.total_output_bytes);
            "Mode couleur: {}", wrap.color_mode;
            "Mode chroma: {}", wrap.chroma_mode;
        ),
        "de-de" => putlns_or_uflns!(wrap;
            "tvid v{}", env!("CARGO_PKG_VERSION");
            "Drücken Sie 'q' zum Beenden, 'n' zum Überspringen zum Nächsten, 'l' für die Wiedergabeliste";
            "{}: {}", if avsync::is_paused() { "Pausiert" } else { "Wiedergabe" }, wrap.playing;
            "Videozeit: {playing_time_str} (Audio-Offset: {audio_offset_str}, Video-Offset: {video_offset_str})";
            "App-Zeit: {app_time_str}";
            "Escape-String-Kodierungszeit: {:.2?} (Durchschnitt der letzten 60)", statistics.escape_string_encode_time.avg();
            "Render-Zeit: {:.2?} (Durchschnitt der letzten 60)", statistics.render_time.avg();
            "Ausgabezeit: {:.2?} (Durchschnitt der letzten 60)", statistics.output_time.avg();
            "Ausgabe-Bytes: {}", format_bytes_count(statistics.output_bytes.avg::<usize>());
            "Übersprungene Videoframes: {}", statistics.video_skipped_frames;
            "Gesamt-Ausgabe-Bytes: {}", format_bytes_count(statistics.total_output_bytes);
            "Farbmodus: {}", wrap.color_mode;
            "Chroma-Modus: {}", wrap.chroma_mode;
        ),
        "es-es" => putlns_or_uflns!(wrap;
            "tvid v{}", env!("CARGO_PKG_VERSION");
            "Presione 'q' para salir, 'n' para saltar al siguiente, 'l' para la lista de reproducción";
            "{}: {}", if avsync::is_paused() { "Pausado" } else { "Reproduciendo" }, wrap.playing;
            "Tiempo de video: {playing_time_str} (desplazamiento de audio: {audio_offset_str}, desplazamiento de video: {video_offset_str})";
            "Tiempo de la aplicación: {app_time_str}";
            "Tiempo de codificación de cadena de escape: {:.2?} (promedio de los últimos 60)", statistics.escape_string_encode_time.avg();
            "Tiempo de renderizado: {:.2?} (promedio de los últimos 60)", statistics.render_time.avg();
            "Tiempo de salida: {:.2?} (promedio de los últimos 60)", statistics.output_time.avg();
            "Bytes de salida: {}", format_bytes_count(statistics.output_bytes.avg::<usize>());
            "Fotogramas de video omitidos: {}", statistics.video_skipped_frames;
            "Total de bytes de salida: {}", format_bytes_count(statistics.total_output_bytes);
            "Modo de color: {}", wrap.color_mode;
            "Modo de croma: {}", wrap.chroma_mode;
        ),
        _ => putlns_or_uflns!(wrap;
            "tvid v{}", env!("CARGO_PKG_VERSION");
            "Press 'q' to quit, 'n' to skip to next, 'l' for playlist";
            "{}: {}", if avsync::is_paused() { "Paused" } else { "Playing" }, wrap.playing;
            "Video Time: {playing_time_str} (a: {audio_offset_str}, v: {video_offset_str})";
            "App Time: {app_time_str}";
            "Escape String Encode Time: {:.2?} (avg over last 60)", statistics.escape_string_encode_time.avg();
            "Render Time: {:.2?} (avg over last 60)", statistics.render_time.avg();
            "Output Time: {:.2?} (avg over last 60)", statistics.output_time.avg();
            "Output Bytes: {}", format_bytes_count(statistics.output_bytes.avg::<usize>());
            "Video Skipped Frames: {}", statistics.video_skipped_frames;
            "Total Output Bytes: {}", format_bytes_count(statistics.total_output_bytes);
            "Color Mode: {}", wrap.color_mode;
            "Chroma Mode: {}", wrap.chroma_mode;
        ),
    }
}

fn render_playlist(wrap: &mut ContextWrapper) {
    if wrap.cells_width < 8 || wrap.cells_height < 8 {
        return; // 防炸
    }

    let playlist_width = if font_large_enough(wrap) {
        62.min(wrap.cells_width)
    } else {
        482.min(wrap.cells_width)
    };

    static mut PLAYLIST_POS: f32 = 0.0;
    let mut playlist_pos = unsafe { PLAYLIST_POS };
    if SHOW_PLAYLIST.load(Ordering::SeqCst) {
        playlist_pos += wrap.delta_time.as_secs_f32() * 3000.0 / wrap.font_width;
    } else {
        playlist_pos -= wrap.delta_time.as_secs_f32() * 3000.0 / wrap.font_width;
    }
    let playlist_pos = playlist_pos.clamp(0.0, playlist_width as f32);
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
        playlist_width,
        wrap.cells_height,
        Some(TERM_DEFAULT_BG),
        TERM_DEFAULT_FG,
        0.5,
    );

    textbox(
        wrap.cells_width.saturating_sub(playlist_pos) as isize + 1,
        1,
        playlist_width - 2,
        wrap.cells_height - 2,
        false,
    );

    textbox_default_color(Some(TERM_DEFAULT_BG), None);

    let len = PLAYLIST.lock().len();
    match locale!() {
        "zh-cn" => putln_or_ufln!(wrap, "播放列表 ({len} 项):"),
        "zh-tw" => putln_or_ufln!(wrap, "播放清單 ({len} 項):"),
        "ja-jp" => putln_or_ufln!(wrap, "プレイリスト ({len} アイテム):"),
        "fr-fr" => putln_or_ufln!(wrap, "Liste de lecture ({len} éléments):"),
        "de-de" => putln_or_ufln!(wrap, "Wiedergabeliste ({len} Elemente):"),
        "es-es" => putln_or_ufln!(wrap, "Lista de reproducción ({len} elementos):"),
        _ => putln_or_ufln!(wrap, "Playlist ({len} items):"),
    }

    let selected_index = *PLAYLIST_SELECTED_INDEX.lock();
    let playing_index = PLAYLIST.lock().get_pos();
    for (i, item) in PLAYLIST.lock().get_items().iter().enumerate() {
        // 这边的 U+2000 是故意占位的，因为 ▶ 符号在终端上渲染宽度是 2
        let icon = if i == playing_index { "▶ " } else { "  " };
        if i as isize == selected_index {
            putln_or_ufln(
                wrap,
                &format!("{icon}{item}"),
                Some(TERM_DEFAULT_FG),
                Some(TERM_DEFAULT_BG),
            );
        } else {
            putln_or_ufln!(wrap, "{icon}{item}");
        }
    }
}

fn render_messages(wrap: &mut ContextWrapper) {
    if wrap.cells_width < 8 || wrap.cells_height < 8 {
        return; // 防炸
    }

    let width = (wrap.cells_width * 4 / 10).max(50);

    if font_large_enough(wrap) {
        for (i, message) in get_messages().queue.iter().rev().enumerate() {
            let y = wrap.cells_height as isize - i as isize - 1;
            if y < 0 {
                continue;
            }
            mask(wrap, 0, y, width, 1, None, message.lv.level_color(), 0.5);
            textbox(0, y, width, 1, false);
            textbox_default_color(Some(TERM_DEFAULT_BG), None);
            putln(wrap, &message.msg, message.fg, message.bg);
        }
    } else {
        for (i, message) in get_messages().queue.iter().rev().enumerate() {
            let y = wrap.cells_height as isize - i as isize * 4 - 4;
            if y < 0 {
                continue;
            }
            mask(wrap, 0, y, width, 4, None, message.lv.level_color(), 0.5);
            textbox(0, y, width, 4, false);
            textbox_default_color(Some(TERM_DEFAULT_BG), None);
            putufln(wrap, &message.msg, message.fg, message.bg);
        }
    }
}

// @ ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== @

pub static FILE_SELECT: AtomicBool = AtomicBool::new(false);
pub static FILE_SELECT_PATH: Mutex<String> = Mutex::new(String::new());
pub static FILE_SELECT_LIST: Mutex<Vec<(FileType, String)>> = Mutex::new(Vec::new());
pub static FILE_SELECT_INDEX: Mutex<usize> = Mutex::new(0);

fn render_file_select(wrap: &mut ContextWrapper) {
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

    match locale!() {
        "zh-cn" => putlns_or_uflns!(wrap;
            "文件选择: {path}";
            "  > 使用方向键导航，空格选择，Q 取消。";
            "{}", "-".repeat(w - 2);
        ),
        "zh-tw" => putlns_or_uflns!(wrap;
            "檔案選擇: {path}";
            "  > 使用方向鍵導航，空格選擇，Q 取消。";
            "{}", "-".repeat(w - 2);
        ),
        "ja-jp" => putlns_or_uflns!(wrap;
            "ファイル選択: {path}";
            "  > 矢印キーで移動、スペースで選択、Qでキャンセル。";
            "{}", "-".repeat(w - 2);
        ),
        "fr-fr" => putlns_or_uflns!(wrap;
            "Sélection de fichier : {path}";
            "  > Utilisez les flèches pour naviguer, Espace pour sélectionner, Q pour annuler.";
            "{}", "-".repeat(w - 2);
        ),
        "de-de" => putlns_or_uflns!(wrap;
            "Datei auswählen: {path}";
            "  > Verwenden Sie die Pfeiltasten zum Navigieren, Leertaste zum Auswählen, Q zum Abbrechen.";
            "{}", "-".repeat(w - 2);
        ),
        "es-es" => putlns_or_uflns!(wrap;
            "Seleccionar archivo: {path}";
            "  > Use las flechas para navegar, Espacio para seleccionar, Q para cancelar.";
            "{}", "-".repeat(w - 2);
        ),
        _ => putlns_or_uflns!(wrap;
            "File Select: {path}";
            "  > Use arrow keys to navigate, Space to select, Q to cancel.";
            "{}", "-".repeat(w - 2);
        ),
    }

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

    let l = h - 2;
    let max_show = (if font_large_enough(wrap) { l } else { l / 4 }) - 3;
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
            putln_or_ufln(wrap, &text, Some(TERM_DEFAULT_FG), Some(TERM_DEFAULT_BG));
        } else {
            putln_or_ufln(wrap, &text, None, None);
        }
    }
}

fn register_file_select_keypress_callbacks() {
    stdin::register_keypress_callback(Key::Normal('q'), |_, _| {
        if !FILE_SELECT.load(Ordering::SeqCst) {
            return false;
        }
        FILE_SELECT.store(false, Ordering::SeqCst);
        true
    });

    let cb = |_, _| {
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
            error_l10n!(
                "zh-cn" => "无法打开非文件: {}", path;
                "zh-tw" => "無法開啟非檔案: {}", path;
                "ja-jp" => "ファイルでないものを開けません: {}", path;
                "fr-fr" => "Impossible d'ouvrir autre qu'un fichier : {}", path;
                "de-de" => "Kann keine Nicht-Datei öffnen: {}", path;
                "es-es" => "No se puede abrir no archivo: {}", path;
                _ => "Cannot open non-file: {}", path;
            );
        }
        true
    };
    stdin::register_keypress_callback(Key::Normal(' '), cb);
    stdin::register_keypress_callback(Key::Enter, cb);

    let cb = |_, _| {
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

    let cb = |_, _| {
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

    let cb = |_, _| {
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

    let cb = |_, _| {
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

fn render_quit_confirmation(wrap: &mut ContextWrapper) {
    if !QUIT_CONFIRMATION.load(Ordering::SeqCst) {
        return;
    }

    if wrap.cells_width < 8 || wrap.cells_height < 8 {
        return; // 防炸
    }

    let w = if font_large_enough(wrap) { 25 } else { 100 };
    let h = if font_large_enough(wrap) { 3 } else { 12 };
    let x = (wrap.cells_width as isize - w as isize) / 2;
    let y = (wrap.cells_height as isize - h as isize) / 2;
    mask(
        wrap,
        x - if font_large_enough(wrap) { 10 } else { 40 },
        y - if font_large_enough(wrap) { 2 } else { 8 },
        w + if font_large_enough(wrap) { 20 } else { 80 },
        h + if font_large_enough(wrap) { 4 } else { 16 },
        Some(TERM_DEFAULT_BG),
        TERM_DEFAULT_FG,
        0.5,
    );
    textbox(x, y, w, h, false);
    textbox_default_color(Some(TERM_DEFAULT_BG), None);
    match locale!() {
        "zh-cn" => putln_or_ufln!(wrap, "        确认退出？       "),
        "zh-tw" => putln_or_ufln!(wrap, "        確認離開？       "),
        "ja-jp" => putln_or_ufln!(wrap, "   終了を確認しますか？  "),
        "fr-fr" => putln_or_ufln!(wrap, " Confirmer la fermeture ?"),
        "de-de" => putln_or_ufln!(wrap, "   Beenden bestätigen?   "),
        "es-es" => putln_or_ufln!(wrap, "   ¿Confirmar salida?    "),
        _ => putln_or_ufln!(wrap, "      Confirm Quit?      "),
    }
    putln_or_ufln!(wrap, "-------------------------");
    putln_or_ufln!(wrap, "        q   /   c        ");
}

// @ ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== @

pub fn register_input_callbacks() {
    register_input_callbacks_progressbar();

    stdin::register_keypress_callback(Key::Normal('h'), |_, _| {
        SHOW_HELP.store(!SHOW_HELP.load(Ordering::SeqCst), Ordering::SeqCst);
        true
    });

    stdin::register_keypress_callback(Key::Normal('q'), |id, _| {
        if !QUIT_CONFIRMATION.load(Ordering::SeqCst) {
            return false;
        }
        if id == 0 {
            term::request_quit();
        }
        true
    });

    stdin::register_keypress_callback(Key::Normal('c'), |id, _| {
        if !QUIT_CONFIRMATION.load(Ordering::SeqCst) {
            return false;
        }
        if id == 0 {
            QUIT_CONFIRMATION.store(false, Ordering::SeqCst);
        }
        true
    });

    stdin::register_keypress_callback(Key::Lower('x'), |_, _| {
        let mut ctx = render::RENDER_CONTEXT.lock();
        ctx.chroma_mode.switch_to_next();
        true
    });

    stdin::register_keypress_callback(Key::Upper('x'), |_, _| {
        let mut ctx = render::RENDER_CONTEXT.lock();
        ctx.chroma_mode.switch_to_prev();
        true
    });

    stdin::register_keypress_callback(Key::Normal('o'), |_, _| {
        SHOW_OVERLAY_TEXT.fetch_xor(true, Ordering::SeqCst);
        true
    });

    stdin::register_keypress_callback(Key::Normal('t'), |_, _| {
        debug_l10n!(
            "zh-cn" => "这是一条测试调试信息。";
            "zh-tw" => "這是一條測試調試信息。";
            "ja-jp" => "これはテスト用のデバッグメッセージです。";
            "fr-fr" => "Ceci est un message de débogage de test.";
            "de-de" => "Dies ist eine Test-Debug-Nachricht.";
            "es-es" => "Este es un mensaje de depuración de prueba.";
            _       => "This is a test debug message.";
        );
        info_l10n!(
            "zh-cn" => "这是一条测试信息。";
            "zh-tw" => "這是一條測試信息。";
            "ja-jp" => "これはテスト用のメッセージです。";
            "fr-fr" => "Ceci est un message de test.";
            "de-de" => "Dies ist eine Testnachricht.";
            "es-es" => "Este es un mensaje de prueba.";
            _       => "This is a test message.";
        );
        warning_l10n!(
            "zh-cn" => "这是一条测试警告。";
            "zh-tw" => "這是一條測試警告。";
            "ja-jp" => "これはテスト用の警告です。";
            "fr-fr" => "Ceci est un avertissement de test.";
            "de-de" => "Dies ist eine Testwarnung.";
            "es-es" => "Esta es una advertencia de prueba.";
            _       => "This is a test warning.";
        );
        error_l10n!(
            "zh-cn" => "这是一条测试错误。";
            "zh-tw" => "這是一條測試錯誤。";
            "ja-jp" => "これはテスト用のエラーです。";
            "fr-fr" => "Ceci est une erreur de test.";
            "de-de" => "Dies ist ein Testfehler.";
            "es-es" => "Este es un error de prueba.";
            _       => "This is a test error.";
        );
        true
    });

    register_file_select_keypress_callbacks();
}
