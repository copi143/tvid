use base64::encoded_len as base64_encoded_len;
use base64::engine::Config;
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use jpeg_encoder::{ColorType, Encoder as JpegEncoder};
use std::io::Write;

use crate::util::Color;

pub fn format_image(
    buf: &mut Vec<u8>,
    data: &[Color],
    width: usize,
    height: usize,
    pitch: usize,
    display_width: usize,
    display_height: usize,
) {
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
    let encoder = JpegEncoder::new(&mut buffer, 90);
    let Ok(_) = encoder.encode(data, width as u16, height as u16, ColorType::Rgba) else {
        return;
    };

    write!(
        buf,
        "\x1b]1337;File=inline=1;width={display_width};height={display_height};size={}:",
        buffer.len(),
    )
    .unwrap();
    BASE64
        .encode_slice(&buffer, {
            let len = buf.len();
            buf.resize(
                len + base64_encoded_len(buffer.len(), BASE64.config().encode_padding()).unwrap(),
                0,
            );
            &mut buf[len..]
        })
        .unwrap();
    buf.extend_from_slice(b"\x1b\\");
}

pub const STEAL_FOCUS: &str = "\x1b]1337;StealFocus\x1b\\";
