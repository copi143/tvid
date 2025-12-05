use av::util::frame::video::Video as VideoFrame;
use ffmpeg_next as av;
use parking_lot::{Condvar, Mutex};
use std::io::Write as _;
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};
use unicode_width::UnicodeWidthChar;

#[cfg(feature = "osc1337")]
use crate::escape;
use crate::playlist::PLAYLIST;
use crate::stdout::{pend_print, pending_frames, remove_pending_frames};
use crate::term::{self, TERM_QUIT, Winsize};
use crate::{TOKIO_RUNTIME, statistics};
use crate::{avsync, util::*};

// @ ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== @

pub struct RenderContext {
    /// 是否强制下一帧全屏刷新
    pub force_flush_next: bool,

    /// 视频帧宽度（像素）
    pub frame_width: usize,
    /// 视频帧高度（像素）
    pub frame_height: usize,

    /// 终端的单元格状态（当前帧）
    pub cells: Option<Vec<Cell>>,
    /// 终端的单元格状态（上一帧）
    pub lasts: Option<Vec<Cell>>,
    /// 终端的宽度（字符）
    pub cells_width: usize,
    /// 终端的高度（字符）
    pub cells_height: usize,
    /// 终端的行跨度（字符）
    pub cells_pitch: usize,

    /// 终端的宽度（像素）
    pub pixels_width: usize,
    /// 终端的高度（像素）
    pub pixels_height: usize,

    /// 终端视频的左侧填充（字符）
    pub padding_left: usize,
    /// 终端视频的右侧填充（字符）
    pub padding_right: usize,
    /// 终端视频的顶部填充（字符）
    pub padding_top: usize,
    /// 终端视频的底部填充（字符）
    pub padding_bottom: usize,

    /// 视频的原始宽度（像素）
    pub video_origin_width: usize,
    /// 视频的原始高度（像素）
    pub video_origin_height: usize,

    /// 视频显示区域的宽度（字符）
    pub video_cells_width: usize,
    /// 视频显示区域的高度（字符）
    pub video_cells_height: usize,

    /// 终端字体的宽度（像素）
    pub font_width: f32,
    /// 终端字体的高度（像素）
    pub font_height: f32,

    /// 每个字符单元对应的视频帧像素数（x 方向）
    pub fppc_x: usize,
    /// 每个字符单元对应的视频帧像素数（y 方向）
    pub fppc_y: usize,

    /// 颜色模式，见 [`ColorMode`]
    pub color_mode: ColorMode,
    /// 绿幕模式，见 [`ChromaMode`]
    pub chroma_mode: ChromaMode,
}

/// 渲染回调的包装结构
#[allow(unused)]
pub struct ContextWrapper<'frame, 'cells> {
    /// 是否强制下一帧全屏刷新
    pub force_flush_next: bool,

    /// 视频帧的像素数据
    pub frame: &'frame [Color],
    /// 视频帧宽度（像素）
    pub frame_width: usize,
    /// 视频帧高度（像素）
    pub frame_height: usize,
    /// 视频帧跨度（像素）
    pub frame_pitch: usize,

    /// 终端的单元格数据（当前帧）
    pub cells: &'cells mut [Cell],
    /// 终端的单元格数据（上一帧）
    pub lasts: &'cells [Cell],
    /// 终端的宽度（字符）
    pub cells_width: usize,
    /// 终端的高度（字符）
    pub cells_height: usize,
    /// 终端的行跨度（字符）
    pub cells_pitch: usize,

    /// 终端的宽度（像素）
    pub pixels_width: usize,
    /// 终端的高度（像素）
    pub pixels_height: usize,

    /// 终端视频的左侧填充（字符）
    pub padding_left: usize,
    /// 终端视频的右侧填充（字符）
    pub padding_right: usize,
    /// 终端视频的顶部填充（字符）
    pub padding_top: usize,
    /// 终端视频的底部填充（字符）
    pub padding_bottom: usize,

    /// 视频的原始宽度（像素）
    pub video_origin_width: usize,
    /// 视频的原始高度（像素）
    pub video_origin_height: usize,

    /// 视频显示区域的宽度（字符）
    pub video_cells_width: usize,
    /// 视频显示区域的高度（字符）
    pub video_cells_height: usize,

    /// 终端字体的宽度（像素）
    pub font_width: f32,
    /// 终端字体的高度（像素）
    pub font_height: f32,

