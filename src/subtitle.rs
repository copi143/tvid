use parking_lot::Mutex;
use std::{collections::VecDeque, time::Duration};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

use crate::{
    audio,
    term::RenderWrapper,
    util::{Cell, Color, best_contrast_color},
};

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct AssDialogue {
    /// 应该开始显示的时间
    pub start: Duration,
    /// 应该结束显示的时间
    pub end: Duration,
    pub id: u32,
    pub layer: i32,
    pub style: String,
    pub name: String,
    pub margin_l: String,
    pub margin_r: String,
    pub margin_v: String,
    pub effect: String,
    pub text: String,
    /// 实际第一次被显示时的时间
    pub display_time: Duration,
}

impl AssDialogue {
    pub fn new(start: Duration, end: Duration, text: &str) -> Self {
        Self {
            start,
            end,
            id: 0,
            layer: 0,
            style: String::new(),
            name: String::new(),
            margin_l: String::new(),
            margin_r: String::new(),
            margin_v: String::new(),
            effect: String::new(),
            text: text.to_string(),
            display_time: Duration::from_millis(0),
        }
    }
}

pub fn parse_duration(s: &str) -> Duration {
    let parts: Vec<&str> = s.split(':').collect();
    if parts.len() != 3 {
        return Duration::from_secs(0);
    }
    let hours: u64 = parts[0].parse().unwrap_or(0);
    let minutes: u64 = parts[1].parse().unwrap_or(0);
    let seconds_parts: Vec<&str> = parts[2].split('.').collect();
    let seconds: u64 = seconds_parts[0].parse().unwrap_or(0);
    let milliseconds: u64 = if seconds_parts.len() > 1 {
        seconds_parts[1].parse().unwrap_or(0)
    } else {
        0
    };
    Duration::from_secs(hours * 3600 + minutes * 60 + seconds) + Duration::from_millis(milliseconds)
}

pub fn parse_ass_line(start: Duration, end: Duration, line: &str) -> Option<AssDialogue> {
    let parts = line
        .trim()
        .splitn(9, ',')
        .map(|s| s.trim())
        .collect::<Vec<_>>();

    if parts.len() < 9 {
        return None;
    }

    Some(AssDialogue {
        start,
        end,
        id: parts[0].parse().unwrap_or(0),
        layer: parts[1].parse().unwrap_or(0),
        style: parts[2].to_string(),
        name: parts[3].to_string(),
        margin_l: parts[4].to_string(),
        margin_r: parts[5].to_string(),
        margin_v: parts[6].to_string(),
        effect: parts[7].to_string(),
        text: parts[8].to_string(),
        display_time: Duration::from_millis(0),
    })
}

static SUBTITLES: Mutex<VecDeque<Option<AssDialogue>>> = Mutex::new(VecDeque::new());

const SUBTITLE_EXTRA_DISPLAY_TIME: Duration = Duration::from_millis(500);

pub fn clear() {
    let mut subtitles = SUBTITLES.lock();
    subtitles.clear();
}

pub fn push_ass(start: Duration, end: Duration, ass: &str) {
    let mut subtitles = SUBTITLES.lock();
    subtitles.iter_mut().for_each(|dialogue| {
        if let Some(dia) = dialogue {
            if dia.end == Duration::from_millis(0) {
                if SUBTITLE_EXTRA_DISPLAY_TIME == Duration::from_millis(0) {
                    *dialogue = None;
                } else {
                    dia.end = audio::played_time();
                }
            }
        }
    });
    if let Some(dialogue) = parse_ass_line(start, end, ass) {
        subtitles.push_back(Some(dialogue));
    }
}

pub fn push_text(start: Duration, end: Duration, text: &str) {
    let mut subtitles = SUBTITLES.lock();
    subtitles.iter_mut().for_each(|dialogue| {
        if let Some(dia) = dialogue {
            if dia.end == Duration::from_millis(0) {
                if SUBTITLE_EXTRA_DISPLAY_TIME == Duration::from_millis(0) {
                    *dialogue = None;
                } else {
                    dia.end = audio::played_time();
                }
            }
        }
    });
    subtitles.push_back(Some(AssDialogue::new(start, end, text)));
}

