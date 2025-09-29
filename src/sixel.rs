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
