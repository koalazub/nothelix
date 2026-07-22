use std::io::Cursor;

use image::{ImageError, ImageFormat, ImageReader};

use crate::error::{Error, Result};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum ImageEncoding {
    Png,
    Jpeg,
    Gif,
    WebP,
    Svg,
    Unrecognised,
}

impl ImageEncoding {
    pub(crate) fn of(data: &[u8]) -> Self {
        if data.starts_with(b"\x89PNG") {
            Self::Png
        } else if data.starts_with(b"\xff\xd8") {
            Self::Jpeg
        } else if data.starts_with(b"GIF8") {
            Self::Gif
        } else if data.starts_with(b"RIFF") && data.get(8..12) == Some(&b"WEBP"[..]) {
            Self::WebP
        } else if data.starts_with(b"<svg") || data.starts_with(b"<?xml") {
            Self::Svg
        } else {
            Self::Unrecognised
        }
    }
}

pub(crate) fn to_png(data: &[u8]) -> Result<Vec<u8>> {
    if ImageEncoding::of(data) == ImageEncoding::Png {
        return Ok(data.to_vec());
    }

    let decode_failure = |source| Error::ImageDecode {
        length: data.len(),
        source,
    };
    let reader = ImageReader::new(Cursor::new(data))
        .with_guessed_format()
        .map_err(|io| decode_failure(ImageError::IoError(io)))?;
    let decoded = reader.decode().map_err(decode_failure)?;

    let mut png = Vec::new();
    decoded
        .write_to(&mut Cursor::new(&mut png), ImageFormat::Png)
        .map_err(|source| Error::ImageEncode {
            format: "png",
            source,
        })?;
    Ok(png)
}
