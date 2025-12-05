use av::util::frame::video::Video as VideoFrame;
use ffmpeg_next as av;
use parking_lot::{Condvar, Mutex};
use std::io::Write as _;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};
use tokio::sync::Mutex as TokioMutex;
use unicode_width::UnicodeWidthChar;

use crate::playlist::PLAYLIST;
use crate::stdout::{pend_print, pending_frames, remove_pending_frames};
use crate::term::Winsize;
use crate::term::{self, TERM_QUIT};
use crate::{TOKIO_RUNTIME, statistics};
use crate::{avsync, util::*};

// @ ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== @

/// 终端大小 (字符)
pub static TERM_SIZE: XY = XY::new();
/// 终端大小 (像素)
pub static TERM_PIXELS: XY = XY::new();
/// 视频原始大小 (像素)
pub static VIDEO_ORIGIN_PIXELS: XY = XY::new();
/// 视频大小 (x: 字符, y: 半个字符)
pub static VIDEO_PIXELS: XY = XY::new();
/// 视频边缘填充大小 (字符)
pub static VIDEO_PADDING: TBLR = TBLR::new();

/// 更新终端和视频大小
/// - 计算新的视频显示大小和填充
/// - 如果大小有变化，重置渲染缓冲区
pub fn updatesize(xvideo: usize, yvideo: usize) {
    if (xvideo == 0 && yvideo != 0) || (xvideo != 0 && yvideo == 0) {
        panic!("Invalid video size: {xvideo}x{yvideo}");
    }

    let winsize = if let Some(winsize) = term::get_winsize() {
        winsize
    } else {
        Winsize {
            row: 24,
            col: 80,
            xpixel: 0,
            ypixel: 0,
        }
    };

    let (xchars, ychars) = (winsize.col as usize, winsize.row as usize);
    let (xpixels, ypixels) = (winsize.xpixel as usize, winsize.ypixel as usize);
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

    if TERM_SIZE.get() == (xchars, ychars) {
        if TERM_PIXELS.get() == (xpixels, ypixels) {
            if VIDEO_ORIGIN_PIXELS.get() == (xvideo, yvideo) {
                return;
            }
        }
    }

    TERM_SIZE.set(xchars, ychars);
    TERM_PIXELS.set(xpixels, ypixels);
    if xvideo != 0 && yvideo != 0 {
        VIDEO_ORIGIN_PIXELS.set(xvideo, yvideo);
    }
    let (xvideo, yvideo) = VIDEO_ORIGIN_PIXELS.get();

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

    TOKIO_RUNTIME.block_on(async {
        let (this_frame, last_frame) = &mut *FRAMES.lock().await;

        // 将大小加一作为哨兵
        this_frame.clear();
        this_frame.resize(xchars * ychars + 1, Cell::default());

        // 将大小加一作为哨兵
        last_frame.clear();
        last_frame.resize(xchars * ychars + 1, Cell::default());
    });

    remove_pending_frames();

    FORCEFLUSH_NEXT.store(true, Ordering::SeqCst);
}

// @ ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== @

/// 渲染回调的包装结构
#[allow(unused)]
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
    pub delta_played_time: Duration,
    pub app_time: Duration,
    pub delta_time: Duration,

    pub term_font_width: f32,
    pub term_font_height: f32,
}

#[allow(unused)]
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

// @ ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== @

/// frames.0 - 当前帧, frames.1 - 上一帧
static FRAMES: TokioMutex<(Vec<Cell>, Vec<Cell>)> = TokioMutex::const_new((Vec::new(), Vec::new()));
static RENDER_CALLBACKS: Mutex<Vec<fn(&mut RenderWrapper)>> = Mutex::new(Vec::new());

/// 强制下一帧全屏刷新
pub static FORCEFLUSH_NEXT: AtomicBool = AtomicBool::new(false);

/// 颜色模式，见 [`ColorMode`]
pub static COLOR_MODE: Mutex<ColorMode> = Mutex::new(ColorMode::new());

pub fn add_render_callback(callback: fn(&mut RenderWrapper<'_, '_>)) {
    RENDER_CALLBACKS.lock().push(callback);
}

