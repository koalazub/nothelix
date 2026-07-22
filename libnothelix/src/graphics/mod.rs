pub(crate) mod apc;
pub(crate) mod png;
mod protocol;

use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};

use crate::error::{Error, Result, ffi};
use apc::Placement;
use png::{ImageEncoding, to_png};
use protocol::TerminalGraphics;

pub fn viuer_protocol() -> String {
    TerminalGraphics::detect().name().to_string()
}

pub fn render_image_b64_bytes(b64_data: String, _width: isize, rows: isize) -> String {
    ffi(place_at_cursor(&b64_data, 1, rows.unsigned_abs() as u32))
}

pub fn kitty_display_image_bytes(b64_data: String, image_id: isize, rows: isize) -> String {
    ffi(place_at_cursor(&b64_data, image_id as u32, rows as u32))
}

fn place_at_cursor(b64_data: &str, image_id: u32, rows: u32) -> Result<String> {
    let png = png_base64(b64_data)?;
    Ok(apc::transmit(&png, Placement::AtCursor { image_id, rows }))
}

pub(crate) fn png_base64(b64_data: &str) -> Result<String> {
    let trimmed = b64_data.trim();
    let data = BASE64.decode(trimmed).map_err(|source| Error::Base64 {
        subject: "inline image",
        length: trimmed.len(),
        source,
    })?;
    if ImageEncoding::of(&data) == ImageEncoding::Png {
        return Ok(trimmed.to_string());
    }
    Ok(BASE64.encode(to_png(&data)?))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn encoding_of_b64(b64: &str) -> ImageEncoding {
        ImageEncoding::of(&BASE64.decode(b64).expect("valid base64"))
    }

    fn escape(b64: &str, image_id: u32, rows: u32) -> String {
        apc::transmit(b64, Placement::AtCursor { image_id, rows })
    }

    #[test]
    fn detect_png() {
        assert_eq!(
            ImageEncoding::of(b"\x89PNG\r\n\x1a\n rest"),
            ImageEncoding::Png
        );
    }

    #[test]
    fn detect_jpeg() {
        assert_eq!(
            ImageEncoding::of(b"\xff\xd8\xff\xe0 rest"),
            ImageEncoding::Jpeg
        );
    }

    #[test]
    fn detect_gif() {
        assert_eq!(ImageEncoding::of(b"GIF89a"), ImageEncoding::Gif);
        assert_eq!(ImageEncoding::of(b"GIF87a"), ImageEncoding::Gif);
    }

    #[test]
    fn detect_webp() {
        let mut data = b"RIFF____WEBP".to_vec();
        data[4..8].copy_from_slice(b"\x00\x00\x00\x00");
        assert_eq!(ImageEncoding::of(&data), ImageEncoding::WebP);
    }

    #[test]
    fn detect_svg() {
        assert_eq!(ImageEncoding::of(b"<svg xmlns="), ImageEncoding::Svg);
        assert_eq!(ImageEncoding::of(b"<?xml version="), ImageEncoding::Svg);
    }

    #[test]
    fn detect_unknown() {
        assert_eq!(
            ImageEncoding::of(b"hello world"),
            ImageEncoding::Unrecognised
        );
        assert_eq!(ImageEncoding::of(b""), ImageEncoding::Unrecognised);
    }

    #[test]
    fn kitty_escape_single_chunk() {
        let result = escape("iVBORw0KGgo=", 1, 10);
        assert!(result.starts_with("\x1b_G"));
        assert!(result.contains("a=T"));
        assert!(result.contains("f=100"));
        assert!(result.contains("m=0"));
        assert!(result.contains("iVBORw0KGgo="));
        assert!(result.ends_with("\x1b\\"));
    }

    #[test]
    fn kitty_escape_multiple_chunks() {
        let result = escape(&"A".repeat(5000), 42, 15);
        assert!(result.matches("\x1b_G").count() >= 2);
        assert!(result.contains("m=1"));
        assert!(result.contains("m=0"));
        assert!(result.contains("I=42"));
        assert!(result.contains("r=15"));
    }

    #[test]
    fn png_input_passes_through_unconverted() {
        let png_header = b"\x89PNG\r\n\x1a\n";
        assert_eq!(to_png(png_header.as_slice()).expect("png"), png_header);
    }

    #[test]
    fn undecodable_bytes_report_their_length() {
        let error = to_png(b"not an image at all").expect_err("must not swallow");
        assert!(error.to_string().contains("19 bytes"), "{error}");
    }

    #[test]
    fn image_detect_format_bytes_base64_png() {
        let b64 = BASE64.encode(b"\x89PNG\r\n\x1a\n");
        assert_eq!(encoding_of_b64(&b64), ImageEncoding::Png);
    }

    #[test]
    fn image_detect_format_bytes_base64_jpeg() {
        let b64 = BASE64.encode(b"\xff\xd8\xff\xe0");
        assert_eq!(encoding_of_b64(&b64), ImageEncoding::Jpeg);
    }
}
