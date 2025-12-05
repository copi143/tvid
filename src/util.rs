use parking_lot::Mutex;
use std::fmt::{Debug, Display};
use std::io::Write;
use std::ops::Mul;
use std::sync::atomic::{AtomicBool, AtomicIsize, AtomicUsize, Ordering};
use std::time::Duration;
use tokio::task::JoinHandle;

use crate::APP_START_TIME;
use crate::avsync::played_time_or_none;

pub struct XY {
    x: AtomicUsize,
    y: AtomicUsize,
}

impl XY {
    pub const fn new() -> Self {
        XY {
            x: AtomicUsize::new(0),
            y: AtomicUsize::new(0),
        }
    }

    pub fn set(&self, x: usize, y: usize) {
        self.x.store(x, Ordering::SeqCst);
        self.y.store(y, Ordering::SeqCst);
    }

    pub fn get(&self) -> (usize, usize) {
        (self.x.load(Ordering::SeqCst), self.y.load(Ordering::SeqCst))
    }

    pub fn x(&self) -> usize {
        self.x.load(Ordering::SeqCst)
    }

    pub fn y(&self) -> usize {
        self.y.load(Ordering::SeqCst)
    }
}

pub struct TBLR {
    top: AtomicUsize,
    bottom: AtomicUsize,
    left: AtomicUsize,
    right: AtomicUsize,
}

impl TBLR {
    pub const fn new() -> Self {
        TBLR {
            top: AtomicUsize::new(0),
            bottom: AtomicUsize::new(0),
            left: AtomicUsize::new(0),
            right: AtomicUsize::new(0),
        }
    }

    pub fn set(&self, top: usize, bottom: usize, left: usize, right: usize) {
        self.top.store(top, Ordering::SeqCst);
        self.bottom.store(bottom, Ordering::SeqCst);
        self.left.store(left, Ordering::SeqCst);
        self.right.store(right, Ordering::SeqCst);
    }

    pub fn get(&self) -> (usize, usize, usize, usize) {
        (
            self.top.load(Ordering::SeqCst),
            self.bottom.load(Ordering::SeqCst),
            self.left.load(Ordering::SeqCst),
            self.right.load(Ordering::SeqCst),
        )
    }

    pub fn top(&self) -> usize {
        self.top.load(Ordering::SeqCst)
    }

    pub fn bottom(&self) -> usize {
        self.bottom.load(Ordering::SeqCst)
    }

    pub fn left(&self) -> usize {
        self.left.load(Ordering::SeqCst)
    }

    pub fn right(&self) -> usize {
        self.right.load(Ordering::SeqCst)
    }
}

pub struct TextBoxInfo {
    pub x: AtomicIsize,
    pub y: AtomicIsize,
    pub w: AtomicUsize,
    pub h: AtomicUsize,
    pub i: AtomicIsize,
    pub j: AtomicIsize,
    pub autowrap: AtomicBool,
}

impl TextBoxInfo {
    pub const fn new() -> Self {
        TextBoxInfo {
            x: AtomicIsize::new(0),
            y: AtomicIsize::new(0),
            w: AtomicUsize::new(0),
            h: AtomicUsize::new(0),
            i: AtomicIsize::new(0),
            j: AtomicIsize::new(0),
            autowrap: AtomicBool::new(false),
        }
    }

    pub fn set(&self, x: isize, y: isize, w: usize, h: usize, i: isize, j: isize) {
        self.x.store(x, Ordering::SeqCst);
        self.y.store(y, Ordering::SeqCst);
        self.w.store(w, Ordering::SeqCst);
        self.h.store(h, Ordering::SeqCst);
        self.i.store(i, Ordering::SeqCst);
        self.j.store(j, Ordering::SeqCst);
    }

    pub fn get(&self) -> (isize, isize, usize, usize, isize, isize) {
        (
            self.x.load(Ordering::SeqCst),
            self.y.load(Ordering::SeqCst),
            self.w.load(Ordering::SeqCst),
            self.h.load(Ordering::SeqCst),
            self.i.load(Ordering::SeqCst),
            self.j.load(Ordering::SeqCst),
        )
    }