    /// 每个字符单元对应的视频帧像素数（x 方向）
    pub fppc_x: usize,
    /// 每个字符单元对应的视频帧像素数（y 方向）
    pub fppc_y: usize,

    /// 颜色模式，见 [`ColorMode`]
    pub color_mode: ColorMode,
    /// 绿幕模式，见 [`ChromaMode`]
    pub chroma_mode: ChromaMode,

    /// 正在播放的文件路径
    pub playing: String,
    pub played_time: Option<Duration>,
    pub delta_played_time: Duration,
    pub app_time: Duration,
    pub delta_time: Duration,
}

impl RenderContext {
    pub const fn new() -> Self {
        Self {
            force_flush_next: true,
            frame_width: 0,
            frame_height: 0,
            cells: None,
            lasts: None,
            cells_width: 0,
            cells_height: 0,
            cells_pitch: 0,
            pixels_width: 0,
            pixels_height: 0,
            padding_left: 0,
            padding_right: 0,
            padding_top: 0,
            padding_bottom: 0,
            video_origin_width: 0,
            video_origin_height: 0,
            video_cells_width: 0,
            video_cells_height: 0,
            font_width: 8.0,
            font_height: 16.0,
            fppc_x: 1,
            fppc_y: 2,
            color_mode: ColorMode::new(),
            chroma_mode: ChromaMode::new(),
        }
    }

    pub fn force_flush_next(&mut self) {
        self.force_flush_next = true;
    }

    fn set_padding(&mut self, top: usize, bottom: usize, left: usize, right: usize) {
        self.padding_top = top;
        self.padding_bottom = bottom;
        self.padding_left = left;
        self.padding_right = right;
    }

    /// 更新终端和视频大小
    /// - 计算新的视频显示大小和填充
    /// - 如果大小有变化，重置渲染缓冲区
    pub fn update_size(&mut self, xvideo: Option<usize>, yvideo: Option<usize>) {
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

        if self.cells_width == xchars && self.cells_height == ychars {
            if self.pixels_width == xpixels && self.pixels_height == ypixels {
                if Some((self.video_origin_width, self.video_origin_height)) == xvideo.zip(yvideo) {
                    return;
                }
            }
        }
        self.cells_width = xchars;
        self.cells_height = ychars;
        self.cells_pitch = xchars;
        self.pixels_width = xpixels;
        self.pixels_height = ypixels;
        self.font_width = xpixels as f32 / xchars as f32;
        self.font_height = ypixels as f32 / ychars as f32;
        if let (Some(xvideo), Some(yvideo)) = (xvideo, yvideo) {
            self.video_origin_width = xvideo;
            self.video_origin_height = yvideo;
        }
        let (xvideo, yvideo) = (self.video_origin_width, self.video_origin_height);

        if (xvideo == 0 && yvideo != 0) || (xvideo != 0 && yvideo == 0) {
            panic!("Invalid video size: {xvideo}x{yvideo}");
        }

        let fppc_is_zero = if self.fppc_x == 0 || self.fppc_y == 0 {
            self.fppc_x = 1;
            self.fppc_y = 1;
            true
        } else {
            false
        };

        match xvideo as i64 * ypixels as i64 - yvideo as i64 * xpixels as i64 {
            0 => {
                self.set_padding(0, 0, 0, 0);
                self.video_cells_width = xchars;
                self.video_cells_height = ychars;
            }
            1..=i64::MAX => {
                let y = (ychars * xpixels * yvideo) / (xvideo * ypixels);
                let p = (ychars - y) / 2;
                self.set_padding(p, p, 0, 0);
                self.video_cells_width = xchars;
                self.video_cells_height = ychars - p * 2;
            }
            i64::MIN..=-1 => {
                let x = (xchars * ypixels * xvideo) / (yvideo * xpixels);
                let p = (xchars - x) / 2;
                self.set_padding(0, 0, p, p);
                self.video_cells_width = xchars - p * 2;
                self.video_cells_height = ychars;
            }
        }

        if fppc_is_zero {
            self.frame_width = (self.video_cells_width as f32 * self.font_width) as usize;
            self.frame_height = (self.video_cells_height as f32 * self.font_height) as usize;
            self.fppc_x = 0;
            self.fppc_y = 0;
            if self.frame_width > self.video_origin_width && self.video_origin_width != 0 {
                self.frame_width = self.video_origin_width;
            }
            if self.frame_height > self.video_origin_height && self.video_origin_height != 0 {
                self.frame_height = self.video_origin_height;
            }
        } else {
            self.frame_width = self.video_cells_width * self.fppc_x;
            self.frame_height = self.video_cells_height * self.fppc_y;
        }

        // 将大小加一作为哨兵
        self.cells = Some(vec![Cell::default(); xchars * ychars + 1]);
        self.lasts = Some(vec![Cell::default(); xchars * ychars + 1]);

        remove_pending_frames();

        self.force_flush_next = true;
    }