async fn render(frame: &[Color], pitch: usize) {
    render_frame(frame, pitch).await;

    let mut force_flush = FORCEFLUSH_NEXT.swap(false, Ordering::SeqCst);
    let pf = pending_frames();
    if pf > 3 {
        error_l10n!(
            "zh-cn" => "待处理帧过多: {pf}";
            "zh-tw" => "待處理幀過多: {pf}";
            "ja-jp" => "保留フレームが多すぎます: {pf}";
            "fr-fr" => "Trop de trames en attente : {pf}";
            "de-de" => "Zu viele ausstehende Frames: {pf}";
            "es-es" => "Demasiados fotogramas pendientes: {pf}";
            _ => "Too many pending frames: {pf}";
        );
        force_flush = true;
    }
    print_diff(force_flush).await;

    let (this_frame, last_frame) = &mut *FRAMES.lock().await;
    std::mem::swap(this_frame, last_frame);
}

async fn render_frame(frame: &[Color], pitch: usize) {
    let (played_time, delta_played_time) = calc_played_time();

    let (app_time, delta_time) = calc_app_time();

    let (this_frame, _) = &mut *FRAMES.lock().await;

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
        delta_played_time,
        app_time,
        delta_time,
        term_font_width: TERM_PIXELS.x() as f32 / TERM_SIZE.x() as f32,
        term_font_height: TERM_PIXELS.y() as f32 / TERM_SIZE.y() as f32,
    };

    let instant = Instant::now();
    wrap.cells.fill(Cell::transparent());
    for callback in RENDER_CALLBACKS.lock().iter() {
        callback(wrap);
    }
    statistics::set_render_time(instant.elapsed());
}

// @ ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== @

pub const TERM_BACKGROUND: Option<Color> = None;
pub const TERM_FOREGROUND: Option<Color> = None;

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
            write!(buf, "\x1b[C").unwrap();
            skip_count = 0;
        } else if skip_count > 1 {
            write!(buf, "\x1b[{skip_count}C").unwrap();
            skip_count = 0;
        }

        let (fg, bg) = (some_if_ne(cell.fg, last_fg), some_if_ne(cell.bg, last_bg));

        escape_set_color(&mut buf, fg, bg, color_mode);
        buf.extend_from_slice(cell.c.unwrap_or('▄').to_string().as_bytes());

        last_fg = cell.fg;
        last_bg = cell.bg;
    }
    buf
}