    pub fn x(&self) -> isize {
        self.x.load(Ordering::SeqCst)
    }

    pub fn y(&self) -> isize {
        self.y.load(Ordering::SeqCst)
    }

    pub fn w(&self) -> usize {
        self.w.load(Ordering::SeqCst)
    }

    pub fn h(&self) -> usize {
        self.h.load(Ordering::SeqCst)
    }

    pub fn i(&self) -> isize {
        self.i.load(Ordering::SeqCst)
    }

    pub fn j(&self) -> isize {
        self.j.load(Ordering::SeqCst)
    }

    pub fn setwrap(&self, autowrap: bool) {
        self.autowrap.store(autowrap, Ordering::SeqCst);
    }

    pub fn getwrap(&self) -> bool {
        self.autowrap.load(Ordering::SeqCst)
    }
}

// @ ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== @

/// 标准 srgb 2.2
pub fn gamma_correct(value: f32) -> f32 {
    if value <= 0.0 {
        0.0
    } else if value <= 0.0031308 {
        value * 12.92
    } else if value <= 1.0 {
        1.055 * value.powf(1.0 / 2.4) - 0.055
    } else {
        1.0
    }
}

/// 标准 srgb 2.2
pub fn gamma_reverse(value: f32) -> f32 {
    if value <= 0.0 {
        0.0
    } else if value <= 0.04045 {
        value / 12.92
    } else if value <= 1.0 {
        ((value + 0.055) / 1.055).powf(2.4)
    } else {
        1.0
    }
}

#[repr(C)]
#[derive(Clone, Copy, PartialEq, PartialOrd)]
pub struct ColorF32 {
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
}

impl Mul<f32> for ColorF32 {
    type Output = ColorF32;

    fn mul(self, rhs: f32) -> Self::Output {
        ColorF32 {
            r: self.r * rhs,
            g: self.g * rhs,
            b: self.b * rhs,
            a: self.a,
        }
    }
}

impl ColorF32 {
    pub fn mix(fg: ColorF32, bg: ColorF32, t: f32) -> Self {
        ColorF32 {
            r: fg.r * t + bg.r * (1.0 - t),
            g: fg.g * t + bg.g * (1.0 - t),
            b: fg.b * t + bg.b * (1.0 - t),
            a: fg.a * t + bg.a * (1.0 - t),
        }
    }

    pub fn luminance(&self) -> f32 {
        0.2126 * self.r + 0.7152 * self.g + 0.0722 * self.b
    }
}

impl From<Color> for ColorF32 {
    fn from(c: Color) -> Self {
        ColorF32 {
            r: gamma_reverse(c.r as f32 / 255.0),
            g: gamma_reverse(c.g as f32 / 255.0),
            b: gamma_reverse(c.b as f32 / 255.0),
            a: c.a as f32 / 255.0,
        }
    }
}

impl From<ColorF32> for Color {
    fn from(c: ColorF32) -> Self {
        Color {
            r: (gamma_correct(c.r) * 255.0) as u8,
            g: (gamma_correct(c.g) * 255.0) as u8,
            b: (gamma_correct(c.b) * 255.0) as u8,
            a: (c.a * 255.0) as u8,
        }
    }
}

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl Default for Color {
    fn default() -> Self {
        Color {
            r: 0,
            g: 0,
            b: 0,
            a: 255,
        }
    }
}

impl Display for Color {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{};{};{}", self.r, self.g, self.b)
    }
}

impl Mul<f32> for Color {
    type Output = Color;

    fn mul(self, rhs: f32) -> Self::Output {
        Color {
            r: (self.r as f32 * rhs).clamp(0.0, 255.0) as u8,
            g: (self.g as f32 * rhs).clamp(0.0, 255.0) as u8,
            b: (self.b as f32 * rhs).clamp(0.0, 255.0) as u8,
            a: self.a,
        }
    }
}

