use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use jpeg_encoder::{ColorType, Encoder as JpegEncoder};

use crate::util::Color;

pub fn format_image(
    data: &[Color],
    width: usize,
    height: usize,
    pitch: usize,
    term_width: usize,
    term_height: usize,
) -> String {
    let mut vec = Vec::new();
    let data = if pitch == width {
        data
    } else {
        vec.reserve(width * height);
        for y in 0..height {
            vec.extend_from_slice(&data[y * pitch..y * pitch + width]);
        }
        vec.as_slice()
    };

    let data = unsafe {
        std::slice::from_raw_parts(data.as_ptr() as *const u8, std::mem::size_of_val(data))
    };

    let mut buffer = Vec::new();
    let encoder = JpegEncoder::new(&mut buffer, 95);
    let Ok(_) = encoder.encode(data, width as u16, height as u16, ColorType::Rgba) else {
        return String::new();
    };

    format!(
        "\x1b[H\x1b]1337;File=inline=1;width={};height={};size={}:{}\x1b\\",
        term_width,
        term_height,
        buffer.len(),
        BASE64.encode(&buffer)
    )
}
