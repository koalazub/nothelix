use crate::error::{Error, Result};

pub(super) fn encode_rgba(rgba: &[u8], width: u16, height: u16) -> Result<Vec<u8>> {
    let mut buffer = Vec::new();
    encode_rgba_into(&mut buffer, rgba, width, height)?;
    Ok(buffer)
}

pub(super) fn encode_rgba_into(
    buffer: &mut Vec<u8>,
    rgba: &[u8],
    width: u16,
    height: u16,
) -> Result<()> {
    use ::image::codecs::png::PngEncoder;
    use ::image::{ColorType, ImageEncoder};
    PngEncoder::new(buffer)
        .write_image(
            rgba,
            u32::from(width),
            u32::from(height),
            ColorType::Rgba8.into(),
        )
        .map_err(|source| Error::ImageEncode {
            format: "png",
            source,
        })
}