impl Color {
    pub const fn new(r: u8, g: u8, b: u8) -> Self {
        Color { r, g, b, a: 255 }
    }

    pub const fn transparent() -> Self {
        Color {
            r: 0,
            g: 0,
            b: 0,
            a: 0,
        }
    }

    pub const fn is_transparent(&self) -> bool {
        self.a == 0
    }

    pub fn halfhalf(a: Color, b: Color) -> Self {
        Color::mix(a, b, 0.5)
    }

    pub fn luminance(&self) -> u8 {
        (gamma_correct(self.as_f32().luminance()) * 255.0) as u8
    }

    pub fn as_f32(&self) -> ColorF32 {
        ColorF32::from(*self)
    }

    pub fn mix(fg: Color, bg: Color, t: f32) -> Self {
        let fg = ColorF32::from(fg);
        let bg = ColorF32::from(bg);
        Color::from(ColorF32::mix(fg, bg, t))
    }

    pub fn similar_to(&self, other: &Color, threshold: f32) -> bool {
        let (c1, c2) = (self.as_f32(), other.as_f32());
        let dr = c1.r - c2.r;
        let dg = c1.g - c2.g;
        let db = c1.b - c2.b;
        dr * dr + dg * dg + db * db < threshold * threshold
    }
}

pub fn best_contrast_color(bg: Color) -> Color {
    let r = if bg.r < 128 { 255 } else { 0 };
    let g = if bg.g < 128 { 255 } else { 0 };
    let b = if bg.b < 128 { 255 } else { 0 };
    Color::new(r, g, b)
}

// @ ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== @

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Cell {
    /// 单元格内的字符
    /// - `None` 表示什么都没有
    /// - `Some('\0')` 表示占位符，这之前应当有一个宽度大于 1 的字符
    pub c: Option<char>,
    pub fg: Color,
    pub bg: Color,
}

impl Cell {
    pub const fn new(c: char, fg: Color, bg: Color) -> Self {
        Cell { c: Some(c), fg, bg }
    }

    pub const fn transparent() -> Self {
        Cell {
            c: Some(' '),
            fg: Color::transparent(),
            bg: Color::transparent(),
        }
    }
}

// @ ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== @

pub fn some_if_eq<T: PartialEq>(a: T, b: T) -> Option<T> {
    if a == b { Some(a) } else { None }
}

pub fn some_if_ne<T: PartialEq>(a: T, b: T) -> Option<T> {
    if a == b { None } else { Some(a) }
}

// @ ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== @

mod palette256 {
    use crate::util::Color;

    pub const ANSI_COLORS: [Color; 16] = [
        Color::new(0, 0, 0),
        Color::new(205, 0, 0),
        Color::new(0, 205, 0),
        Color::new(205, 205, 0),
        Color::new(0, 0, 238),
        Color::new(205, 0, 205),
        Color::new(0, 205, 205),
        Color::new(229, 229, 229),
        Color::new(127, 127, 127),
        Color::new(255, 0, 0),
        Color::new(0, 255, 0),
        Color::new(255, 255, 0),
        Color::new(92, 92, 255),
        Color::new(255, 0, 255),
        Color::new(0, 255, 255),
        Color::new(255, 255, 255),
    ];

    pub const fn scale(c: u8) -> u8 {
        if c == 0 { 0 } else { c * 40 + 55 }
    }

    pub const fn reverse(c: u8) -> u8 {
        if c < 35 { 0 } else { (c - 35) / 40 }
    }

    pub const fn try_reverse(c: u8) -> Option<u8> {
        match c {
            0 => Some(0),
            95 => Some(1),
            135 => Some(2),
            175 => Some(3),
            215 => Some(4),
            255 => Some(5),
            _ => None,
        }
    }

    pub const fn gray(c: u8) -> u8 {
        c * 10 + 8
    }

    pub const fn gray_try_reverse(c: u8) -> Option<u8> {
        if c >= 8 && c <= 238 && (c - 8) % 10 == 0 {
            Some((c - 8) / 10)
        } else {
            None
        }
    }
}

