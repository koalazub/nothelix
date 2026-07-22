mod grid;
mod svg;

use abi_stable::std_types::RVec;
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};

use crate::error::{Error, Result, ffi};
use crate::graphics::apc::{self, Placement};
use crate::graphics::png::{ImageEncoding, to_png};
use grid::PlaceholderGrid;

const IMAGE_ID_MASK: u32 = 0x00FF_FFFF;

pub fn kitty_placeholder_max_dim() -> isize {
    grid::capacity() as isize
}

pub fn kitty_placeholder_payload(b64_data: String, image_id: isize) -> String {
    ffi(transmit_base64(&b64_data, addressable_id(image_id)))
}

pub fn kitty_placeholder_payload_bytes(raw_data: RVec<u8>, image_id: isize) -> String {
    ffi(transmit_bytes(&raw_data, addressable_id(image_id)))
}

pub fn kitty_placeholder_rows(_image_id: isize, cols: isize, rows: isize) -> String {
    match PlaceholderGrid::new(cols.max(1) as usize, rows.max(1) as usize) {
        Some(grid) => grid.render(),
        None => String::new(),
    }
}

fn addressable_id(image_id: isize) -> u32 {
    image_id as u32 & IMAGE_ID_MASK
}

fn transmit_base64(b64_data: &str, image_id: u32) -> Result<String> {
    let trimmed = b64_data.trim();
    let data = BASE64.decode(trimmed).map_err(|source| Error::Base64 {
        subject: "kitty placeholder image",
        length: trimmed.len(),
        source,
    })?;
    let b64 = match ImageEncoding::of(&data) {
        ImageEncoding::Png => trimmed.to_string(),
        _ => BASE64.encode(kitty_png(&data)?),
    };
    Ok(transmission(&b64, image_id))
}

fn transmit_bytes(raw_data: &[u8], image_id: u32) -> Result<String> {
    Ok(transmission(&BASE64.encode(kitty_png(raw_data)?), image_id))
}

fn kitty_png(data: &[u8]) -> Result<Vec<u8>> {
    match ImageEncoding::of(data) {
        ImageEncoding::Png => Ok(data.to_vec()),
        ImageEncoding::Svg => svg::rasterize_to_png(data),
        _ => to_png(data),
    }
}

fn transmission(b64: &str, image_id: u32) -> String {
    apc::transmit(b64, Placement::UnicodePlaceholder { image_id })
}

#[cfg(test)]
mod tests {
    use super::*;
    use grid::PLACEHOLDER;

    #[test]
    fn payload_rasterizes_svg_to_png() {
        let svg = br#"<svg xmlns="http://www.w3.org/2000/svg" width="10" height="10" viewBox="0 0 10 10"><rect width="10" height="10" fill="red"/></svg>"#;
        let payload = kitty_placeholder_payload(BASE64.encode(svg), 42);
        assert!(
            payload.contains("f=100"),
            "svg payload must transmit as PNG to Kitty"
        );
        assert!(payload.contains("U=1"));
        assert!(payload.contains("i=42"));
    }

    #[test]
    fn payload_masks_image_id_to_24_bits() {
        let b64 = BASE64.encode(b"\x89PNG\r\n\x1a\nx");
        let payload = kitty_placeholder_payload(b64, 0x0100_002A);
        assert!(payload.contains("i=42"), "id must be masked to low 24 bits");
        assert!(
            !payload.contains("i=16777258"),
            "full out-of-range id must not survive into the transmission"
        );
    }

    #[test]
    fn payload_has_virtual_placement_flag() {
        let b64 = BASE64.encode(b"\x89PNG\r\n\x1a\nfakepngdata");
        let payload = kitty_placeholder_payload(b64, 1001);
        assert!(
            payload.contains("U=1"),
            "payload must set U=1 (virtual placement)"
        );
        assert!(payload.contains("i=1001"), "payload must set image id");
        assert!(
            payload.starts_with("\x1b_G"),
            "payload must be an APC escape"
        );
        assert!(
            payload.ends_with("\x1b\\"),
            "payload must terminate with ST"
        );
    }

    #[test]
    fn payload_chunks_large_base64() {
        let big = "A".repeat(10_000);
        let b64 = BASE64.encode([b"\x89PNG\r\n\x1a\n".as_slice(), big.as_bytes()].concat());
        let payload = kitty_placeholder_payload(b64, 1);
        let chunk_count = payload.matches("\x1b_G").count();
        assert!(
            chunk_count >= 2,
            "expected multi-chunk transmission, got {chunk_count}"
        );
        assert!(payload.contains("m=1"), "non-final chunk should carry m=1");
        assert!(payload.contains("m=0"), "final chunk should carry m=0");
    }

    #[test]
    fn payload_reports_undecodable_base64() {
        let payload = kitty_placeholder_payload("!!!not base64!!!".to_string(), 7);
        assert!(payload.starts_with("ERROR: "), "{payload}");
        assert!(payload.contains("kitty placeholder image"), "{payload}");
    }

    #[test]
    fn rows_produces_one_line_per_row() {
        let s = kitty_placeholder_rows(1001, 4, 3);
        assert_eq!(
            s.split('\n').count(),
            3,
            "should emit exactly one line per row"
        );
    }

    #[test]
    fn rows_contain_no_escape_bytes() {
        let s = kitty_placeholder_rows(1001, 2, 1);
        assert!(
            !s.contains('\x1b'),
            "placeholder rows must not contain escape bytes"
        );
        assert!(!s.contains("[38;2"), "placeholder rows must not embed SGR");
        assert!(
            !s.contains("[39m"),
            "placeholder rows must not embed SGR reset"
        );
    }

    #[test]
    fn rows_first_cell_has_row_and_column_diacritics() {
        let s = kitty_placeholder_rows(1, 3, 2);
        let placeholders = s.chars().filter(|c| *c == PLACEHOLDER).count();
        assert_eq!(placeholders, 6, "expected 6 placeholder codepoints");
    }

    #[test]
    fn rows_byte_count_matches_expected_codepoints() {
        let s = kitty_placeholder_rows(42, 3, 1);
        assert_eq!(s.len(), 16, "unexpected byte count for 3×1 grid");
    }

    #[test]
    fn rows_return_empty_for_oversize_grids() {
        let too_many = kitty_placeholder_max_dim() + 1;
        assert_eq!(kitty_placeholder_rows(1, too_many, 1), "");
        assert_eq!(kitty_placeholder_rows(1, 1, too_many), "");
    }

    #[test]
    fn max_dim_matches_diacritic_table() {
        assert_eq!(kitty_placeholder_max_dim(), grid::capacity() as isize);
    }
}
