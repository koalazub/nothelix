use std::io::Cursor;

use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};

use crate::error::{Error, Result, ffi};
use crate::kitty_placeholder::grid_capacity;

pub const TRANSMIT_LONGEST_SIDE_PX: u32 = 1600;
const MIN_COLS: u32 = 10;
const MIN_ROWS: u32 = 2;
const PX_PER_COL: f64 = 12.0;
const SIDE_MARGIN_COLS: isize = 4;
const DEFAULT_CELL_ASPECT: f64 = 2.0;

pub fn image_dims_b64(b64_data: String) -> String {
    ffi(dims_of_b64(&b64_data).map(|(w, h)| format!("{w}x{h}")))
}

pub fn cached_image_dims(jl_path: String, rel_path: String) -> String {
    ffi(cached_dims(&jl_path, &rel_path).map(|(w, h)| format!("{w}x{h}")))
}

pub fn fit_image_grid(
    img_w: isize,
    img_h: isize,
    viewport_cols: isize,
    max_rows: isize,
    cell_aspect: String,
) -> String {
    let (cols, rows) = fit_grid(
        img_w,
        img_h,
        viewport_cols,
        max_rows,
        parse_aspect(&cell_aspect),
    );
    format!("{cols},{rows}")
}

pub(crate) fn shrink_to_transmit_limit(png: Vec<u8>) -> Vec<u8> {
    match shrink_longest_side(&png, TRANSMIT_LONGEST_SIDE_PX) {
        Some(shrunk) => shrunk,
        None => png,
    }
}

fn cached_dims(jl_path: &str, rel_path: &str) -> Result<(u32, u32)> {
    let parent = std::path::Path::new(jl_path)
        .parent()
        .ok_or_else(|| Error::orphan(jl_path))?;
    let full = parent.join(rel_path);
    let bytes =
        std::fs::read(&full).map_err(|source| Error::reading(full.display().to_string(), source))?;
    dims_of_bytes(&bytes)
}

fn dims_of_b64(b64_data: &str) -> Result<(u32, u32)> {
    let trimmed = b64_data.trim();
    let bytes = BASE64
        .decode(trimmed.as_bytes())
        .map_err(|source| Error::Base64 {
            subject: "image dimensions",
            length: trimmed.len(),
            source,
        })?;
    dims_of_bytes(&bytes)
}

fn dims_of_bytes(bytes: &[u8]) -> Result<(u32, u32)> {
    if is_svg(bytes) {
        return svg_dims(bytes);
    }
    let reader = image::ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .map_err(|io| Error::ImageDecode {
            length: bytes.len(),
            source: image::ImageError::IoError(io),
        })?;
    reader.into_dimensions().map_err(|source| Error::ImageDecode {
        length: bytes.len(),
        source,
    })
}

fn is_svg(bytes: &[u8]) -> bool {
    let head = if bytes.len() > 128 {
        &bytes[..128]
    } else {
        bytes
    };
    let text = String::from_utf8_lossy(head);
    let trimmed = text.trim_start().to_lowercase();
    trimmed.starts_with("<svg") || trimmed.starts_with("<?xml")
}

fn svg_dims(bytes: &[u8]) -> Result<(u32, u32)> {
    let tree = resvg::usvg::Tree::from_data(bytes, &resvg::usvg::Options::default()).map_err(
        |source| Error::Malformed {
            subject: "svg dimensions",
            detail: source.to_string(),
        },
    )?;
    let size = tree.size();
    Ok((size.width().ceil() as u32, size.height().ceil() as u32))
}

fn parse_aspect(raw: &str) -> f64 {
    match raw.trim().parse::<f64>() {
        Ok(parsed) if parsed.is_finite() && parsed > 0.0 => parsed,
        _ => DEFAULT_CELL_ASPECT,
    }
}

fn fit_grid(
    img_w: isize,
    img_h: isize,
    viewport_cols: isize,
    max_rows: isize,
    cell_aspect: f64,
) -> (u32, u32) {
    let capacity = grid_capacity() as u32;
    let avail = (viewport_cols - SIDE_MARGIN_COLS)
        .clamp(8, capacity as isize) as u32;
    let ceiling = (max_rows.max(MIN_ROWS as isize) as u32).min(capacity);
    if img_w <= 0 || img_h <= 0 {
        return (avail.min(40), ceiling.min(12));
    }
    let px_per_row = PX_PER_COL * cell_aspect;
    let natural_cols = ((img_w as f64 / PX_PER_COL).ceil() as u32).max(MIN_COLS);
    let natural_rows = ((img_h as f64 / px_per_row).ceil() as u32).max(MIN_ROWS);
    let width_scale = avail as f64 / natural_cols as f64;
    let height_scale = ceiling as f64 / natural_rows as f64;
    let scale = width_scale.min(height_scale).min(1.0);
    let cols = ((natural_cols as f64 * scale).round() as u32).clamp(MIN_COLS.min(avail), avail);
    let rows = ((natural_rows as f64 * scale).round() as u32).clamp(MIN_ROWS, ceiling);
    (cols.min(capacity), rows.min(capacity))
}

