//! Graphics protocol detection and Kitty escape sequence generation.

use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};

// ─── Protocol detection ───────────────────────────────────────────────────────

fn terminal_protocol() -> &'static str {
    if std::env::var("KITTY_WINDOW_ID").is_ok() {
        return "kitty";
    }
    if let Ok(term) = std::env::var("TERM") {
        if term.contains("kitty") {
            return "kitty";
        }
    }
    if let Ok(prog) = std::env::var("TERM_PROGRAM") {
        if prog == "iTerm.app" || prog == "WezTerm" {
            return "iterm";
        }
    }
    "block"
}

pub fn viuer_protocol() -> String {
    terminal_protocol().to_string()
}

// ─── Kitty escape generation ──────────────────────────────────────────────────

/// Build a Kitty APC escape sequence from base64-encoded PNG data.
/// Chunks the payload at 4096 bytes to stay within terminal limits.
pub fn kitty_escape_for_b64_png(b64: &str, image_id: u32, rows: u32) -> String {
    let bytes = b64.as_bytes();
    let chunk_size = 4096;
    let chunks: Vec<&[u8]> = bytes.chunks(chunk_size).collect();
    let total = chunks.len();
    let mut out = String::with_capacity(b64.len() + total * 64);

    for (i, chunk) in chunks.iter().enumerate() {
        let s = std::str::from_utf8(chunk).unwrap_or("");
        let more = if i < total - 1 { 1 } else { 0 };

        if i == 0 {
            out.push_str(&format!(
                "\x1b_Ga=T,f=100,t=d,q=2,I={image_id},r={rows},m={more};{s}\x1b\\"
            ));
        } else {
            out.push_str(&format!("\x1b_Gm={more};{s}\x1b\\"));
        }
    }

    out
}

// ─── PNG conversion ───────────────────────────────────────────────────────────

/// Convert raw image bytes to PNG. Returns the input unchanged if already PNG.
pub fn ensure_png(data: &[u8]) -> Vec<u8> {
    if data.starts_with(b"\x89PNG") {
        return data.to_vec();
    }
    use image::ImageReader;
    use std::io::Cursor;
    let Ok(reader) = ImageReader::new(Cursor::new(data)).with_guessed_format() else {
        return data.to_vec();
    };
    let Ok(img) = reader.decode() else {
        return data.to_vec();
    };
    let mut buf = Vec::new();
    let mut cursor = Cursor::new(&mut buf);
    if img.write_to(&mut cursor, image::ImageFormat::Png).is_ok() {
        buf
    } else {
        data.to_vec()
    }
}

// ─── FFI-facing render functions ──────────────────────────────────────────────

pub fn render_image_b64_bytes(b64_data: String, _width: isize, rows: isize) -> String {
    let trimmed = b64_data.trim();
    let data = match BASE64.decode(trimmed) {
        Ok(d) => d,
        Err(e) => return format!("ERROR: base64 decode failed: {e}"),
    };
    if data.starts_with(b"\x89PNG") {
        return kitty_escape_for_b64_png(trimmed, 1, rows.unsigned_abs() as u32);
    }
    let png = ensure_png(&data);
    let b64 = BASE64.encode(&png);
    kitty_escape_for_b64_png(&b64, 1, rows.unsigned_abs() as u32)
}

pub fn kitty_display_image_bytes(b64_data: String, image_id: isize, rows: isize) -> String {
    let trimmed = b64_data.trim();
    let data = match BASE64.decode(trimmed) {
        Ok(d) => d,
        Err(e) => return format!("ERROR: base64 decode: {e}"),
    };
    if data.starts_with(b"\x89PNG") {
        return kitty_escape_for_b64_png(trimmed, image_id as u32, rows as u32);
    }
    let png = ensure_png(&data);
    let b64 = BASE64.encode(&png);
    kitty_escape_for_b64_png(&b64, image_id as u32, rows as u32)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn detect_format_from_magic(data: &[u8]) -> String {
        if data.starts_with(b"\x89PNG") {
            "png"
        } else if data.starts_with(b"\xff\xd8") {
            "jpeg"
        } else if data.starts_with(b"GIF8") {
            "gif"
        } else if data.starts_with(b"RIFF") && data.get(8..12) == Some(b"WEBP") {
            "webp"
        } else if data.starts_with(b"<svg") || data.starts_with(b"<?xml") {
            "svg"
        } else {
            "unknown"
        }
        .to_string()
    }

    fn image_detect_format_bytes(b64_or_bytes: String) -> String {
        let data = BASE64
            .decode(b64_or_bytes.trim())
            .unwrap_or_else(|_| b64_or_bytes.into_bytes());
        detect_format_from_magic(&data)
    }

    #[test]
    fn detect_png() {
        assert_eq!(detect_format_from_magic(b"\x89PNG\r\n\x1a\n rest"), "png");
    }

    #[test]
    fn detect_jpeg() {
        assert_eq!(detect_format_from_magic(b"\xff\xd8\xff\xe0 rest"), "jpeg");
    }

    #[test]
    fn detect_gif() {
        assert_eq!(detect_format_from_magic(b"GIF89a"), "gif");
        assert_eq!(detect_format_from_magic(b"GIF87a"), "gif");
    }

    #[test]
    fn detect_webp() {
        let mut data = b"RIFF____WEBP".to_vec();
        data[4..8].copy_from_slice(b"\x00\x00\x00\x00");
        assert_eq!(detect_format_from_magic(&data), "webp");
    }

    #[test]
    fn detect_svg() {
        assert_eq!(detect_format_from_magic(b"<svg xmlns="), "svg");
        assert_eq!(detect_format_from_magic(b"<?xml version="), "svg");
    }

    #[test]
    fn detect_unknown() {
        assert_eq!(detect_format_from_magic(b"hello world"), "unknown");
        assert_eq!(detect_format_from_magic(b""), "unknown");
    }

    #[test]
    fn kitty_escape_single_chunk() {
        let b64 = "iVBORw0KGgo=";
        let result = kitty_escape_for_b64_png(b64, 1, 10);
        assert!(result.starts_with("\x1b_G"));
        assert!(result.contains("a=T"));
        assert!(result.contains("f=100"));
        assert!(result.contains("m=0"));
        assert!(result.contains("iVBORw0KGgo="));
        assert!(result.ends_with("\x1b\\"));
    }

    #[test]
    fn kitty_escape_multiple_chunks() {
        let b64: String = "A".repeat(5000);
        let result = kitty_escape_for_b64_png(&b64, 42, 15);
        let chunk_count = result.matches("\x1b_G").count();
        assert!(chunk_count >= 2);
        assert!(result.contains("m=1"));
        assert!(result.contains("m=0"));
        assert!(result.contains("I=42"));
        assert!(result.contains("r=15"));
    }

    #[test]
    fn ensure_png_passthrough() {
        let png_header = b"\x89PNG\r\n\x1a\n";
        let result = ensure_png(png_header.as_slice());
        assert_eq!(result, png_header.as_slice());
    }

    #[test]
    fn image_detect_format_bytes_base64_png() {
        let b64 = BASE64.encode(b"\x89PNG\r\n\x1a\n");
        assert_eq!(image_detect_format_bytes(b64), "png");
    }

    #[test]
    fn image_detect_format_bytes_base64_jpeg() {
        let b64 = BASE64.encode(b"\xff\xd8\xff\xe0");
        assert_eq!(image_detect_format_bytes(b64), "jpeg");
    }
}