    pub fn update_fppc(&mut self, fppc_x: usize, fppc_y: usize) {
        self.fppc_x = fppc_x;
        self.fppc_y = fppc_y;
        self.update_size(None, None);
    }

    fn take_cells(&mut self) -> Option<(Vec<Cell>, Vec<Cell>)> {
        match (self.cells.take(), self.lasts.take()) {
            (Some(cells), Some(lasts)) => Some((cells, lasts)),
            (None, None) => None,
            (Some(_), None) => panic!("lasts is None when cells is Some"),
            (None, Some(_)) => panic!("cells is None when lasts is Some"),
        }
    }

    fn put_cells_back(&mut self, cells: Vec<Cell>, lasts: Vec<Cell>) {
        if self.cells.is_none() {
            self.cells.replace(cells);
        }
        if self.lasts.is_none() {
            self.lasts.replace(lasts);
        }
    }

    fn try_wrap<'frame, 'cells>(
        &mut self,
        frame: &'frame [Color],
        frame_width: usize,
        frame_height: usize,
        frame_pitch: usize,
        cells: &'cells mut [Cell],
        lasts: &'cells [Cell],
    ) -> Option<ContextWrapper<'frame, 'cells>> {
        if frame_width != self.frame_width || frame_height != self.frame_height {
            return None;
        }

        let playing = PLAYLIST.lock().current().cloned().unwrap_or_default();

        let (played_time, delta_played_time) = calc_played_time();

        let (app_time, delta_time) = calc_app_time();

        Some(ContextWrapper {
            force_flush_next: self.force_flush_next,
            frame,
            frame_width,
            frame_height,
            frame_pitch,
            cells,
            lasts,
            cells_width: self.cells_width,
            cells_height: self.cells_height,
            cells_pitch: self.cells_pitch,
            pixels_width: self.pixels_width,
            pixels_height: self.pixels_height,
            padding_left: self.padding_left,
            padding_right: self.padding_right,
            padding_top: self.padding_top,
            padding_bottom: self.padding_bottom,
            video_origin_width: self.video_origin_width,
            video_origin_height: self.video_origin_height,
            video_cells_width: self.video_cells_width,
            video_cells_height: self.video_cells_height,
            font_width: self.font_width,
            font_height: self.font_height,
            fppc_x: self.fppc_x,
            fppc_y: self.fppc_y,
            color_mode: self.color_mode,
            chroma_mode: self.chroma_mode,
            playing,
            played_time,
            delta_played_time,
            app_time,
            delta_time,
        })
    }
}

#[allow(unused)]
impl ContextWrapper<'_, '_> {
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

static RENDER_CALLBACKS: Mutex<Vec<fn(&mut ContextWrapper)>> = Mutex::new(Vec::new());

pub fn add_render_callback(callback: fn(&mut ContextWrapper<'_, '_>)) {
    RENDER_CALLBACKS.lock().push(callback);
}

pub static RENDER_CONTEXT: Mutex<RenderContext> = Mutex::new(RenderContext::new());

fn render(frame: &[Color], width: usize, height: usize, pitch: usize) -> bool {
    let mut ctx = RENDER_CONTEXT.lock();

    let Some((mut cells, lasts)) = ctx.take_cells() else {
        return false;
    };

    let Some(mut wrap) = ctx.try_wrap(frame, width, height, pitch, &mut cells, &lasts) else {
        ctx.put_cells_back(cells, lasts);
        return false;
    };

    drop(ctx);

    TOKIO_RUNTIME.block_on(render_frame(&mut wrap));

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
        wrap.force_flush_next = true;
    }
    TOKIO_RUNTIME.block_on(print_diff(&mut wrap));

