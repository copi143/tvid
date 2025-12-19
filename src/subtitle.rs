use data_classes::data;
use parking_lot::Mutex;
use std::{collections::VecDeque, time::Duration};
use unicode_width::UnicodeWidthChar;

use crate::avsync::played_time_or_zero;
use crate::render::ContextWrapper;
use crate::util::{Cell, Color, best_contrast_color};
use std::num::ParseIntError;

#[data]
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
                    dia.end = played_time_or_zero();
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
                    dia.end = played_time_or_zero();
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
                    dia.end = played_time_or_zero();
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
        .filter_map(|dialogue| dialogue.as_mut())
        .filter(|dialogue| dialogue.display_time == Duration::from_millis(0))
        .for_each(|dialogue| dialogue.display_time = time);
    let mut result: Vec<AssDialogue> = subtitles
        .iter()
        .filter_map(|dialogue| dialogue.as_ref())
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

// 解析 ASS override 标签中的颜色，支持类似 "\c&HBBGGRR&" 或 "\1c&HBBGGRR&" 的写法。
// 返回每个字符以及该字符（如果有）应该使用的前景色。
fn parse_ass_color_tags(text: &str) -> Vec<(char, Option<Color>)> {
    let mut out: Vec<(char, Option<Color>)> = Vec::new();
    let mut cur_color: Option<Color> = None;
    let chars: Vec<char> = text.chars().collect();
    let mut i = 0usize;
    while i < chars.len() {
        let c = chars[i];
        if c == '{' {
            // 找到匹配的 '}' 并解析内部标签
            if let Some(j) = (i + 1..chars.len()).find(|&k| chars[k] == '}') {
                let tag: String = chars[i + 1..j].iter().collect();
                if let Some(col) = parse_color_from_tag(&tag) {
                    cur_color = Some(col);
                }
                i = j + 1;
                continue;
            } else {
                // 没有闭合，作为普通字符处理
                out.push((c, cur_color));
                i += 1;
                continue;
            }
        }

        if c == '\\' {
            // 处理常见的换行标记 \N 或 \n
            if i + 1 < chars.len() {
                let nx = chars[i + 1];
                if nx == 'N' || nx == 'n' {
                    out.push(('\n', None));
                    i += 2;
                    continue;
                }
            }
            // 不是换行，保留反斜杠为文字
            out.push(('\\', cur_color));
            i += 1;
            continue;
        }

        out.push((c, cur_color));
        i += 1;
    }

    out
}

fn parse_color_from_tag(tag: &str) -> Option<Color> {
    // 在 tag 中查找 c&H 或 1c&H 等形式（不区分大小写），取后面的十六进制数
    let lower = tag.to_lowercase();
    if let Some(pos) = lower.find("c&h") {
        let rest = &tag[pos + 3..];
        // 从 rest 中提取连续的十六进制字符
        let mut hex = String::new();
        for ch in rest.chars() {
            if ch.is_ascii_hexdigit() {
                hex.push(ch);
            } else {
                break;
            }
        }
        if !hex.is_empty() {
            return hex_to_color(&hex).ok();
        }
    }
    None
}

fn hex_to_color(s: &str) -> Result<Color, ParseIntError> {
    // ASS 颜色通常为 BBRRGG 或 AABBGGRR（十六进制，低位为 R），我们按 BBGGRR 或 AABBGGRR 来处理，取最低 6 个字节作为 B G R
    let mut hex = s.trim();
    if hex.starts_with("&h") || hex.starts_with("&H") {
        hex = &hex[2..];
    } else if hex.starts_with('H') || hex.starts_with('h') {
        hex = &hex[1..];
    }
    // 只取连续的十六进制字符（调用者通常已经截取）
    let hex = hex.trim_start_matches('0');
    // 如果太短则补齐到至少 6
    let hex = if hex.len() < 6 {
        format!("{:0>6}", hex)
    } else {
        hex.to_string()
    };
    // 只取最后 6 位（BBGGRR 或 AABBGGRR -> 取低 6 位表示 BBGGRR）
    let hex_tail = if hex.len() > 6 {
        &hex[hex.len() - 6..]
    } else {
        &hex
    };
    let val = u32::from_str_radix(hex_tail, 16)?;
    let r = (val & 0x0000ff) as u8;
    let g = ((val & 0x00ff00) >> 8) as u8;
    let b = ((val & 0xff0000) >> 16) as u8;
    Ok(Color::new(r, g, b))
}

pub fn render_subtitle(wrap: &mut ContextWrapper) {
    if let Some(played_time) = wrap.played_time {
        let subtitles = get_subtitles(played_time);
        let mut y = wrap.cells_height - 1 - wrap.padding_bottom;
        for sub in subtitles {
            // 解析内联 ASS 颜色标签，得到每个字符以及可选的前景色
            let spans = parse_ass_color_tags(&sub.text);
            let n: usize = spans
                .iter()
                .map(|(ch, _)| ch.width().unwrap_or(1).max(1))
                .sum();
            let mut i = 0;
            let mut x = (wrap.cells_width - n) / 2;
            for (ch, span_color) in spans {
                // 目前不处理 ch == '\n' 的情况

                let k_in = played_time.as_millis() as f32 - sub.display_time.as_millis() as f32;
                let k_in = ((k_in - 50.0 * i as f32) / 200.0).clamp(0.0, 1.0);
                let k_out = if sub.end.as_millis() as f32 == 0.0 {
                    0.0
                } else {
                    played_time.as_millis() as f32 - sub.end.as_millis() as f32
                };
                let k_out = (k_out / 500.0).clamp(0.0, 1.0);
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
                let base_fg = best_contrast_color(bg);
                // 如果 span 提供了颜色，优先使用该颜色再与背景按 k 混合
                let fg = if let Some(col) = span_color {
                    Color::mix(col, bg, k)
                } else {
                    Color::mix(base_fg, bg, k)
                };
                wrap.cells[p] = Cell::new(ch, fg, bg);
                for i in 1..cw {
                    wrap.cells[p + i].c = Some('\0');
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