pub fn palette256_to_color(index: u8) -> Color {
    if index < 16 {
        palette256::ANSI_COLORS[index as usize]
    } else if index < 232 {
        let r = palette256::scale(index / 36);
        let g = palette256::scale(index % 36 / 6);
        let b = palette256::scale(index % 6);
        Color::new(r, g, b)
    } else {
        let c = palette256::gray(index - 232);
        Color::new(c, c, c)
    }
}

pub fn palette256_from_color(c: Color) -> u8 {
    let r = palette256::reverse(c.r);
    let g = palette256::reverse(c.g);
    let b = palette256::reverse(c.b);
    r * 36 + g * 6 + b + 16
}

pub fn try_palette256(c: Color) -> Option<u8> {
    if let Some(ri) = palette256::try_reverse(c.r) {
        if let Some(gi) = palette256::try_reverse(c.g) {
            if let Some(bi) = palette256::try_reverse(c.b) {
                return Some(ri * 36 + gi * 6 + bi + 16);
            }
        }
    }

    if let Some(i) = palette256::gray_try_reverse(c.g) {
        if c.r == c.g && c.g == c.b {
            return Some(i + 232);
        }
    }

    None
}

// @ ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== @

/// 颜色模式
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ColorMode {
    /// 真彩色模式，仅使用 24 位真彩色
    #[default]
    TrueColorOnly,
    /// 256 色模式优先，如果颜色无法表示则使用真彩色
    Palette256Prefer,
    /// 仅使用 256 色模式
    Palette256Only,
    /// 灰度模式
    GrayScale,
    /// 黑白模式
    BlackWhite,
}

impl Display for ColorMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match locale!() {
            "zh-cn" => match self {
                ColorMode::TrueColorOnly => write!(f, "真彩色模式"),
                ColorMode::Palette256Prefer => write!(f, "256 色优先"),
                ColorMode::Palette256Only => write!(f, "仅 256 色"),
                ColorMode::GrayScale => write!(f, "灰度模式"),
                ColorMode::BlackWhite => write!(f, "黑白模式"),
            },
            "zh-tw" => match self {
                ColorMode::TrueColorOnly => write!(f, "真彩色模式"),
                ColorMode::Palette256Prefer => write!(f, "256 色優先"),
                ColorMode::Palette256Only => write!(f, "僅 256 色"),
                ColorMode::GrayScale => write!(f, "灰階模式"),
                ColorMode::BlackWhite => write!(f, "黑白模式"),
            },
            "ja-jp" => match self {
                ColorMode::TrueColorOnly => write!(f, "フルカラー"),
                ColorMode::Palette256Prefer => write!(f, "256色優先"),
                ColorMode::Palette256Only => write!(f, "256色のみ"),
                ColorMode::GrayScale => write!(f, "グレースケール"),
                ColorMode::BlackWhite => write!(f, "白黒モード"),
            },
            "fr-fr" => match self {
                ColorMode::TrueColorOnly => write!(f, "Couleurs vraies"),
                ColorMode::Palette256Prefer => write!(f, "Palette 256 couleurs prioritaire"),
                ColorMode::Palette256Only => write!(f, "Palette 256 couleurs uniquement"),
                ColorMode::GrayScale => write!(f, "Niveaux de gris"),
                ColorMode::BlackWhite => write!(f, "Noir et blanc"),
            },
            "de-de" => match self {
                ColorMode::TrueColorOnly => write!(f, "Truecolor-Modus"),
                ColorMode::Palette256Prefer => write!(f, "256-Farben-Priorität"),
                ColorMode::Palette256Only => write!(f, "Nur 256 Farben"),
                ColorMode::GrayScale => write!(f, "Graustufenmodus"),
                ColorMode::BlackWhite => write!(f, "Schwarz-Weiß-Modus"),
            },
            "es-es" => match self {
                ColorMode::TrueColorOnly => write!(f, "Modo de color verdadero"),
                ColorMode::Palette256Prefer => write!(f, "Prioridad de paleta de 256 colores"),
                ColorMode::Palette256Only => write!(f, "Solo paleta de 256 colores"),
                ColorMode::GrayScale => write!(f, "Modo de escala de grises"),
                ColorMode::BlackWhite => write!(f, "Modo blanco y negro"),
            },
            _ => match self {
                ColorMode::TrueColorOnly => write!(f, "True Color Mode"),
                ColorMode::Palette256Prefer => write!(f, "256 Color Palette Prefer"),
                ColorMode::Palette256Only => write!(f, "256 Color Palette Only"),
                ColorMode::GrayScale => write!(f, "Gray Scale Mode"),
                ColorMode::BlackWhite => write!(f, "Black and White Mode"),
            },
        }
    }
}