async fn print_diff_inner(
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
    buf.extend_from_slice(b"\x1b[m\x1b[H");
    for (i, line) in result.into_iter().enumerate() {
        if i != 0 {
            buf.extend_from_slice(b"\x1b[m\x1b[E");
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

/// 打印帧差异部分
async fn print_diff(force_flush: bool) {
    let (term_width, term_height) = TERM_SIZE.get();
    let (this_frame, last_frame) = &mut *FRAMES.lock().await;

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

    print_diff_inner(force_flush, cells, lasts).await;
}

// @ ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== @

static VIDEO_FRAME: Mutex<Option<VideoFrame>> = Mutex::new(None);
static VIDEO_FRAME_COND: Condvar = Condvar::new();
static VIDEO_FRAME_REQUEST: Condvar = Condvar::new();

pub static VIDEO_SIZE_CACHE: XY = XY::new();

pub fn api_send_frame(frame: VideoFrame) {
    let mut lock = VIDEO_FRAME.lock();
    lock.replace(frame);
    VIDEO_FRAME_COND.notify_one();
}

pub fn api_wait_frame_request_for(duration: Duration) -> bool {
    let mut lock = VIDEO_FRAME.lock();
    let result = VIDEO_FRAME_REQUEST.wait_for(&mut lock, duration);
    result.timed_out() == false
}

pub fn render_main() {
    let mut empty_frame = Vec::new();
    let mut now_frame = None;
    while TERM_QUIT.load(Ordering::SeqCst) == false {
        updatesize(VIDEO_SIZE_CACHE.x(), VIDEO_SIZE_CACHE.y());

        let new_size = VIDEO_PIXELS.x() * VIDEO_PIXELS.y();
        if empty_frame.len() != new_size {
            empty_frame.resize(new_size, Color::new(0, 0, 0));
        }

        if let Some(frame) = VIDEO_FRAME.lock().take() {
            now_frame = Some(frame);
        }

        if let Some(ref frame) = now_frame {
            if (frame.width() as usize, frame.height() as usize) != VIDEO_PIXELS.get() {
                VIDEO_FRAME_REQUEST.notify_one();
                now_frame = None;
            }
        }

        let render_start = Instant::now();

        TOKIO_RUNTIME.block_on(if let Some(ref frame) = now_frame {
            let bytes = frame.data(0);
            let colors: &[Color] = unsafe {
                std::slice::from_raw_parts(
                    bytes.as_ptr() as *const Color,
                    bytes.len() / std::mem::size_of::<Color>(),
                )
            };
            render(colors, frame.stride(0) / std::mem::size_of::<Color>())
        } else {
            #[cfg(feature = "audio")]
            if !avsync::has_video() {
                use crate::audio::{AUDIO_VOLUME_STATISTICS, AUDIO_VOLUME_STATISTICS_LEN};
                let mut stat = AUDIO_VOLUME_STATISTICS.lock();
                while stat.len() < AUDIO_VOLUME_STATISTICS_LEN {
                    stat.push_front(0.0);
                }
                let (w, h) = VIDEO_PIXELS.get();
                for x in 0..w {
                    let max = (h as f32 * 0.8).clamp(0.0, h as f32).round() as usize;
                    empty_frame[(h - max) / 2 * w + x] = Color::new(64, 192, 128);
                    empty_frame[(h + max) / 2 * w + x] = Color::new(64, 192, 128);
                    let i0 = x as f32 / w as f32 * stat.len() as f32;
                    let i1 = (i0.floor() as usize).clamp(0, stat.len() - 1);
                    let i2 = (i0.ceil() as usize).clamp(0, stat.len() - 1);
                    let k = i0 - i0.floor();
                    let vol = stat[i1] * (1.0 - k) + stat[i2] * k;
                    let filled = (vol * h as f32 * 0.8).round().clamp(0.0, h as f32) as usize;
                    for y in (h - filled) / 2..(h + filled) / 2 {
                        let idx = y * w + x;
                        empty_frame[idx] = Color::new(255, 255, 255);
                    }
                }
            }
            render(&empty_frame, VIDEO_PIXELS.x())
        });

        if now_frame.is_none() && !avsync::has_video() {
            empty_frame.fill(Color::new(0, 0, 0));
        }

        let remaining = Duration::from_millis(33).saturating_sub(render_start.elapsed());
        let mut lock = VIDEO_FRAME.lock();
        if lock.is_none() {
            VIDEO_FRAME_COND.wait_for(&mut lock, remaining);
        }
    }
}

// @ ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== @

/// 绿幕背景色
pub static CHROMA_KEY_COLOR: Mutex<Option<Color>> = Mutex::new(None);

pub fn render_video(wrap: &mut RenderWrapper) {
    if let Some(chroma_key) = *CHROMA_KEY_COLOR.lock() {
        for cy in wrap.padding_top..(wrap.cells_height - wrap.padding_bottom) {
            for cx in wrap.padding_left..(wrap.cells_width - wrap.padding_right) {
                let fy = cy - wrap.padding_top;
                let fx = cx - wrap.padding_left;
                let fg = wrap.frame[fy * wrap.frame_pitch * 2 + fx + wrap.frame_pitch];
                let bg = wrap.frame[fy * wrap.frame_pitch * 2 + fx];
                let fs = fg.similar_to(&chroma_key, 0.1);
                let bs = bg.similar_to(&chroma_key, 0.1);
                wrap.cells[cy * wrap.cells_pitch + cx] = match (fs, bs) {
                    (true, true) => Cell {
                        c: Some(' '),
                        fg: Color::transparent(),
                        bg: Color::transparent(),
                    },
                    (true, false) => Cell {
                        c: None,
                        fg: bg,
                        bg,
                    },
                    (false, true) => Cell {
                        c: None,
                        fg,
                        bg: fg,
                    },
                    (false, false) => Cell { c: None, fg, bg },
                };
            }
        }
    } else {
        for cy in wrap.padding_top..(wrap.cells_height - wrap.padding_bottom) {
            for cx in wrap.padding_left..(wrap.cells_width - wrap.padding_right) {
                let fy = cy - wrap.padding_top;
                let fx = cx - wrap.padding_left;
                let fg = wrap.frame[fy * wrap.frame_pitch * 2 + fx + wrap.frame_pitch];
                let bg = wrap.frame[fy * wrap.frame_pitch * 2 + fx];
                wrap.cells[cy * wrap.cells_pitch + cx] = Cell { c: None, fg, bg };
            }
        }
    }
}

// @ ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== @