    drop(wrap);

    let mut ctx = RENDER_CONTEXT.lock();
    ctx.put_cells_back(lasts, cells);

    true
}

async fn render_frame(wrap: &mut ContextWrapper<'_, '_>) {
    let instant = Instant::now();
    wrap.cells.fill(Cell::transparent());
    for callback in RENDER_CALLBACKS.lock().iter() {
        callback(wrap);
    }
    statistics::set_render_time(instant.elapsed());
}

// @ ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== @

#[allow(unused)]
pub const TERM_BACKGROUND: Option<Color> = None;
#[allow(unused)]
pub const TERM_FOREGROUND: Option<Color> = None;

async fn print_diff_line(
    cells: &mut [Cell],
    lasts: &[Cell],
    force_flush: bool,
    color_mode: ColorMode,
) -> Vec<u8> {
    let default_char = match color_mode {
        #[cfg(feature = "osc1337")]
        ColorMode::OSC1337 => ' ',
        ColorMode::TrueColorOnly => '▄',
        ColorMode::Palette256Prefer => '▄',
        ColorMode::Palette256Only => '▄',
        ColorMode::GrayScale => '▄',
        ColorMode::BlackWhite => '▄',
        ColorMode::AsciiArt => '*',
        ColorMode::Braille => '⣿',
    };
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
        if default_char == '⣿' {
            buf.extend_from_slice(cell.c.unwrap_or(cell.braille).to_string().as_bytes());
        } else {
            buf.extend_from_slice(cell.c.unwrap_or(default_char).to_string().as_bytes());
        }

        last_fg = cell.fg;
        last_bg = cell.bg;
    }
    buf
}

async fn print_diff_inner(
    wrap: &mut ContextWrapper<'_, '_>,
    cells: Vec<&'static mut [Cell]>,
    lasts: Vec<&'static [Cell]>,
) {
    let instant = Instant::now();

    let result = (cells.into_iter().zip(lasts.into_iter()))
        .map(|(cell, last)| {
            tokio::spawn(print_diff_line(
                cell,
                last,
                wrap.force_flush_next,
                wrap.color_mode,
            ))
        })
        .collect::<Vec<_>>()
        .join_all()
        .await;

    let mut buf = Vec::with_capacity(65536);

    #[cfg(feature = "osc1337")]
    if wrap.color_mode == ColorMode::OSC1337 {
        write!(
            buf,
            "\x1b[m\x1b[{};{}H",
            wrap.padding_top + 1,
            wrap.padding_left + 1,
        )
        .unwrap();
        escape::format_image(
            &mut buf,
            wrap.frame,
            wrap.frame_width,
            wrap.frame_height,
            wrap.frame_pitch,
            wrap.video_cells_width,
            wrap.video_cells_height,
        );
    }

    buf.extend_from_slice(b"\x1b[m\x1b[H");
    for (i, line) in result.into_iter().enumerate() {
        if i != 0 {
            buf.extend_from_slice(b"\x1b[m\x1b[E");
        }
        buf.extend_from_slice(&line);
    }

    statistics::set_escape_string_encode_time(instant.elapsed());
    if wrap.force_flush_next {
        remove_pending_frames();
        assert!(buf.len() > 0, "force flush but buffer is empty");
        pend_print(buf);
    } else if buf.len() > 0 {
        pend_print(buf);
    }
}

/// 打印帧差异部分
async fn print_diff(wrap: &mut ContextWrapper<'_, '_>) {
    let cells = unsafe {
        wrap.cells
            .split_at_mut(wrap.cells.len() - 1)
            .0
            .chunks_mut(wrap.cells_width)
            .map(|chunk| std::mem::transmute(chunk))
            .collect::<Vec<_>>()
    };
    let lasts = unsafe {
        wrap.lasts
            .split_at(wrap.lasts.len() - 1)
            .0
            .chunks(wrap.cells_width)
            .map(|chunk| std::mem::transmute(chunk))
            .collect::<Vec<_>>()
    };

    assert!(cells.len() == wrap.cells_height, "cells length mismatch");
    assert!(lasts.len() == wrap.cells_height, "lasts length mismatch");

    print_diff_inner(wrap, cells, lasts).await;
}

// @ ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== @