impl ColorMode {
    pub const fn new() -> Self {
        ColorMode::TrueColorOnly
    }

    pub const fn default() -> Self {
        ColorMode::TrueColorOnly
    }

    pub const fn switch_next(&mut self) {
        *self = match self {
            ColorMode::TrueColorOnly => ColorMode::Palette256Prefer,
            ColorMode::Palette256Prefer => ColorMode::Palette256Only,
            ColorMode::Palette256Only => ColorMode::GrayScale,
            ColorMode::GrayScale => ColorMode::BlackWhite,
            ColorMode::BlackWhite => ColorMode::TrueColorOnly,
        };
    }
}

// @ ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== @

#[inline(always)]
pub fn escape_set_color(
    wr: &mut impl Write,
    mut fg: Option<Color>,
    mut bg: Option<Color>,
    mode: ColorMode,
) {
    let mut b = true;
    if fg.is_some() && fg.unwrap().is_transparent() {
        if b {
            write!(wr, "\x1b[m").unwrap();
            b = false;
        }
        fg = None;
    };
    if bg.is_some() && bg.unwrap().is_transparent() {
        if b {
            write!(wr, "\x1b[m").unwrap();
            b = false;
        }
        bg = None;
    };
    match mode {
        ColorMode::TrueColorOnly => escape_set_color_rgb(wr, fg, bg),
        ColorMode::Palette256Prefer => escape_set_color_256_prefer(wr, fg, bg),
        ColorMode::Palette256Only => escape_set_color_256(wr, fg, bg),
        ColorMode::GrayScale => escape_set_color_gray_scale(wr, fg, bg),
        ColorMode::BlackWhite => escape_set_color_black_white(wr, fg, bg),
    }
}

#[inline(always)]
pub fn escape_set_color_rgb(wr: &mut impl Write, fg: Option<Color>, bg: Option<Color>) {
    match (fg, bg) {
        (Some(fg), Some(bg)) => write!(wr, "\x1b[38;2;{fg};48;2;{bg}m"),
        (Some(fg), None) => write!(wr, "\x1b[38;2;{}m", fg),
        (None, Some(bg)) => write!(wr, "\x1b[48;2;{}m", bg),
        (None, None) => Ok(()),
    }
    .unwrap()
}

#[inline(always)]
pub fn escape_set_color_256_prefer(wr: &mut impl Write, fg: Option<Color>, bg: Option<Color>) {
    match (fg, bg) {
        (Some(fg), Some(bg)) => match (try_palette256(fg), try_palette256(bg)) {
            (Some(fgi), Some(bgi)) => write!(wr, "\x1b[38;5;{fgi};48;5;{bgi}m"),
            (Some(fgi), None) => write!(wr, "\x1b[38;5;{fgi};48;2;{bg}m"),
            (None, Some(bgi)) => write!(wr, "\x1b[38;2;{fg};48;5;{bgi}m"),
            (None, None) => write!(wr, "\x1b[38;2;{fg};48;2;{bg}m"),
        },
        (Some(fg), None) => match try_palette256(fg) {
            Some(fgi) => write!(wr, "\x1b[38;5;{fgi}m"),
            None => write!(wr, "\x1b[38;2;{fg}m"),
        },
        (None, Some(bg)) => match try_palette256(bg) {
            Some(bgi) => write!(wr, "\x1b[48;5;{bgi}m"),
            None => write!(wr, "\x1b[48;2;{bg}m"),
        },
        (None, None) => Ok(()),
    }
    .unwrap()
}

