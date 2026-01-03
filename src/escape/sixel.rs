use std::io::Write;

use crate::util::Color;

const SIXEL_PADDING_WIDTH: usize = 30;

// fn image_to_256_color(image: &[Color]) -> (Box<[Color; 256]>, Vec<u8>) {
//     let mut count = [0u32; 4096];
//     for color in image {
//         let r = (color.r as usize) >> 4;
//         let g = (color.g as usize) >> 4;
//         let b = (color.b as usize) >> 4;
//         count[(r << 8) | (g << 4) | b] += 1;
//     }

//     let mut num_colors = count.iter().filter(|&&c| c > 0).count();

//     let mut color_map = [0u8; 4096];

//     while num_colors > 256 {
//         let mut min_count = u32::MAX;
//         let mut min_index = 0;
//         for (i, &c) in count.iter().enumerate() {
//             if c > 0 && c < min_count {
//                 min_count = c;
//                 min_index = i;
//             }
//         }
//         count[min_index] = 0;
//         num_colors -= 1;
//     }

//     todo!()
// }

// TODO
// 一行是六像素高
fn format_sixel_line(wr: &mut impl Write, image: [&[Color]; 6]) -> Vec<u8> {
    let mut color_used = [false; 256];
    todo!()
}

// TODO
pub fn format_sixel(
    wr: &mut impl Write,
    data: &[Color],
    width: usize,
    height: usize,
    pitch: usize,
    display_width: usize,
    display_height: usize,
) {
    write!(wr, "\x1bPq").unwrap();
    // TODO
}

// TODO
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
