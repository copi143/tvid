use parking_lot::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::stdin::{self, Key};
use crate::term::{TERM_DEFAULT_BG, TERM_DEFAULT_FG};
use crate::{avsync, ffmpeg, term, ui::helper as uihelper};

static COMMAND_MODE: AtomicBool = AtomicBool::new(false);
static COMMAND_BUFFER: Mutex<String> = Mutex::new(String::new());
static COMMAND_CANDIDATES: Mutex<Vec<String>> = Mutex::new(Vec::new());

type Completer = fn(args: &[&str], prefix: &str) -> Vec<String>;
type Handler = fn(args: &[&str]);

struct CommandSpec {
    name: &'static str,
    handler: Handler,
    completer: Option<Completer>,
}

static COMMANDS: Mutex<Vec<CommandSpec>> = Mutex::new(Vec::new());

fn enter_mode() {
    if !COMMAND_MODE.swap(true, Ordering::SeqCst) {
        clear_buffer();
        refresh_candidates();
    }
}

fn exit_mode() {
    if COMMAND_MODE.swap(false, Ordering::SeqCst) {
        clear_buffer();
        COMMAND_CANDIDATES.lock().clear();
    }
}

fn clear_buffer() {
    COMMAND_BUFFER.lock().clear();
    refresh_candidates();
}

fn push_str(s: &str) {
    COMMAND_BUFFER.lock().push_str(s);
    refresh_candidates();
}

fn pop_char() {
    COMMAND_BUFFER.lock().pop();
    refresh_candidates();
}

fn submit_command() {
    let cmd = COMMAND_BUFFER.lock().trim().to_string();
    exit_mode();
    if !cmd.is_empty() {
        execute_command(&cmd);
    }
}

fn execute_command(line: &str) {
    let mut parts = line.split_whitespace();
    let Some(cmd) = parts.next() else {
        return;
    };
    let args = parts.collect::<Vec<_>>();
    let handler = COMMANDS
        .lock()
        .iter()
        .find(|c| c.name == cmd)
        .map(|c| c.handler);
    if let Some(handler) = handler {
        handler(&args);
    } else {
        error_f16n!("Unknown command: {}", cmd);
    }
}

pub fn register_commands() {
    register_command("help", cmd_help, None);
    register_command("seek", cmd_seek, Some(complete_seek));
    register_command("vol", cmd_volume, Some(complete_volume));
    register_command("volume", cmd_volume, Some(complete_volume));
    register_command("pause", cmd_pause, None);
    register_command("resume", cmd_resume, None);
    register_command("toggle", cmd_toggle, None);
    register_command("next", cmd_next, None);
    register_command("quit", cmd_quit, None);
    register_command("exit", cmd_quit, None);
}

pub fn register_command(name: &'static str, handler: Handler, completer: Option<Completer>) {
    let mut commands = COMMANDS.lock();
    if let Some(cmd) = commands.iter_mut().find(|c| c.name == name) {
        cmd.handler = handler;
        cmd.completer = completer;
    } else {
        commands.push(CommandSpec {
            name,
            handler,
            completer,
        });
    }
}

fn cmd_help(_args: &[&str]) {
    let mut names = COMMANDS.lock().iter().map(|c| c.name).collect::<Vec<_>>();
    names.sort_unstable();
    let list = names.join(" ");
    info_f16n!("Commands: {}", list);
}

fn cmd_seek(args: &[&str]) {
    let Some(arg) = args.first() else {
        error_l10n!("seek: missing argument");
        return;
    };
    let Ok(value) = arg.parse::<f64>() else {
        error_f16n!("seek: invalid argument: {}", arg);
        return;
    };
    if arg.starts_with('+') || arg.starts_with('-') {
        ffmpeg::seek_request_relative(value);
    } else {
        ffmpeg::seek_request_absolute(value);
    }
}

fn cmd_volume(args: &[&str]) {
    let Some(arg) = args.first() else {
        error_l10n!("vol: missing argument");
        return;
    };
    let Ok(value) = arg.parse::<f32>() else {
        error_f16n!("vol: invalid argument: {}", arg);
        return;
    };
    #[cfg(feature = "audio")]
    {
        let target = (value / 100.0).clamp(0.0, 2.0);
        let cur = crate::audio::get_volume();
        crate::audio::adjust_volume(target - cur);
    }
    #[cfg(not(feature = "audio"))]
    {
        let _ = value;
        warning_l10n!("Audio is disabled");
    }
}

fn cmd_pause(_args: &[&str]) {
    avsync::pause();
}

fn cmd_resume(_args: &[&str]) {
    avsync::resume();
}

fn cmd_toggle(_args: &[&str]) {
    avsync::switch_pause_state();
}

fn cmd_next(_args: &[&str]) {
    ffmpeg::notify_quit();
}

fn cmd_quit(_args: &[&str]) {
    term::request_quit();
}