fn shrink_longest_side(bytes: &[u8], max_dim: u32) -> Option<Vec<u8>> {
    if is_svg(bytes) {
        return None;
    }
    let decoded = image::load_from_memory(bytes).ok()?;
    let (w, h) = (decoded.width(), decoded.height());
    if w == 0 || h == 0 || w.max(h) <= max_dim {
        return None;
    }
    let scale = max_dim as f32 / w.max(h) as f32;
    let target_w = ((w as f32 * scale).round() as u32).max(1);
    let target_h = ((h as f32 * scale).round() as u32).max(1);
    let resized = decoded.resize(target_w, target_h, image::imageops::FilterType::Lanczos3);
    let mut png = Vec::new();
    resized
        .write_to(&mut Cursor::new(&mut png), image::ImageFormat::Png)
        .ok()?;
    Some(png)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture_png(width: u32, height: u32) -> String {
        let image = image::DynamicImage::ImageRgba8(image::RgbaImage::from_pixel(
            width,
            height,
            image::Rgba([30, 120, 200, 255]),
        ));
        let mut png = Vec::new();
        image
            .write_to(&mut Cursor::new(&mut png), image::ImageFormat::Png)
            .expect("fixture encodes as png");
        BASE64.encode(png)
    }

    #[test]
    fn dims_report_pixel_size_of_a_png() {
        assert_eq!(dims_of_b64(&fixture_png(24, 48)).expect("dims"), (24, 48));
    }

    #[test]
    fn dims_reject_malformed_base64_with_an_error_reply() {
        let reply = image_dims_b64("!!!not base64!!!".to_string());
        assert!(reply.starts_with("ERROR: "), "{reply}");
    }

    #[test]
    fn fit_never_exceeds_the_viewport_box() {
        let (cols, rows) = fit_grid(3000, 4000, 80, 60, 2.0);
        assert!(cols <= 76, "cols={cols}");
        assert!(rows <= 60, "rows={rows}");
    }

    #[test]
    fn fit_preserves_a_portrait_aspect() {
        let (cols, rows) = fit_grid(3000, 4000, 80, 60, 2.0);
        let visual = rows as f64 / cols as f64 * 2.0;
        let natural = 4000.0 / 3000.0;
        assert!((visual - natural).abs() < 0.15, "cols={cols} rows={rows}");
    }

    #[test]
    fn fit_never_upscales_a_small_image() {
        let (cols, rows) = fit_grid(120, 96, 80, 60, 2.0);
        assert_eq!((cols, rows), (10, 4));
    }

    #[test]
    fn fit_survives_a_viewport_narrower_than_the_minimum_grid() {
        let (cols, rows) = fit_grid(3000, 4000, 10, 60, 2.0);
        assert!(cols <= 8, "cols={cols}");
        assert!(rows >= 2, "rows={rows}");
    }

    #[test]
    fn fit_falls_back_to_a_bounded_grid_for_degenerate_input() {
        let (cols, rows) = fit_grid(0, 0, 80, 60, 2.0);
        assert!(cols <= 76 && rows <= 60, "cols={cols} rows={rows}");
    }

    #[test]
    fn fit_parses_cell_aspect_leniently() {
        assert_eq!(parse_aspect("2.0"), 2.0);
        assert_eq!(parse_aspect("garbage"), DEFAULT_CELL_ASPECT);
        assert_eq!(parse_aspect("-3"), DEFAULT_CELL_ASPECT);
    }

    #[test]
    fn transmit_shrink_caps_a_large_image_at_the_limit() {
        let image = image::DynamicImage::ImageRgba8(image::RgbaImage::from_pixel(
            64,
            32,
            image::Rgba([30, 120, 200, 255]),
        ));
        let mut png = Vec::new();
        image
            .write_to(&mut Cursor::new(&mut png), image::ImageFormat::Png)
            .expect("fixture encodes as png");
        let shrunk = shrink_to_transmit_limit(png);
        let (w, h) = dims_of_bytes(&shrunk).expect("dims of shrunk");
        assert!(
            w.max(h) <= TRANSMIT_LONGEST_SIDE_PX,
            "transmit must cap the longest side: {w}x{h}"
        );
    }

    #[test]
    fn transmit_shrink_keeps_a_small_image_byte_identical() {
        let image = image::DynamicImage::ImageRgba8(image::RgbaImage::from_pixel(
            8,
            8,
            image::Rgba([30, 120, 200, 255]),
        ));
        let mut png = Vec::new();
        image
            .write_to(&mut Cursor::new(&mut png), image::ImageFormat::Png)
            .expect("fixture encodes as png");
        assert_eq!(shrink_to_transmit_limit(png.clone()), png);
    }

    #[test]
    fn transmit_shrink_passes_undecodable_bytes_through() {
        let raw = b"not an image at all".to_vec();
        assert_eq!(shrink_to_transmit_limit(raw.clone()), raw);
    }
}
