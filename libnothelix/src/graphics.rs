//! Terminal graphics: Kitty protocol encoding and image format detection.
//!
//! Supports:
//!   - Automatic terminal protocol detection (kitty / iterm / block)
//!   - PNG conversion via the `image` crate
//!   - Kitty APC escape sequence generation (chunked, ≤4096 bytes per chunk)
//!   - Config override via `~/.config/helix/nothelix.toml`

use std::fs;

use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};

// ─── Protocol detection ───────────────────────────────────────────────────────

fn home_dir() -> std::path::PathBuf {
    std::env::var("HOME")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| std::path::PathBuf::from("/tmp"))
}

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

pub fn detect_graphics_protocol() -> String {
    terminal_protocol().to_string()
}

pub fn config_get_protocol() -> String {
    let config = home_dir().join(".config/helix/nothelix.toml");
    if let Ok(content) = fs::read_to_string(&config) {
        if let Ok(v) = toml::from_str::<toml::Value>(&content) {
            if let Some(proto) = v
                .get("graphics")
                .and_then(|g| g.get("protocol"))
                .and_then(|p| p.as_str())
            {
                if proto != "auto" {
                    return proto.to_string();
                }
            }
        }
    }
    terminal_protocol().to_string()
}

// ─── Kitty escape generation ──────────────────────────────────────────────────

/// Produce Kitty APC escape sequences for a PNG (already base64-encoded).
///
/// The data is split into ≤4096-byte chunks.  The first chunk carries all
/// parameters; subsequent chunks carry only `m=<more>`.
pub fn kitty_escape_for_b64_png(png_b64: &str, image_id: u32, rows: u32) -> String {
    const CHUNK: usize = 4096;
    let bytes = png_b64.as_bytes();
    let chunks: Vec<&[u8]> = bytes.chunks(CHUNK).collect();
    let n = chunks.len();
    let mut out = String::new();

    for (i, chunk) in chunks.iter().enumerate() {
        let more = if i < n - 1 { 1u8 } else { 0u8 };
        // SAFETY: base64 output is always valid ASCII.
        let s = std::str::from_utf8(chunk).unwrap_or("");
        if i == 0 {
            // f=100 → PNG, a=T → transmit, t=d → direct, q=2 → quiet
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

/// Convert raw image bytes to PNG.  If the bytes are already PNG they are
/// returned as-is; otherwise the `image` crate is used for conversion.
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

// ─── Format detection ─────────────────────────────────────────────────────────

pub fn detect_format_from_magic(data: &[u8]) -> String {
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

pub fn image_detect_format(path: String) -> String {
    fs::read(&path)
        .map(|b| detect_format_from_magic(&b))
        .unwrap_or_else(|_| "unknown".to_string())
}

pub fn image_detect_format_bytes(b64_or_bytes: String) -> String {
    let data = BASE64
        .decode(b64_or_bytes.trim())
        .unwrap_or_else(|_| b64_or_bytes.into_bytes());
    detect_format_from_magic(&data)
}

// ─── FFI-facing render functions ──────────────────────────────────────────────

pub fn render_image_bytes(path: String, _width: isize, rows: isize) -> String {
    let data = match fs::read(&path) {
        Ok(d) => d,
        Err(e) => return format!("ERROR: Cannot read {path}: {e}"),
    };
    let png = ensure_png(&data);
    let b64 = BASE64.encode(&png);
    kitty_escape_for_b64_png(&b64, 1, rows.unsigned_abs() as u32)
}

pub fn render_image_b64_bytes(b64_data: String, _width: isize, rows: isize) -> String {
    let data = match BASE64.decode(b64_data.trim()) {
        Ok(d) => d,
        Err(e) => return format!("ERROR: base64 decode failed: {e}"),
    };
    let png = ensure_png(&data);
    let b64 = BASE64.encode(&png);
    kitty_escape_for_b64_png(&b64, 1, rows.unsigned_abs() as u32)
}

pub fn kitty_display_image_bytes(b64_data: String, image_id: isize, rows: isize) -> String {
    let data = match BASE64.decode(b64_data.trim()) {
        Ok(d) => d,
        Err(e) => return format!("ERROR: base64 decode: {e}"),
    };
    let png = ensure_png(&data);
    let b64 = BASE64.encode(&png);
    kitty_escape_for_b64_png(&b64, image_id as u32, rows as u32)
}

/// Placeholder mode: same escape sequence; the caller handles placement.
pub fn kitty_placeholder_image(b64_data: String, image_id: isize, rows: isize) -> String {
    kitty_display_image_bytes(b64_data, image_id, rows)
}

pub fn kitty_display_image(b64_data: String, image_id: isize, rows: isize) -> String {
    kitty_display_image_bytes(b64_data, image_id, rows)
}

/// Write the escape sequence directly to `/dev/tty`.
pub fn write_raw_to_tty(escape_seq: String) -> String {
    use std::io::Write as _;
    match std::fs::OpenOptions::new().write(true).open("/dev/tty") {
        Err(e) => format!("Cannot open /dev/tty: {e}"),
        Ok(mut tty) => match tty.write_all(escape_seq.as_bytes()) {
            Ok(_) => String::new(),
            Err(e) => format!("Write error: {e}"),
        },
    }
}