fn complete_seek(_args: &[&str], prefix: &str) -> Vec<String> {
    let suggestions = ["-30", "-5", "+5", "+30", "0", "60", "120"];
    filter_suggestions(prefix, &suggestions)
}

fn complete_volume(_args: &[&str], prefix: &str) -> Vec<String> {
    let suggestions = ["0", "25", "50", "75", "100", "150", "200"];
    filter_suggestions(prefix, &suggestions)
}

fn filter_suggestions(prefix: &str, items: &[&str]) -> Vec<String> {
    items
        .iter()
        .filter(|s| s.starts_with(prefix))
        .map(|s| s.to_string())
        .collect()
}

fn longest_common_prefix(list: &[String]) -> String {
    if list.is_empty() {
        return String::new();
    }
    let mut prefix = list[0].clone();
    for item in list.iter().skip(1) {
        while !item.starts_with(&prefix) {
            if prefix.is_empty() {
                return prefix;
            }
            prefix.pop();
        }
    }
    prefix
}

fn apply_completion(prefix: &str, matches: Vec<String>, replace_last: bool) {
    if matches.is_empty() {
        COMMAND_CANDIDATES.lock().clear();
        return;
    }
    let lcp = longest_common_prefix(&matches);
    let completion = if lcp.len() > prefix.len() {
        lcp
    } else if matches.len() == 1 {
        matches[0].clone()
    } else {
        *COMMAND_CANDIDATES.lock() = matches;
        return;
    };

    COMMAND_CANDIDATES.lock().clear();
    let mut buf = COMMAND_BUFFER.lock();
    if replace_last {
        let input = buf.clone();
        let start = input
            .rfind(|c: char| c.is_whitespace())
            .map(|i| i + 1)
            .unwrap_or(0);
        buf.replace_range(start.., &completion);
    } else {
        buf.push_str(&completion);
    }
    if matches.len() == 1 && !completion.ends_with(' ') {
        buf.push(' ');
    }
    drop(buf);
    refresh_candidates();
}

fn complete_current() {
    let input = COMMAND_BUFFER.lock().clone();
    let ends_with_space = matches!(input.chars().last(), Some(c) if c.is_whitespace());
    let tokens = input.split_whitespace().collect::<Vec<_>>();

    if tokens.is_empty() {
        let names = COMMANDS
            .lock()
            .iter()
            .map(|c| c.name.to_string())
            .collect::<Vec<_>>();
        apply_completion("", names, false);
        return;
    }

    if tokens.len() == 1 && !ends_with_space {
        let prefix = tokens[0];
        let matches = COMMANDS
            .lock()
            .iter()
            .filter(|c| c.name.starts_with(prefix))
            .map(|c| c.name.to_string())
            .collect::<Vec<_>>();
        apply_completion(prefix, matches, true);
        return;
    }

    let cmd = tokens[0];
    let prefix = if ends_with_space {
        ""
    } else {
        *tokens.last().unwrap()
    };
    let args = if ends_with_space {
        &tokens[1..]
    } else if tokens.len() > 1 {
        &tokens[1..tokens.len() - 1]
    } else {
        &[]
    };

    let completer = COMMANDS
        .lock()
        .iter()
        .find(|c| c.name == cmd)
        .and_then(|c| c.completer);
    if let Some(completer) = completer {
        let matches = completer(args, prefix);
        apply_completion(prefix, matches, !ends_with_space);
    }
}

fn refresh_candidates() {
    if !COMMAND_MODE.load(Ordering::SeqCst) {
        COMMAND_CANDIDATES.lock().clear();
        return;
    }
    let input = COMMAND_BUFFER.lock().clone();
    let ends_with_space = matches!(input.chars().last(), Some(c) if c.is_whitespace());
    let tokens = input.split_whitespace().collect::<Vec<_>>();

    let matches = if tokens.is_empty() {
        COMMANDS
            .lock()
            .iter()
            .map(|c| c.name.to_string())
            .collect::<Vec<_>>()
    } else if tokens.len() == 1 && !ends_with_space {
        let prefix = tokens[0];
        COMMANDS
            .lock()
            .iter()
            .filter(|c| c.name.starts_with(prefix))
            .map(|c| c.name.to_string())
            .collect::<Vec<_>>()
    } else {
        let cmd = tokens[0];
        let prefix = if ends_with_space {
            ""
        } else {
            *tokens.last().unwrap()
        };
        let args = if ends_with_space {
            &tokens[1..]
        } else if tokens.len() > 1 {
            &tokens[1..tokens.len() - 1]
        } else {
            &[]
        };
        let completer = COMMANDS
            .lock()
            .iter()
            .find(|c| c.name == cmd)
            .and_then(|c| c.completer);
        completer.map(|c| c(args, prefix)).unwrap_or_default()
    };

    *COMMAND_CANDIDATES.lock() = matches;
}