static VIDEO_FRAME: Mutex<Option<Arc<VideoFrame>>> = Mutex::new(None);
static VIDEO_FRAME_COND: Condvar = Condvar::new();
static VIDEO_FRAME_REQUEST: Condvar = Condvar::new();

pub fn api_send_frame(frame: VideoFrame) {
    let mut lock = VIDEO_FRAME.lock();
    lock.replace(Arc::new(frame));
    VIDEO_FRAME_COND.notify_one();
}

pub fn api_wait_frame_request_for(duration: Duration) -> bool {
    let mut lock = VIDEO_FRAME.lock();
    let result = VIDEO_FRAME_REQUEST.wait_for(&mut lock, duration);
    result.timed_out() == false
}

fn update_termsize_and_take_frame(
    empty_frame: &mut Vec<Color>,
) -> (Option<Arc<VideoFrame>>, usize, usize) {
    let mut ctx = RENDER_CONTEXT.lock();
    ctx.update_size(None, None);

    let new_size = ctx.frame_width * ctx.frame_height;
    if empty_frame.len() != new_size {
        empty_frame.resize(new_size, Color::new(0, 0, 0));
    }

    let mut now_frame = VIDEO_FRAME.lock().clone();

    if let Some(ref frame) = now_frame {
        if (frame.width(), frame.height()) != (ctx.frame_width as u32, ctx.frame_height as u32) {
            VIDEO_FRAME_REQUEST.notify_one();
            now_frame = None;
        }
    }

    (now_frame, ctx.frame_width, ctx.frame_height)
}

