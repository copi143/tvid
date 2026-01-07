use std::io::Write;

use crate::util::{Color, JoinAll, palette256_from_color, palette256_to_color};

// 一行是六像素高
async fn format_sixel_line(image: [&[Color]; 6]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut color_used = [false; 256];
    let width = image[0].len();
    let mut indices = Vec::with_capacity(6);
    for line in image {
        let mut row = Vec::with_capacity(width);
        for &pixel in line {
            if pixel.is_transparent() {
                row.push(255);
            } else {
                let index = palette256_from_color(pixel);
                color_used[index as usize] = true;
                row.push(index);
            }
        }
        indices.push(row);
    }

    let mut used_colors = Vec::new();
    for (index, used) in color_used.iter().enumerate() {
        if *used {
            used_colors.push(index as u8);
        }
    }

    for (i, &color_index) in used_colors.iter().enumerate() {
        write!(out, "#{color_index}").unwrap();

        let mut line_buf = Vec::with_capacity(width);
        for x in 0..width {
            let mut mask = 0u8;
            for (row_index, row) in indices.iter().enumerate() {
                if row[x] == color_index {
                    mask |= 1 << row_index;
                }
            }
            line_buf.push(mask + b'?');
        }

        let mut pos = 0;
        while pos < line_buf.len() {
            let ch = line_buf[pos];
            let mut count = 1;
            while pos + count < line_buf.len() && line_buf[pos + count] == ch {
                count += 1;
            }
            if count > 3 {
                write!(out, "!{count}{}", ch as char).unwrap();
            } else {
                for _ in 0..count {
                    out.write_all(&[ch]).unwrap();
                }
            }
            pos += count;
        }

        out.write_all(b"$").unwrap();
    }

    out
}

pub async fn format_sixel(
    wr: &mut impl Write,
    data: &[Color],
    width: usize,
    height: usize,
    pitch: usize,
    display_width: usize,
    display_height: usize,
) {
    wr.write_all(b"\x1bPq").unwrap();

    let mut flat = Vec::new();
    let data = if pitch == width {
        data
    } else {
        flat.reserve(width * height);
        for y in 0..height {
            flat.extend_from_slice(&data[y * pitch..y * pitch + width]);
        }
        flat.as_slice()
    };

    let mut color_used = [false; 256];
    for &pixel in data {
        if pixel.is_transparent() {
            continue;
        }
        let index = palette256_from_color(pixel);
        color_used[index as usize] = true;
    }

    for (index, _) in color_used.iter().enumerate().filter(|&(_, &used)| used) {
        let color = palette256_to_color(index as u8);
        let r = (color.r as u16 * 100 / 255) as u16;
        let g = (color.g as u16 * 100 / 255) as u16;
        let b = (color.b as u16 * 100 / 255) as u16;
        write!(wr, "#{index};2;{r};{g};{b}").unwrap();
    }

    let pad = vec![Color::transparent(); width];
    let pad = unsafe { std::mem::transmute::<_, &[Color]>(pad.as_slice()) };
    let data = unsafe { std::mem::transmute::<_, &[Color]>(data) };

    let mut tasks = Vec::new();
    let mut y = 0;
    while y < height {
        tasks.push(tokio::spawn(async move {
            let mut rows = [&[][..]; 6];
            for i in 0..6 {
                if y + i < height {
                    rows[i] = &data[(y + i) * width..(y + i + 1) * width];
                } else {
                    rows[i] = pad;
                }
            }
            format_sixel_line(rows).await
        }));
        y += 6;
    }

    for (i, line) in tasks.join_all().await.iter().enumerate() {
        if i != 0 {
            wr.write_all(b"-").unwrap();
        }
        wr.write_all(line).unwrap();
    }

    wr.write_all(b"\x1b\\").unwrap();
}

fn test() {
    // stdout::print(b"\x1bPq");
    // stdout::print(b"#0;2;0;0;0#1;2;100;100;0#2;2;0;100;0");
    // stdout::print(b"#1~~@@vv@@~~@@~~$");
    // stdout::print(b"#2??}}GG}}??}}??-");
    // stdout::print(b"#1!14@-");
    // stdout::print(b"#0;2;0;0;0#1;2;100;100;100#2;2;0;0;100");
    // stdout::print(b"#1~~@@vv@@~~@@~~$");
    // stdout::print(b"#2??}}GG}}??}}??-");
    // stdout::print(b"#1!14@-");
    // stdout::print(b"\x1b\\");

    // stdout::print(b"\x1bPq#0;2;100;100;100#1;2;0;100;0#1~\x1b\\");
}
