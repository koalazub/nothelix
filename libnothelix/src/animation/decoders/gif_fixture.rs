use ::image::{Delay, Frame, RgbaImage, codecs::gif::GifEncoder};
use std::time::Duration;

pub fn tiny_gif_bytes() -> Vec<u8> {
    let mut buf = Vec::new();
    {
        let mut encoder = GifEncoder::new(&mut buf);
        encoder
            .set_repeat(::image::codecs::gif::Repeat::Infinite)
            .unwrap();
        for step in 0..4u8 {
            let mut image = RgbaImage::new(32, 32);
            for pixel in image.pixels_mut() {
                *pixel = ::image::Rgba([step * 60, 0, 255 - step * 60, 255]);
            }
            let frame = Frame::from_parts(
                image,
                0,
                0,
                Delay::from_saturating_duration(Duration::from_millis(100)),
            );
            encoder.encode_frame(frame).unwrap();
        }
    }
    buf
}
