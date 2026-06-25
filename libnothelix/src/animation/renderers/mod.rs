pub mod kitty_native;
pub mod kitty_replay;
pub mod static_fallback;

/// Encode an RGBA8 buffer to a PNG byte stream. Shared by every renderer
/// (kitty-native, kitty-replay, static-fallback) which previously each
/// carried a byte-identical copy. On encode failure the returned buffer is
/// empty rather than erroring — callers treat an empty payload as "nothing
/// to send".
pub(crate) fn encode_rgba_to_png(rgba: &[u8], w: u16, h: u16) -> Vec<u8> {
    let mut buf = Vec::new();
    encode_rgba_to_png_into(&mut buf, rgba, w, h);
    buf
}

/// Encode an RGBA8 buffer to PNG, appending into an existing buffer. Lets the
/// kitty-native renderer reuse one scratch buffer across frames instead of
/// allocating a fresh `Vec` per frame.
pub(crate) fn encode_rgba_to_png_into(buf: &mut Vec<u8>, rgba: &[u8], w: u16, h: u16) {
    use ::image::codecs::png::PngEncoder;
    use ::image::{ColorType, ImageEncoder};
    PngEncoder::new(buf)
        .write_image(rgba, w as u32, h as u32, ColorType::Rgba8.into())
        .ok(); // leaves `buf` untouched on failure
}
