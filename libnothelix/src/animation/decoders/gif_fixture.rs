//! Deterministic 4-frame test GIF fixture, generated programmatically.

use ::image::{codecs::gif::GifEncoder, Delay, Frame, RgbaImage};
use std::time::Duration;

pub fn tiny_gif_bytes() -> Vec<u8> {
    let mut buf = Vec::new();
    {
        let mut enc = GifEncoder::new(&mut buf);
        enc.set_repeat(::image::codecs::gif::Repeat::Infinite)
            .unwrap();
        for k in 0..4u8 {
            let mut img = RgbaImage::new(32, 32);
            for p in img.pixels_mut() {
                *p = ::image::Rgba([k * 60, 0, 255 - k * 60, 255]);
            }
            let frame = Frame::from_parts(
                img,
                0,
                0,
                Delay::from_saturating_duration(Duration::from_millis(100)),
            );
            enc.encode_frame(frame).unwrap();
        }
    }
    buf
}