pub fn push_nothing() {
    let mut subtitles = SUBTITLES.lock();
    subtitles.iter_mut().for_each(|dialogue| {
        if let Some(dia) = dialogue {
            if dia.end == Duration::from_millis(0) {
                if SUBTITLE_EXTRA_DISPLAY_TIME == Duration::from_millis(0) {
                    *dialogue = None;
                } else {
                    dia.end = audio::played_time();
                }
            }
        }
    });
}

pub fn get_subtitles(time: Duration) -> Vec<AssDialogue> {
    let mut subtitles = SUBTITLES.lock();
    while let Some(dialogue) = subtitles.front() {
        if dialogue.is_none() {
            subtitles.pop_front();
        } else if let Some(dialogue) = dialogue
            && dialogue.end != Duration::from_millis(0)
            && dialogue.end + SUBTITLE_EXTRA_DISPLAY_TIME <= time
        {
            subtitles.pop_front();
        } else {
            break;
        }
    }
    subtitles
        .iter_mut()
        .filter(|dialogue| dialogue.is_some())
        .map(|dialogue| dialogue.as_mut().unwrap())
        .filter(|dialogue| dialogue.display_time == Duration::from_millis(0))
        .for_each(|dialogue| dialogue.display_time = time);
    let mut result: Vec<AssDialogue> = subtitles
        .iter()
        .filter(|dialogue| dialogue.is_some())
        .map(|dialogue| dialogue.as_ref().unwrap())
        .filter(|dialogue| {
            dialogue.end == Duration::from_millis(0)
                || (dialogue.start <= time && time <= dialogue.end + SUBTITLE_EXTRA_DISPLAY_TIME)
        })
        .cloned()
        .collect();
    result.sort_by(|a, b| {
        if a.end == Duration::from_millis(0) && b.end == Duration::from_millis(0) {
            std::cmp::Ordering::Equal
        } else if a.end == Duration::from_millis(0) {
            std::cmp::Ordering::Less
        } else if b.end == Duration::from_millis(0) {
            std::cmp::Ordering::Greater
        } else {
            b.end.cmp(&a.end)
        }
    });
    result
}

pub fn render_subtitle(wrap: &mut RenderWrapper) {
    if let Some(played_time) = wrap.played_time {
        let subtitles = get_subtitles(played_time);
        let mut y = wrap.cells_height - 1 - wrap.padding_bottom;
        for sub in subtitles {
            let n = sub.text.width();
            let mut i = 0;
            let mut x = (wrap.cells_width - n) / 2;
            for ch in sub.text.chars() {
                let k_in = played_time.as_millis() as f32 - sub.display_time.as_millis() as f32;
                let k_in = ((k_in - 50.0 * i as f32) / 200.0).min(1.0).max(0.0);
                let k_out = if sub.end.as_millis() as f32 == 0.0 {
                    0.0
                } else {
                    played_time.as_millis() as f32 - sub.end.as_millis() as f32
                };
                let k_out = (k_out / 500.0).min(1.0).max(0.0);
                let k = k_in * (1.0 - k_out);
                let cw = ch.width().unwrap_or(1).max(1);
                if x < wrap.padding_left || x + cw > wrap.cells_width - wrap.padding_right {
                    break;
                }
                if y < wrap.padding_top || y + 1 > wrap.cells_height - wrap.padding_bottom {
                    break;
                }
                let p = (y - (k_out * 5.0) as usize) * wrap.cells_pitch + x;
                let bg = Color::halfhalf(wrap.cells[p].fg, wrap.cells[p].bg);
                let fg = Color::mix(best_contrast_color(bg), bg, k);
                wrap.cells[p] = Cell::new(ch, fg, bg);
                for i in 1..cw {
                    wrap.cells[p + i] = Cell {
                        c: Some('\0'),
                        ..Default::default()
                    };
                }

                i += cw;
                x += cw;
            }

            if y > wrap.padding_top {
                y -= 1;
            } else {
                break;
            }
        }
    }
}
