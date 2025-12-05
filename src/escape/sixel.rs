use crate::util::Color;

const SIXEL_PADDING_WIDTH: usize = 30;

// TODO
fn image_to_sixel_line(image: &[Color]) -> Vec<u8> {
    let mut result = Vec::new();
    let width = image.len() / 6;
    for x in 0..width {
        let mut byte = 0u8;
        for y in 0..6 {
            let color = image[x + y * width];
            if color.a > 128 {
                byte |= 1 << y;
            }
        }
        result.push(0x3f + byte);
    }
    result
}

// TODO
fn image_to_sixel(image: &[Color]) -> Vec<u8> {
    let mut result = Vec::new();
    let width = image.len() / 6;
    result.extend_from_slice(b"\x1bPq");
    for x in 0..width {
        let mut byte = 0u8;
        for y in 0..6 {
            let color = image[x + y * width];
            if color.a > 128 {
                byte |= 1 << y;
            }
        }
        result.push(0x3f + byte);
    }
    result.extend_from_slice(b"\x1b\\");
    result
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