fn handle_command_key(k: Key) -> bool {
    if !COMMAND_MODE.load(Ordering::SeqCst) {
        return false;
    }
    match k {
        Key::Tab => complete_current(),
        Key::Backspace => pop_char(),
        Key::Escape => exit_mode(),
        Key::Normal('\n') => submit_command(),
        Key::Normal(c) => push_str(&c.to_string()),
        _ => {}
    }
    true
}

pub fn render_command(wrap: &mut crate::render::ContextWrapper) {
    if !COMMAND_MODE.load(Ordering::SeqCst) {
        return;
    }
    let candidates = COMMAND_CANDIDATES.lock().clone();
    let prompt = format!("/{}", COMMAND_BUFFER.lock().as_str());
    let mut text = format!("{prompt}_");
    let max = wrap.cells_width;
    if text.chars().count() > max {
        let skip = text.chars().count() - max;
        text = text.chars().skip(skip).collect();
    }
    let mut y = wrap.cells_height as isize - 1;
    let max_lines = wrap.cells_height.saturating_sub(1).min(6);
    if !candidates.is_empty() && max_lines > 0 {
        let mut lines = candidates;
        lines.truncate(max_lines);
        let line_count = lines.len().min(y as usize);
        let opacity = 0.7;
        let start_y = y - line_count as isize;
        for (i, line) in lines.into_iter().enumerate() {
            let row = start_y + i as isize;
            uihelper::mask(
                wrap,
                0,
                row,
                wrap.cells_width,
                1,
                None,
                TERM_DEFAULT_BG,
                opacity,
            );
            uihelper::textbox(0, row, wrap.cells_width, 1, false);
            uihelper::textbox_default_color(Some(TERM_DEFAULT_FG), Some(TERM_DEFAULT_BG));
            uihelper::put(wrap, &line, None, None);
        }
        y = start_y + line_count as isize;
    }
    uihelper::mask(wrap, 0, y, wrap.cells_width, 1, None, TERM_DEFAULT_BG, 1.0);
    uihelper::textbox(0, y, wrap.cells_width, 1, false);
    uihelper::textbox_default_color(Some(TERM_DEFAULT_FG), Some(TERM_DEFAULT_BG));
    uihelper::put(wrap, &text, None, None);
}

pub fn register_input_callbacks() {
    stdin::register_keypress_callback(Key::Normal('/'), |_, k| {
        if COMMAND_MODE.load(Ordering::SeqCst) {
            return handle_command_key(k);
        }
        enter_mode();
        refresh_candidates();
        true
    });

    stdin::register_keypress_callback(Key::Backspace, |_, k| handle_command_key(k));
    stdin::register_keypress_callback(Key::Escape, |_, k| handle_command_key(k));
    stdin::register_keypress_callback(Key::Normal('\n'), |_, k| handle_command_key(k));
    stdin::register_keypress_callback(Key::Tab, |_, k| handle_command_key(k));

    for c in 32u8..=126u8 {
        let ch = c as char;
        stdin::register_keypress_callback(Key::Normal(ch), |_, k| handle_command_key(k));
    }
    for c in b'a'..=b'z' {
        let ch = c as char;
        stdin::register_keypress_callback(Key::Lower(ch), |_, _| {
            COMMAND_MODE.load(Ordering::SeqCst)
        });
        stdin::register_keypress_callback(Key::Upper(ch), |_, _| {
            COMMAND_MODE.load(Ordering::SeqCst)
        });
        stdin::register_keypress_callback(Key::Ctrl(ch), |_, _| {
            COMMAND_MODE.load(Ordering::SeqCst)
        });
    }
    stdin::register_keypress_callback(Key::Up, |_, _| COMMAND_MODE.load(Ordering::SeqCst));
    stdin::register_keypress_callback(Key::Down, |_, _| COMMAND_MODE.load(Ordering::SeqCst));
    stdin::register_keypress_callback(Key::Left, |_, _| COMMAND_MODE.load(Ordering::SeqCst));
    stdin::register_keypress_callback(Key::Right, |_, _| COMMAND_MODE.load(Ordering::SeqCst));
    stdin::register_keypress_callback(Key::Enter, |_, _| COMMAND_MODE.load(Ordering::SeqCst));

    stdin::register_paste_callback(|_, data| {
        if !COMMAND_MODE.load(Ordering::SeqCst) {
            return false;
        }
        push_str(data);
        true
    });
}

fn format_candidates_lines(width: usize, candidates: &[String], max_lines: usize) -> Vec<String> {
    let mut lines = Vec::new();
    let mut current = String::new();
    for cand in candidates {
        let extra = if current.is_empty() { 0 } else { 1 };
        if current.len() + cand.len() + extra > width {
            lines.push(current);
            if lines.len() >= max_lines {
                return lines;
            }
            current = cand.clone();
        } else {
            if !current.is_empty() {
                current.push(' ');
            }
            current.push_str(cand);
        }
    }
    if !current.is_empty() && lines.len() < max_lines {
        lines.push(current);
    }
    lines
}