#[inline(always)]
pub fn escape_set_color_256(wr: &mut impl Write, fg: Option<Color>, bg: Option<Color>) {
    match (fg, bg) {
        (Some(fg), Some(bg)) => {
            let (fgi, bgi) = (palette256_from_color(fg), palette256_from_color(bg));
            write!(wr, "\x1b[38;5;{};48;5;{}m", fgi, bgi)
        }
        (Some(fg), None) => write!(wr, "\x1b[38;5;{}m", palette256_from_color(fg)),
        (None, Some(bg)) => write!(wr, "\x1b[48;5;{}m", palette256_from_color(bg)),
        (None, None) => Ok(()),
    }
    .unwrap()
}

#[inline(always)]
pub fn escape_set_color_gray_scale(wr: &mut impl Write, fg: Option<Color>, bg: Option<Color>) {
    match (fg, bg) {
        (Some(fg), Some(bg)) => {
            let c1 = fg.luminance();
            let c2 = bg.luminance();
            write!(wr, "\x1b[38;2;{c1};{c1};{c1};48;2;{c2};{c2};{c2}m")
        }
        (Some(fg), None) => {
            let c = fg.luminance();
            write!(wr, "\x1b[38;2;{c};{c};{c}m")
        }
        (None, Some(bg)) => {
            let c = bg.luminance();
            write!(wr, "\x1b[48;2;{c};{c};{c}m")
        }
        (None, None) => Ok(()),
    }
    .unwrap()
}

#[inline(always)]
pub fn escape_set_color_black_white(wr: &mut impl Write, fg: Option<Color>, bg: Option<Color>) {
    match (fg, bg) {
        (Some(fg), Some(bg)) => {
            let fgi = if fg.luminance() < 128 { 30 } else { 97 };
            let bgi = if bg.luminance() < 128 { 40 } else { 107 };
            write!(wr, "\x1b[{};{}m", fgi, bgi)
        }
        (Some(fg), None) => {
            let fgi = if fg.luminance() < 128 { 30 } else { 97 };
            write!(wr, "\x1b[{}m", fgi)
        }
        (None, Some(bg)) => {
            let bgi = if bg.luminance() < 128 { 40 } else { 107 };
            write!(wr, "\x1b[{}m", bgi)
        }
        (None, None) => Ok(()),
    }
    .unwrap()
}

// @ ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== @

#[allow(async_fn_in_trait)]
pub trait JoinAll {
    type Output;
    async fn join_all(self) -> Vec<Self::Output>;
}

impl<T: Send + 'static> JoinAll for Vec<JoinHandle<T>> {
    type Output = T;
    async fn join_all(self) -> Vec<T> {
        let mut results = Vec::with_capacity(self.len());
        for handle in self {
            results.push(handle.await.unwrap());
        }
        results
    }
}

// @ ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== @

pub fn calc_played_time() -> (Option<Duration>, Duration) {
    static LAST_PLAYED_TIME: Mutex<Option<Duration>> = Mutex::new(None);
    let played_time = played_time_or_none();
    let mut lock = LAST_PLAYED_TIME.lock();
    let delta_played_time = lock
        .map(|t1| played_time.map(|t2| t2.saturating_sub(t1)))
        .flatten()
        .unwrap_or(Duration::ZERO);
    if let Some(played_time) = played_time {
        *lock = Some(played_time);
    }
    (played_time, delta_played_time)
}

pub fn calc_app_time() -> (Duration, Duration) {
    static LAST_APP_TIME: Mutex<Duration> = Mutex::new(Duration::ZERO);
    let app_time = APP_START_TIME.get().unwrap().elapsed();
    let mut lock = LAST_APP_TIME.lock();
    let delta_time = app_time.saturating_sub(*lock);
    *lock = app_time;
    (app_time, delta_time)
}

// @ ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== @
