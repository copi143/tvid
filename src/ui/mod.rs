use data_classes::{ToNext, ToPrev};
use parking_lot::Mutex;
use std::cmp::min;
use std::fs::FileType;
use std::path::PathBuf;
use std::sync::atomic::Ordering;
use std::time::Duration;

#[cfg(feature = "audio")]
use crate::audio;
#[cfg(feature = "command")]
use crate::command::render_command;
use crate::logging::get_messages;
use crate::playlist::{PLAYLIST, PLAYLIST_SELECTED_INDEX, SHOW_PLAYLIST};
use crate::render::ContextWrapper;
use crate::statistics;
use crate::stdin::{self, Key, MouseAction};
use crate::term::{TERM_DEFAULT_BG, TERM_DEFAULT_FG};
use crate::util::Color;
use crate::{avsync, render};
use crate::{ffmpeg, term};

#[macro_use]
pub mod helper;

const PROGRESSBAR_HEIGHT_PX: f32 = 16.0;

struct FileSelect {
    open: bool,
    path: String,
    list: Vec<(FileType, String)>,
    index: usize,
    shown: f32,
    alpha: f32,
}

impl FileSelect {
    const fn new() -> Self {
        Self {
            open: false,
            path: String::new(),
            list: Vec::new(),
            index: 0,
            shown: 0.0,
            alpha: 0.0,
        }
    }

    fn load_entries(&mut self) {
        self.list.clear();
        if let Ok(entries) = std::fs::read_dir(&self.path) {
            for entry in entries.flatten() {
                if let Ok(file_type) = entry.file_type() {
                    self.list
                        .push((file_type, entry.file_name().to_string_lossy().into_owned()));
                }
            }
        }
    }

    fn ensure_loaded(&mut self) {
        if self.path.is_empty() {
            self.path = "/".to_string();
            self.load_entries();
        }
    }

    fn tick_alpha(&mut self, dt: f32) -> f32 {
        tick_anim(&mut self.alpha, self.open, dt * 2.0, 1.0)
    }

    fn tick_shown(&mut self, dt: f32, max: f32) -> f32 {
        tick_anim(&mut self.shown, self.open, dt * 60.0, max)
    }

    fn move_sel(&mut self, delta: isize) {
        let len = self.list.len();
        if len == 0 {
            self.index = 0;
            return;
        }
        if delta < 0 {
            self.index = self.index.clamp(1, len) - 1;
        } else {
            self.index = (self.index + 1).clamp(0, len - 1);
        }
    }

    fn go_parent(&mut self) {
        let filename = self.path.rsplit('/').next().unwrap_or("").to_string();
        self.path = std::fs::canonicalize(&self.path)
            .unwrap_or_else(|_| PathBuf::from("/"))
            .parent()
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_else(|| "/".to_string());
        self.load_entries();
        self.index = self
            .list
            .iter()
            .position(|(_, name)| *name == filename)
            .unwrap_or(0);
    }

    fn enter_selected_dir(&mut self) {
        if self.list.is_empty() {
            return;
        }
        let (file_type, file_name) = &self.list[self.index];
        if !file_type.is_dir() {
            return;
        }
        let file_name = file_name.clone();
        if self.path.ends_with('/') {
            self.path.push_str(&file_name);
        } else {
            self.path.push('/');
            self.path.push_str(&file_name);
        }
        self.index = 0;
        self.load_entries();
    }

    fn selected_path(&self) -> Option<(bool, String)> {
        if self.list.is_empty() {
            return None;
        }
        let (file_type, file_name) = &self.list[self.index];
        let path = format!("{}/{}", self.path, file_name);
        let mut is_file = file_type.is_file();
        if file_type.is_symlink()
            && let Ok(target_type) = std::fs::metadata(&path).map(|m| m.file_type())
        {
            is_file = target_type.is_file();
        }
        Some((is_file, path))
    }
}

struct Ui {
    first_rendered: bool,
    show_progressbar: bool,
    dragging_progressbar: bool,
    show_help: bool,
    show_overlay: bool,
    playlist_pos: f32,
    file_select: FileSelect,
    quit_confirm: bool,
}

impl Ui {
    const fn new() -> Self {
        Self {
            first_rendered: false,
            show_progressbar: true,
            dragging_progressbar: false,
            show_help: false,
            show_overlay: true,
            playlist_pos: 0.0,
            file_select: FileSelect::new(),
            quit_confirm: false,
        }
    }
}

