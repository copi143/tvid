use unicode_width::UnicodeWidthChar;

use crate::{
    term::{RenderWrapper, TERM_DEFAULT_FG},
    util::{Cell, Color},
};

pub fn puts(
    wrap: &mut RenderWrapper,
    text: &str,
    x: isize,
    y: isize,
    fg: Option<Color>,
    bg: Option<Color>,
) {
    let mut cx = x;
    let cy = y;
    if cy < 0 || cy + 1 > wrap.cells_height as isize {
        return;
    }
    for ch in text.chars() {
        let cw = ch.width().unwrap_or(1).max(1) as isize;
        if cx < 0 || cx + cw > wrap.cells_width as isize {
            cx += cw;
            continue;
        }
        let p = cy as usize * wrap.cells_pitch + cx as usize;
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
}

macro_rules! puts {
    ($wrap:expr, $x:expr, $y:expr, $($arg:tt)*) => {
        crate::ui::puts($wrap, &format!($($arg)*), $x, $y, None, None)
    };
}

pub fn render_ui(wrap: &mut RenderWrapper) {
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

    puts!(wrap, 2, 1, "tvid v{}", env!("CARGO_PKG_VERSION"));
    puts!(wrap, 2, 2, "Press 'q' to quit, 'n' to skip to next");
    puts!(wrap, 2, 3, "Playing: {}", wrap.playing);
    puts!(wrap, 2, 4, "Time: {}", time_str);
}