#[cfg(feature = "audio")]
fn render_audio_visualizer(empty_frame: &mut [Color], w: usize, h: usize) {
    use crate::audio::{AUDIO_VOLUME_STATISTICS, AUDIO_VOLUME_STATISTICS_LEN};
    let mut stat = AUDIO_VOLUME_STATISTICS.lock();
    while stat.len() < AUDIO_VOLUME_STATISTICS_LEN {
        stat.push_front(0.0);
    }
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

pub fn render_main() {
    let mut empty_frame = Vec::new();
    while TERM_QUIT.load(Ordering::SeqCst) == false {
        let (frame, width, height) = update_termsize_and_take_frame(&mut empty_frame);

        let render_start = Instant::now();

        let success = if let Some(ref frame) = frame {
            let bytes = frame.data(0);
            let colors: &[Color] = unsafe {
                std::slice::from_raw_parts(
                    bytes.as_ptr() as *const Color,
                    bytes.len() / std::mem::size_of::<Color>(),
                )
            };
            let width = frame.width() as usize;
            let height = frame.height() as usize;
            let pitch = frame.stride(0) / std::mem::size_of::<Color>();
            render(colors, width, height, pitch)
        } else {
            #[cfg(feature = "audio")]
            if !avsync::has_video() {
                render_audio_visualizer(&mut empty_frame, width, height);
            }
            let success = render(&empty_frame, width, height, width);
            #[cfg(feature = "audio")]
            if frame.is_none() && !avsync::has_video() {
                empty_frame.fill(Color::new(0, 0, 0));
            }
            success
        };

        if !success {
            continue;
        }

        let remaining = Duration::from_millis(33).saturating_sub(render_start.elapsed());
        let mut lock = VIDEO_FRAME.lock();
        let next = lock.clone();
        if next.zip(frame).is_none_or(|(l, n)| Arc::ptr_eq(&l, &n)) {
            VIDEO_FRAME_COND.wait_for(&mut lock, remaining);
        }
    }
}

// @ ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== @

fn render_video_1x1(wrap: &mut ContextWrapper) {
    if wrap.fppc_x != 1 || wrap.fppc_y != 1 {
        panic!("render_video_1x1 only supports fppc_x = 1 and fppc_y = 1");
    }
    if let Some(chroma_key) = wrap.chroma_mode.color() {
        for cy in wrap.padding_top..(wrap.cells_height - wrap.padding_bottom) {
            for cx in wrap.padding_left..(wrap.cells_width - wrap.padding_right) {
                let fy = cy - wrap.padding_top;
                let fx = cx - wrap.padding_left;
                let fg = wrap.frame[fy * wrap.frame_pitch + fx];
                let fs = fg.similar_to(&chroma_key, 0.1);
                wrap.cells[cy * wrap.cells_pitch + cx] = match fs {
                    true => Cell::new(' ', Color::transparent(), Color::transparent()),
                    false => Cell::none(fg, Color::transparent()),
                };
            }
        }
    } else {
        for cy in wrap.padding_top..(wrap.cells_height - wrap.padding_bottom) {
            for cx in wrap.padding_left..(wrap.cells_width - wrap.padding_right) {
                let fy = cy - wrap.padding_top;
                let fx = cx - wrap.padding_left;
                let fg = wrap.frame[fy * wrap.frame_pitch + fx];
                wrap.cells[cy * wrap.cells_pitch + cx] = Cell::none(fg, Color::transparent());
            }
        }
    }
}

fn render_video_1x2(wrap: &mut ContextWrapper) {
    if wrap.fppc_x != 1 || wrap.fppc_y != 2 {
        panic!("render_video_1x2 only supports fppc_x = 1 and fppc_y = 2");
    }
    if let Some(chroma_key) = wrap.chroma_mode.color() {
        for cy in wrap.padding_top..(wrap.cells_height - wrap.padding_bottom) {
            for cx in wrap.padding_left..(wrap.cells_width - wrap.padding_right) {
                let fy = cy - wrap.padding_top;
                let fx = cx - wrap.padding_left;
                let fg = wrap.frame[fy * wrap.frame_pitch * 2 + fx + wrap.frame_pitch];
                let bg = wrap.frame[fy * wrap.frame_pitch * 2 + fx];
                let fs = fg.similar_to(&chroma_key, 0.1);
                let bs = bg.similar_to(&chroma_key, 0.1);
                wrap.cells[cy * wrap.cells_pitch + cx] = match (fs, bs) {
                    (true, true) => Cell::new(' ', Color::transparent(), Color::transparent()),
                    (true, false) => Cell::none(bg, bg),
                    (false, true) => Cell::none(fg, fg),
                    (false, false) => Cell::none(fg, bg),
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
                wrap.cells[cy * wrap.cells_pitch + cx] = Cell::none(fg, bg);
            }
        }
    }
}

fn render_video_2x4(wrap: &mut ContextWrapper) {
    if wrap.fppc_x != 2 || wrap.fppc_y != 4 {
        panic!("render_video_2x4 only supports fppc_x = 2 and fppc_y = 4");
    }
    if let Some(chroma_key) = wrap.chroma_mode.color() {
        for cy in wrap.padding_top..(wrap.cells_height - wrap.padding_bottom) {
            for cx in wrap.padding_left..(wrap.cells_width - wrap.padding_right) {
                let fy = cy - wrap.padding_top;
                let fx = cx - wrap.padding_left;
                let c1 = wrap.frame[fy * wrap.frame_pitch * 4 + fx * 2];
                let c2 = wrap.frame[fy * wrap.frame_pitch * 4 + fx * 2 + 1];
                let c3 = wrap.frame[fy * wrap.frame_pitch * 4 + fx * 2 + wrap.frame_pitch];
                let c4 = wrap.frame[fy * wrap.frame_pitch * 4 + fx * 2 + wrap.frame_pitch + 1];
                let c5 = wrap.frame[fy * wrap.frame_pitch * 4 + fx * 2 + wrap.frame_pitch * 2];
                let c6 = wrap.frame[fy * wrap.frame_pitch * 4 + fx * 2 + wrap.frame_pitch * 2 + 1];
                let c7 = wrap.frame[fy * wrap.frame_pitch * 4 + fx * 2 + wrap.frame_pitch * 3];
                let c8 = wrap.frame[fy * wrap.frame_pitch * 4 + fx * 2 + wrap.frame_pitch * 3 + 1];
                let s1 = c1.similar_to(&chroma_key, 0.1);
                let s2 = c2.similar_to(&chroma_key, 0.1);
                let s3 = c3.similar_to(&chroma_key, 0.1);
                let s4 = c4.similar_to(&chroma_key, 0.1);
                let s5 = c5.similar_to(&chroma_key, 0.1);
                let s6 = c6.similar_to(&chroma_key, 0.1);
                let s7 = c7.similar_to(&chroma_key, 0.1);
                let s8 = c8.similar_to(&chroma_key, 0.1);
                let num = s1 as usize
                    + s2 as usize
                    + s3 as usize
                    + s4 as usize
                    + s5 as usize
                    + s6 as usize
                    + s7 as usize
                    + s8 as usize;
                if num == 8 {
                    wrap.cells[cy * wrap.cells_pitch + cx] =
                        Cell::new(' ', Color::transparent(), Color::transparent());
                    continue;
                }
                let bin = ((!s1 as u32) << 1
                    | (!s2 as u32) << 4
                    | (!s3 as u32) << 2
                    | (!s4 as u32) << 5
                    | (!s5 as u32) << 3
                    | (!s6 as u32) << 6
                    | (!s7 as u32) << 7
                    | (!s8 as u32) << 8)
                    >> 1;
                let c1 = if s1 { ColorF32::zero() } else { c1.as_f32() };
                let c2 = if s2 { ColorF32::zero() } else { c2.as_f32() };
                let c3 = if s3 { ColorF32::zero() } else { c3.as_f32() };
                let c4 = if s4 { ColorF32::zero() } else { c4.as_f32() };
                let c5 = if s5 { ColorF32::zero() } else { c5.as_f32() };
                let c6 = if s6 { ColorF32::zero() } else { c6.as_f32() };
                let c7 = if s7 { ColorF32::zero() } else { c7.as_f32() };
                let c8 = if s8 { ColorF32::zero() } else { c8.as_f32() };
                let color = (c1 + c2 + c3 + c4 + c5 + c6 + c7 + c8) / (8 - num) as f32;
                let color = Color::from(color);
                wrap.cells[cy * wrap.cells_pitch + cx] = Cell::none(color, Color::transparent());
                wrap.cells[cy * wrap.cells_pitch + cx].braille =
                    char::from_u32(0x2800 + bin).unwrap();
            }
        }
    } else {
        for cy in wrap.padding_top..(wrap.cells_height - wrap.padding_bottom) {
            for cx in wrap.padding_left..(wrap.cells_width - wrap.padding_right) {
                let fy = cy - wrap.padding_top;
                let fx = cx - wrap.padding_left;
                let c1 = wrap.frame[fy * wrap.frame_pitch * 4 + fx * 2];
                let c2 = wrap.frame[fy * wrap.frame_pitch * 4 + fx * 2 + 1];
                let c3 = wrap.frame[fy * wrap.frame_pitch * 4 + fx * 2 + wrap.frame_pitch];
                let c4 = wrap.frame[fy * wrap.frame_pitch * 4 + fx * 2 + wrap.frame_pitch + 1];
                let c5 = wrap.frame[fy * wrap.frame_pitch * 4 + fx * 2 + wrap.frame_pitch * 2];
                let c6 = wrap.frame[fy * wrap.frame_pitch * 4 + fx * 2 + wrap.frame_pitch * 2 + 1];
                let c7 = wrap.frame[fy * wrap.frame_pitch * 4 + fx * 2 + wrap.frame_pitch * 3];
                let c8 = wrap.frame[fy * wrap.frame_pitch * 4 + fx * 2 + wrap.frame_pitch * 3 + 1];
                let c1 = c1.as_f32();
                let c2 = c2.as_f32();
                let c3 = c3.as_f32();
                let c4 = c4.as_f32();
                let c5 = c5.as_f32();
                let c6 = c6.as_f32();
                let c7 = c7.as_f32();
                let c8 = c8.as_f32();
                let color = (c1 + c2 + c3 + c4 + c5 + c6 + c7 + c8) / 8.0;
                let color = Color::from(color);
                wrap.cells[cy * wrap.cells_pitch + cx] = Cell::none(color, Color::transparent());
                wrap.cells[cy * wrap.cells_pitch + cx].braille = char::from_u32(0x28ff).unwrap();
            }
        }
    }
}

pub fn render_video(wrap: &mut ContextWrapper) {
    match wrap.color_mode {
        #[cfg(feature = "osc1337")]
        ColorMode::OSC1337 => (),
        ColorMode::TrueColorOnly => render_video_1x2(wrap),
        ColorMode::Palette256Prefer => render_video_1x2(wrap),
        ColorMode::Palette256Only => render_video_1x2(wrap),
        ColorMode::GrayScale => render_video_1x2(wrap),
        ColorMode::BlackWhite => render_video_1x2(wrap),
        ColorMode::AsciiArt => render_video_1x1(wrap),
        ColorMode::Braille => render_video_2x4(wrap),
    }
}

// @ ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== ===== @