static UI: Mutex<Ui> = Mutex::new(Ui::new());

fn tick_anim(value: &mut f32, toward: bool, delta: f32, max: f32) -> f32 {
    if toward {
        *value += delta;
    } else {
        *value -= delta;
    }
    *value = value.clamp(0.0, max);
    *value
}

pub fn toggle_file_select() {
    UI.lock().file_select.open ^= true;
}

pub fn request_quit_confirm() {
    UI.lock().quit_confirm = true;
}

pub fn render_ui(wrap: &mut ContextWrapper) {
    UI.lock().first_rendered = true;
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
    #[cfg(feature = "command")]
    render_command(wrap);
}

fn calc_bar_size(cells_width: usize, cells_height: usize, font_height: f32) -> (usize, usize) {
    let bar_w = cells_width as f64 * avsync::playback_progress() + 0.5;
    let bar_h = PROGRESSBAR_HEIGHT_PX / font_height * 2.0;
    let bar_w = (bar_w as usize).clamp(0, cells_width);
    let bar_h = (bar_h as usize).clamp(1, cells_height * 2);
    (bar_w, bar_h)
}

fn render_progressbar(wrap: &mut ContextWrapper) {
    if !UI.lock().show_progressbar {
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
    stdin::register_mouse_callback(|_, m| {
        if !UI.lock().first_rendered {
            return false;
        }
        let ctx = render::RENDER_CONTEXT.lock();

        let term_h = ctx.cells_height;
        let (_, bar_h) = calc_bar_size(ctx.cells_width, ctx.cells_height, ctx.font_height);
        let bar_h = bar_h.div_ceil(2);
        let mut ui = UI.lock();

        if ui.dragging_progressbar {
            if m.left {
                let p = m.pos.0 as f64 / ctx.cells_width as f64;
                ffmpeg::seek_request_absolute(p * avsync::total_duration().as_secs_f64());
            } else {
                ui.dragging_progressbar = false;
            }
            true
        } else if (term_h - bar_h..term_h).contains(&(m.pos.1 as usize)) {
            if m.action != MouseAction::LeftDown {
                return false;
            }
            ui.dragging_progressbar = true;
            let p = m.pos.0 as f64 / ctx.cells_width as f64;
            ffmpeg::seek_request_absolute(p * avsync::total_duration().as_secs_f64());
            true
        } else {
            false
        }
    });
}

fn render_help(wrap: &mut ContextWrapper) {
    if wrap.cells_width < 8 || wrap.cells_height < 8 {
        return; // 防炸
    }

    if !UI.lock().show_help {
        return;
    }

    let w = if helper::font_large_enough(wrap) {
        54
    } else {
        204
    };
    let h = if helper::font_large_enough(wrap) {
        12
    } else {
        42
    };
    let x = (wrap.cells_width as isize - w as isize) / 2;
    let y = (wrap.cells_height as isize - h as isize) / 2;
    helper::mask(
        wrap,
        x,
        y,
        w,
        h,
        Some(TERM_DEFAULT_BG),
        TERM_DEFAULT_FG,
        0.7,
    );
    helper::textbox(x + 2, y + 1, w - 4, h - 2, false);
    helper::textbox_default_color(Some(TERM_DEFAULT_BG), None);
    putlns_or_uflns!(wrap;
        "{}", l10n!("        Help Information (press h to close)       ");
        "{}",       "--------------------------------------------------";
        "{}", l10n!("     q:            Quit the program               ");
        "{}", l10n!("     n:            Next item                      ");
        "{}", l10n!("     l:            Open/close playlist            ");
        "{}", l10n!("     Space/Enter:  Select file                    ");
        "{}", l10n!("     w/s/↑/↓:      Move up/down                   ");
        "{}", l10n!("     a/d/←/→:      Enter/return directory         ");
        "{}", l10n!("     h:            Open/close help                ");
        "{}",       "--------------------------------------------------";
    );
}

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

fn format_bytes_count(count: usize) -> String {
    match count {
        _ if count >= (1 << 30) * 100 => format!("{:5.1} GiB", (count >> 20) as f64 / 1024.0),
        _ if count >= (1 << 30) * 10 => format!("{:5.2} GiB", (count >> 20) as f64 / 1024.0),
        _ if count >= (1 << 20) * 1000 => format!("{:5.3} GiB", (count >> 20) as f64 / 1024.0),
        _ if count >= (1 << 20) * 100 => format!("{:5.1} MiB", (count >> 10) as f64 / 1024.0),
        _ if count >= (1 << 20) * 10 => format!("{:5.2} MiB", (count >> 10) as f64 / 1024.0),
        _ if count >= (1 << 10) * 1000 => format!("{:5.3} KiB", (count >> 10) as f64 / 1024.0),
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

    if !UI.lock().show_overlay {
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
    helper::textbox(2, 1, wrap.cells_width - 4, wrap.cells_height - 2, false);

    let statistics = statistics::get(0);
    let statistics = statistics.lock();

    #[cfg(feature = "audio")]
    let visualizer_status = if render::show_audio_visualizer() {
        "ON"
    } else {
        "OFF"
    };

    let status = if avsync::is_paused() {
        l10n!("Paused")
    } else {
        l10n!("Playing")
    };

    putlns_or_uflns!(wrap;
        "tvid v{}", env!("CARGO_PKG_VERSION");
        "{}", l10n!("Press 'q' to quit, 'n' to skip to next, 'l' for playlist");
        "{}: {}", status, wrap.playing;
        "{}", f16n!("Video Time: {} (a: {}, v: {})", playing_time_str, audio_offset_str, video_offset_str);
        "{}", f16n!("App Time: {}", app_time_str);
        "{}", f16n!("Escape String Encode Time: {:.2?} (avg over last 60)", statistics.escape_string_encode_time.avg());
        "{}", f16n!("Render Time: {:.2?} (avg over last 60)", statistics.render_time.avg());
        "{}", f16n!("Output Time: {:.2?} (avg over last 60)", statistics.output_time.avg());
        "{}", f16n!("Output Bytes: {}", format_bytes_count(statistics.output_bytes.avg::<usize>()));
        "{}", f16n!("Video Skipped Frames: {}", statistics.video_skipped_frames);
        "{}", f16n!("Total Output Bytes: {}", format_bytes_count(statistics.total_output_bytes));
        "{}", f16n!("Color Mode: {}", wrap.color_mode);
        "{}", f16n!("Chroma Mode: {}", wrap.chroma_mode);
        #[cfg(feature = "audio")]
        "{}", f16n!("Volume: {}%", (audio::get_volume() * 100.0).round() as usize);
        #[cfg(feature = "audio")]
        "{}", f16n!("Audio Visualizer(w): {}", visualizer_status);
    );
}

fn render_playlist(wrap: &mut ContextWrapper) {
    if wrap.cells_width < 8 || wrap.cells_height < 8 {
        return; // 防炸
    }

    let playlist_width = if helper::font_large_enough(wrap) {
        62.min(wrap.cells_width)
    } else {
        482.min(wrap.cells_width)
    };

    let playlist_pos = {
        let mut ui = UI.lock();
        let speed = wrap.delta_time.as_secs_f32() * 3000.0 / wrap.font_width;
        tick_anim(
            &mut ui.playlist_pos,
            SHOW_PLAYLIST.load(Ordering::SeqCst),
            speed,
            playlist_width as f32,
        )
    } as usize;
    if playlist_pos == 0 {
        return;
    }

    if wrap.cells_width < 8 || wrap.cells_height < 8 {
        return; // 防炸
    }

    helper::mask(
        wrap,
        wrap.cells_width.saturating_sub(playlist_pos) as isize,
        0,
        playlist_width,
        wrap.cells_height,
        Some(TERM_DEFAULT_BG),
        TERM_DEFAULT_FG,
        0.5,
    );

    helper::textbox(
        wrap.cells_width.saturating_sub(playlist_pos) as isize + 1,
        1,
        playlist_width - 2,
        wrap.cells_height - 2,
        false,
    );

    helper::textbox_default_color(Some(TERM_DEFAULT_BG), None);

    let len = PLAYLIST.lock().len();
    putln_or_ufln!(wrap, "{}", f16n!("Playlist ({} items):", len));

    let selected_index = *PLAYLIST_SELECTED_INDEX.lock();
    let playing_index = PLAYLIST.lock().get_pos();
    for (i, item) in PLAYLIST.lock().get_items().iter().enumerate() {
        // 这边的 U+2000 是故意占位的，因为 ▶ 符号在终端上渲染宽度是 2
        let icon = if i == playing_index { "▶ " } else { "  " };
        if i as isize == selected_index {
            helper::putln_or_ufln(
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

    #[cfg(feature = "unifont")]
    if helper::font_large_enough(wrap) {
        for (i, message) in get_messages().queue.iter().rev().enumerate() {
            let y = wrap.cells_height as isize - i as isize - 1;
            if y < 0 {
                continue;
            }
            helper::mask(wrap, 0, y, width, 1, None, message.lv.level_color(), 0.5);
            helper::textbox(0, y, width, 1, false);
            helper::textbox_default_color(Some(TERM_DEFAULT_BG), None);
            helper::putln(wrap, &message.msg, message.fg, message.bg);
        }
    } else {
        for (i, message) in get_messages().queue.iter().rev().enumerate() {
            let y = wrap.cells_height as isize - i as isize * 4 - 4;
            if y < 0 {
                continue;
            }
            helper::mask(wrap, 0, y, width, 4, None, message.lv.level_color(), 0.5);
            helper::textbox(0, y, width, 4, false);
            helper::textbox_default_color(Some(TERM_DEFAULT_BG), None);
            helper::putufln(wrap, &message.msg, message.fg, message.bg);
        }
    }

    #[cfg(not(feature = "unifont"))]
    for (i, message) in get_messages().queue.iter().rev().enumerate() {
        let y = wrap.cells_height as isize - i as isize - 1;
        if y < 0 {
            continue;
        }
        helper::mask(wrap, 0, y, width, 1, None, message.lv.level_color(), 0.5);
        helper::textbox(0, y, width, 1, false);
        helper::textbox_default_color(Some(TERM_DEFAULT_BG), None);
        helper::putln(wrap, &message.msg, message.fg, message.bg);
    }
}

fn render_file_select(wrap: &mut ContextWrapper) {
    let dt = wrap.delta_time.as_secs_f32();
    let file_select_alpha = UI.lock().file_select.tick_alpha(dt);
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

    helper::mask(
        wrap,
        x,
        y,
        w,
        h,
        Some(TERM_DEFAULT_BG),
        TERM_DEFAULT_FG,
        file_select_alpha * 0.5,
    );

    helper::textbox(x + 1, y + 1, w - 2, h - 2, false);

    helper::textbox_default_color(Some(TERM_DEFAULT_BG), None);

    let mut ui = UI.lock();
    ui.file_select.ensure_loaded();
    let shown_max = min(h - 5, ui.file_select.list.len()) as f32;
    let file_select_shown = ui.file_select.tick_shown(dt, shown_max);

    putlns_or_uflns!(wrap;
        "{}", f16n!("File Select: {}", ui.file_select.path);
        "{}", l10n!("  > Use arrow keys to navigate, Space to select, Q to cancel.");
        "{}", "-".repeat(w - 2);
    );

    let l = h - 2;
    let max_show = (if helper::font_large_enough(wrap) {
        l
    } else {
        l / 4
    }) - 3;
    let mut show_cnt = 0;
    let index = ui.file_select.index;
    let list_len = ui.file_select.list.len();
    for (i, (file_type, file_name)) in ui.file_select.list.iter().enumerate() {
        if i + max_show / 2 < index && i + max_show < list_len {
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
        if i == index {
            helper::putln_or_ufln(wrap, &text, Some(TERM_DEFAULT_FG), Some(TERM_DEFAULT_BG));
        } else {
            helper::putln_or_ufln(wrap, &text, None, None);
        }
    }
}

fn register_file_select_keypress_callbacks() {
    stdin::register_keypress_callback(Key::Normal('q'), |_, _| {
        let mut ui = UI.lock();
        if !ui.file_select.open {
            return false;
        }
        ui.file_select.open = false;
        true
    });

    let cb = |_, _| {
        let selected = {
            let ui = UI.lock();
            if !ui.file_select.open {
                return false;
            }
            ui.file_select.selected_path()
        };
        match selected {
            None => true,
            Some((true, path)) => {
                UI.lock().file_select.open = false;
                PLAYLIST.lock().push_and_setnext(&path);
                ffmpeg::notify_quit();
                true
            }
            Some((false, path)) => {
                error_f16n!("Cannot open non-file: {}", path);
                true
            }
        }
    };
    stdin::register_keypress_callback(Key::Normal(' '), cb);
    stdin::register_keypress_callback(Key::Enter, cb);

    let cb = |_, _| {
        let mut ui = UI.lock();
        if !ui.file_select.open {
            return false;
        }
        ui.file_select.move_sel(-1);
        true
    };
    stdin::register_keypress_callback(Key::Normal('w'), cb);
    stdin::register_keypress_callback(Key::Up, cb);

    let cb = |_, _| {
        let mut ui = UI.lock();
        if !ui.file_select.open {
            return false;
        }
        ui.file_select.move_sel(1);
        true
    };
    stdin::register_keypress_callback(Key::Normal('s'), cb);
    stdin::register_keypress_callback(Key::Down, cb);

    let cb = |_, _| {
        let mut ui = UI.lock();
        if !ui.file_select.open {
            return false;
        }
        ui.file_select.go_parent();
        true
    };
    stdin::register_keypress_callback(Key::Normal('a'), cb);
    stdin::register_keypress_callback(Key::Left, cb);

    let cb = |_, _| {
        let mut ui = UI.lock();
        if !ui.file_select.open {
            return false;
        }
        ui.file_select.enter_selected_dir();
        true
    };
    stdin::register_keypress_callback(Key::Normal('d'), cb);
    stdin::register_keypress_callback(Key::Right, cb);
}

fn render_quit_confirmation(wrap: &mut ContextWrapper) {
    if !UI.lock().quit_confirm {
        return;
    }

    if wrap.cells_width < 8 || wrap.cells_height < 8 {
        return; // 防炸
    }

    let w = if helper::font_large_enough(wrap) {
        25
    } else {
        100
    };
    let h = if helper::font_large_enough(wrap) {
        3
    } else {
        12
    };
    let x = (wrap.cells_width as isize - w as isize) / 2;
    let y = (wrap.cells_height as isize - h as isize) / 2;
    helper::mask(
        wrap,
        x - if helper::font_large_enough(wrap) {
            10
        } else {
            40
        },
        y - if helper::font_large_enough(wrap) {
            2
        } else {
            8
        },
        w + if helper::font_large_enough(wrap) {
            20
        } else {
            80
        },
        h + if helper::font_large_enough(wrap) {
            4
        } else {
            16
        },
        Some(TERM_DEFAULT_BG),
        TERM_DEFAULT_FG,
        0.5,
    );
    helper::textbox(x, y, w, h, false);
    helper::textbox_default_color(Some(TERM_DEFAULT_BG), None);
    putln_or_ufln!(wrap, "{}", l10n!("      Confirm Quit?      "));
    putln_or_ufln!(wrap, "-------------------------");
    putln_or_ufln!(wrap, "        q   /   c        ");
}

pub fn register_input_callbacks() {
    register_input_callbacks_progressbar();

    #[cfg(feature = "audio")]
    stdin::register_mouse_callback(|_, m| match m.action {
        MouseAction::ScrollUp => {
            audio::adjust_volume(0.05);
            true
        }
        MouseAction::ScrollDown => {
            audio::adjust_volume(-0.05);
            true
        }
        _ => false,
    });

    stdin::register_keypress_callback(Key::Normal('h'), |_, _| {
        let mut ui = UI.lock();
        ui.show_help = !ui.show_help;
        true
    });

    stdin::register_keypress_callback(Key::Normal('q'), |id, _| {
        if !UI.lock().quit_confirm {
            return false;
        }
        if id == 0 {
            term::request_quit();
        }
        true
    });

    stdin::register_keypress_callback(Key::Normal('c'), |id, _| {
        if !UI.lock().quit_confirm {
            return false;
        }
        if id == 0 {
            UI.lock().quit_confirm = false;
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
        UI.lock().show_overlay ^= true;
        true
    });

    stdin::register_keypress_callback(Key::Normal('t'), |_, _| {
        debug_l10n!("This is a test debug message.");
        info_l10n!("This is a test message.");
        warning_l10n!("This is a test warning.");
        error_l10n!("This is a test error.");
        true
    });

    register_file_select_keypress_callbacks();
}
