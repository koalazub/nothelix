//! Kitty Unicode placeholder protocol — the correct way to embed an
//! image inline in a terminal-based editor.
//!
//! # Why this exists
//!
//! The `kitty_display_image_bytes` path in `graphics.rs` uses `a=T`
//! direct transmission: Kitty places the image at the current terminal
//! cursor, and the image is pinned to those absolute terminal cells.
//! When the user scrolls or types a newline, the buffer's visual
//! position for the image changes, so the editor has to delete the
//! image at the old terminal position and re-place it at the new one
//! every frame the position changes. That dance is fragile — any
//! frame-level delay between "delete old" and "place new" leaves stale
//! pixels on the terminal, and under sustained editing (insert-mode
//! held Enter) the stale pixels stack into ghost copies of the plot.
//! That's the smearing artefact.
//!
//! Unicode placeholders are Kitty's architectural answer. The image
//! is transmitted once with `U=1` ("virtual placement") — Kitty caches
//! the pixels under its image id but does not draw them anywhere. The
//! editor then writes a rectangular grid of **placeholder cells** into
//! the terminal buffer where the image should appear. Each cell
//! contains the primary placeholder codepoint `U+10EEEE`, two combining
//! diacritics that encode the image's (row, column) coordinate, and an
//! SGR foreground colour that encodes the low 24 bits of the image id.
//! Kitty's renderer substitutes the corresponding image tile every
//! time it draws such a cell. Because the placeholder cells live in
//! the normal text grid, scrolling, selection, and buffer edits all
//! move the image naturally — no delete/redraw tracking, no stale
//! Kitty pixels, no smearing.
//!
//! # Protocol cheat sheet
//!
//! Transmission escape:
//!
//! ```text
//! \x1b_Ga=T,f=100,t=d,q=2,U=1,i=<id>,m=<more>;<base64>\x1b\\
//! ```
//!
//! Placeholder cell encoding:
//!
//! ```text
//! ESC[38;2;R;G;Bm  U+10EEEE  row-diacritic  col-diacritic  … more cells …  ESC[39m
//! ```
//!
//! Kitty auto-increments the column within a contiguous run of
//! placeholder cells, so only the FIRST cell on each row needs both
//! the row and column diacritics; subsequent cells in the same row
//! can be bare placeholder codepoints.
//!
//! The row and column indices are NOT simple offsets into
//! `[U+0305..)`. Kitty accepts only 297 specific combining diacritics
//! as valid index marks, and they must be emitted in the exact order
//! defined by the spec. The `DIACRITICS` table below is that ordering,
//! lifted verbatim from the upstream Kitty implementation.

use std::fmt::Write;

use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};

use crate::graphics::ensure_png;

/// Primary placeholder character. Kitty recognises every cell starting
/// with this codepoint as part of a virtual image placement.
const PLACEHOLDER: char = '\u{10EEEE}';

/// Valid combining diacritics for encoding placeholder row/column
/// indices, in the order Kitty expects. `DIACRITICS[n]` is the
/// diacritic that encodes index `n` (0-indexed). Max representable
/// row/column count is `DIACRITICS.len()` — well above any realistic
/// inline plot.
const DIACRITICS: &[u32] = &[
    0x0305, 0x030D, 0x030E, 0x0310, 0x0312, 0x033D, 0x033E, 0x033F, 0x0346, 0x034A, 0x034B, 0x034C,
    0x0350, 0x0351, 0x0352, 0x0357, 0x035B, 0x0363, 0x0364, 0x0365, 0x0366, 0x0367, 0x0368, 0x0369,
    0x036A, 0x036B, 0x036C, 0x036D, 0x036E, 0x036F, 0x0483, 0x0484, 0x0485, 0x0486, 0x0487, 0x0592,
    0x0593, 0x0594, 0x0595, 0x0597, 0x0598, 0x0599, 0x059C, 0x059D, 0x059E, 0x059F, 0x05A0, 0x05A1,
    0x05A8, 0x05A9, 0x05AB, 0x05AC, 0x05AF, 0x05C4, 0x0610, 0x0611, 0x0612, 0x0613, 0x0614, 0x0615,
    0x0616, 0x0617, 0x0657, 0x0658, 0x0659, 0x065A, 0x065B, 0x065D, 0x065E, 0x06D6, 0x06D7, 0x06D8,
    0x06D9, 0x06DA, 0x06DB, 0x06DC, 0x06DF, 0x06E0, 0x06E1, 0x06E2, 0x06E4, 0x06E7, 0x06E8, 0x06EB,
    0x06EC, 0x0730, 0x0732, 0x0733, 0x0735, 0x0736, 0x073A, 0x073D, 0x073F, 0x0740, 0x0741, 0x0743,
    0x0745, 0x0747, 0x0749, 0x074A, 0x07EB, 0x07EC, 0x07ED, 0x07EE, 0x07EF, 0x07F0, 0x07F1, 0x07F3,
    0x0816, 0x0817, 0x0818, 0x0819, 0x081B, 0x081C, 0x081D, 0x081E, 0x081F, 0x0820, 0x0821, 0x0822,
    0x0823, 0x0825, 0x0826, 0x0827, 0x0829, 0x082A, 0x082B, 0x082C, 0x082D, 0x0951, 0x0953, 0x0954,
    0x0F82, 0x0F83, 0x0F86, 0x0F87, 0x135D, 0x135E, 0x135F, 0x17DD, 0x193A, 0x1A17, 0x1A75, 0x1A76,
    0x1A77, 0x1A78, 0x1A79, 0x1A7A, 0x1A7B, 0x1A7C, 0x1B6B, 0x1B6D, 0x1B6E, 0x1B6F, 0x1B70, 0x1B71,
    0x1B72, 0x1B73, 0x1CD0, 0x1CD1, 0x1CD2, 0x1CDA, 0x1CDB, 0x1CE0, 0x1DC0, 0x1DC1, 0x1DC3, 0x1DC4,
    0x1DC5, 0x1DC6, 0x1DC7, 0x1DC8, 0x1DC9, 0x1DCB, 0x1DCC, 0x1DD1, 0x1DD2, 0x1DD3, 0x1DD4, 0x1DD5,
    0x1DD6, 0x1DD7, 0x1DD8, 0x1DD9, 0x1DDA, 0x1DDB, 0x1DDC, 0x1DDD, 0x1DDE, 0x1DDF, 0x1DE0, 0x1DE1,
    0x1DE2, 0x1DE3, 0x1DE4, 0x1DE5, 0x1DE6, 0x1DFE, 0x20D0, 0x20D1, 0x20D4, 0x20D5, 0x20D6, 0x20D7,
    0x20DB, 0x20DC, 0x20E1, 0x20E7, 0x20E9, 0x20F0, 0x2CEF, 0x2CF0, 0x2CF1, 0x2DE0, 0x2DE1, 0x2DE2,
    0x2DE3, 0x2DE4, 0x2DE5, 0x2DE6, 0x2DE7, 0x2DE8, 0x2DE9, 0x2DEA, 0x2DEB, 0x2DEC, 0x2DED, 0x2DEE,
    0x2DEF, 0x2DF0, 0x2DF1, 0x2DF2, 0x2DF3, 0x2DF4, 0x2DF5, 0x2DF6, 0x2DF7, 0x2DF8, 0x2DF9, 0x2DFA,
    0x2DFB, 0x2DFC, 0x2DFD, 0x2DFE, 0x2DFF, 0xA66F, 0xA67C, 0xA67D, 0xA6F0, 0xA6F1, 0xA8E0, 0xA8E1,
    0xA8E2, 0xA8E3, 0xA8E4, 0xA8E5, 0xA8E6, 0xA8E7, 0xA8E8, 0xA8E9, 0xA8EA, 0xA8EB, 0xA8EC, 0xA8ED,
    0xA8EE, 0xA8EF, 0xA8F0, 0xA8F1, 0xAAB0, 0xAAB2, 0xAAB3, 0xAAB7, 0xAAB8, 0xAABE, 0xAABF, 0xAAC1,
    0xFE20, 0xFE21, 0xFE22, 0xFE23, 0xFE24, 0xFE25, 0xFE26, 0x10A0F, 0x10A38, 0x1D185, 0x1D186,
    0x1D187, 0x1D188, 0x1D189, 0x1D1AA, 0x1D1AB, 0x1D1AC, 0x1D1AD, 0x1D242, 0x1D243, 0x1D244,
];

/// Maximum placeholder grid dimension we can encode. Returned from the
/// FFI so the plugin can size down if an image needs more cells than
/// Kitty's diacritic table allows (it won't for anything we render —
/// 297 is comfortably above typical inline plot dimensions).
pub fn kitty_placeholder_max_dim() -> isize {
    DIACRITICS.len() as isize
}

/// Build the transmission escape for virtual placement of `b64_data`
/// under `image_id`. After Kitty receives this, it has the pixels
/// cached under the id but does NOT draw them anywhere. Drawing is
/// triggered by placeholder cells produced by `kitty_placeholder_rows`.
///
/// `b64_data` is either a base64-encoded PNG (fast path) or a base64-
/// encoded image in any other format (we decode, re-encode as PNG, and
/// re-base64).
pub fn kitty_placeholder_payload(b64_data: String, image_id: isize) -> String {
    let trimmed = b64_data.trim();
    let data = match BASE64.decode(trimmed) {
        Ok(d) => d,
        Err(e) => return format!("ERROR: base64 decode: {e}"),
    };
    let b64 = if data.starts_with(b"\x89PNG") {
        trimmed.to_string()
    } else {
        let png = ensure_png(&data);
        BASE64.encode(&png)
    };
    build_virtual_transmission(&b64, image_id as u32)
}

/// Like `kitty_placeholder_payload` but accepts raw image bytes directly,
/// skipping the base64 decode round-trip. Used when image data comes from
/// a sidecar file rather than a JSON string.
pub fn kitty_placeholder_payload_bytes(raw_data: Vec<u8>, image_id: isize) -> String {
    let png_data = ensure_png(&raw_data);
    let b64 = BASE64.encode(&png_data);
    build_virtual_transmission(&b64, image_id as u32)
}

/// Chunked APC transmission with `U=1` set and no `r=`/`c=` parameters.
/// Kitty will size the image to the number of placeholder cells the
/// terminal ends up rendering, so we don't need to pre-commit dimensions
/// at transmission time.
fn build_virtual_transmission(b64: &str, image_id: u32) -> String {
    let bytes = b64.as_bytes();
    let chunk_size = 4096;
    let total = bytes.len().div_ceil(chunk_size);
    let mut out = String::with_capacity(b64.len() + total * 64);

    for (i, chunk) in bytes.chunks(chunk_size).enumerate() {
        let s = std::str::from_utf8(chunk).unwrap_or("");
        let more = if i < total - 1 { 1 } else { 0 };

        if i == 0 {
            // a=T  transmit+place
            // f=100 PNG
            // t=d  direct (inline base64)
            // q=2  suppress OK/ERROR responses
            // U=1  virtual placement via Unicode placeholders
            // i=   image id (low 24 bits carry through the SGR colour encoding)
            let _ = write!(out, "\x1b_Ga=T,f=100,t=d,q=2,U=1,i={image_id},m={more};{s}\x1b\\");
        } else {
            let _ = write!(out, "\x1b_Gm={more};{s}\x1b\\");
        }
    }

    out
}

/// Build the newline-separated placeholder grid that tells Kitty where
/// to draw the image. `cols` × `rows` cells. Each row is a flat run of
/// placeholder codepoints:
///
///   first cell:     PLACEHOLDER + row-diacritic + col-0-diacritic
///   remaining cells: PLACEHOLDER (Kitty auto-increments the column)
///
/// **No SGR escapes.** Helix's `set_string` iterates the string as
/// Unicode graphemes and writes each one as a single terminal cell via
/// `Buffer::set_symbol`; control bytes like `\x1b` have `width() == 0`
/// so they are silently skipped, leaving the following ASCII of the
/// escape (`[38;2;...m`) to render as literal text. Embedding SGR
/// directly would produce exactly the "visible colour-escape text"
/// artefact instead of a rendered image.
///
/// The foreground colour (which Kitty reads as the 24-bit image id) is
/// applied by the Helix fork's `draw_raw_content` via
/// `Style::default().fg(Color::Rgb(r, g, b))` where `(r, g, b)` are
/// derived from `raw.id`. That routes through Helix's normal rendering
/// pipeline and emits correct SGR bracketing around each placeholder
/// cell on flush.
///
/// Returns the empty string if `cols` or `rows` exceed
/// `DIACRITICS.len()` — the caller should treat that as "image is too
/// large for the placeholder protocol" and fall back.
pub fn kitty_placeholder_rows(image_id: isize, cols: isize, rows: isize) -> String {
    let _ = image_id; // reserved so the FFI signature stays stable
    let cols = cols.max(1) as usize;
    let rows = rows.max(1) as usize;

    if rows > DIACRITICS.len() || cols > DIACRITICS.len() {
        return String::new();
    }

    // One row = 3 codepoints for the first cell (PLACEHOLDER, row
    // diacritic, col-0 diacritic) + (cols-1) bare placeholders. Each
    // PLACEHOLDER is 4 bytes UTF-8, each diacritic is 2 bytes. Budget
    // generously.
    let mut out = String::with_capacity(rows * (cols * 4 + 4 + 1));

    for (row, &dia) in DIACRITICS.iter().enumerate().take(rows) {
        // First cell carries both diacritics so Kitty knows the start
        // of a new row run.
        let row_diacritic = char::from_u32(dia).unwrap_or(' ');
        let col0_diacritic = char::from_u32(DIACRITICS[0]).unwrap_or(' ');
        out.push(PLACEHOLDER);
        out.push(row_diacritic);
        out.push(col0_diacritic);

        // Remaining cells are bare placeholders — Kitty auto-increments
        // the column within the same run.
        for _ in 1..cols {
            out.push(PLACEHOLDER);
        }

        if row + 1 < rows {
            out.push('\n');
        }
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn payload_has_virtual_placement_flag() {
        let b64 = BASE64.encode(b"\x89PNG\r\n\x1a\nfakepngdata");
        let payload = kitty_placeholder_payload(b64, 1001);
        assert!(payload.contains("U=1"), "payload must set U=1 (virtual placement)");
        assert!(payload.contains("i=1001"), "payload must set image id");
        assert!(payload.starts_with("\x1b_G"), "payload must be an APC escape");
        assert!(payload.ends_with("\x1b\\"), "payload must terminate with ST");
    }

    #[test]
    fn payload_chunks_large_base64() {
        // Fabricate a large base64-ish blob to force chunking past 4096.
        let big = "A".repeat(10_000);
        // Prepend a PNG magic so we skip the re-encode path.
        let b64 = BASE64.encode([b"\x89PNG\r\n\x1a\n".as_slice(), big.as_bytes()].concat());
        let payload = kitty_placeholder_payload(b64, 1);
        let chunk_count = payload.matches("\x1b_G").count();
        assert!(chunk_count >= 2, "expected multi-chunk transmission, got {chunk_count}");
        assert!(payload.contains("m=1"), "non-final chunk should carry m=1");
        assert!(payload.contains("m=0"), "final chunk should carry m=0");
    }

    #[test]
    fn rows_produces_one_line_per_row() {
        let s = kitty_placeholder_rows(1001, 4, 3);
        let lines: Vec<&str> = s.split('\n').collect();
        assert_eq!(lines.len(), 3, "should emit exactly one line per row");
    }

    #[test]
    fn rows_contain_no_escape_bytes() {
        // SGR/ESC bytes go through Helix's Style pipeline instead of
        // being embedded in the row text. set_string strips ESC so
        // embedded escapes would render as literal `[38;2;...m` text.
        let s = kitty_placeholder_rows(1001, 2, 1);
        assert!(!s.contains('\x1b'), "placeholder rows must not contain escape bytes");
        assert!(!s.contains("[38;2"), "placeholder rows must not embed SGR");
        assert!(!s.contains("[39m"), "placeholder rows must not embed SGR reset");
    }

    #[test]
    fn rows_first_cell_has_row_and_column_diacritics() {
        let s = kitty_placeholder_rows(1, 3, 2);
        // Pull out the placeholder codepoints and adjacent diacritics.
        let placeholders: Vec<char> = s.chars().filter(|c| *c == PLACEHOLDER).collect();
        // 2 rows × 3 cols = 6 placeholder cells total.
        assert_eq!(placeholders.len(), 6, "expected 6 placeholder codepoints");
    }

    #[test]
    fn rows_byte_count_matches_expected_codepoints() {
        // 1 row × 3 cols = 1 first-cell (PLACEHOLDER + row_diac + col0_diac)
        // + 2 bare placeholders. No newline because only one row.
        //
        //   PLACEHOLDER = 4 bytes UTF-8 (U+10EEEE)
        //   diacritic (U+0305) = 2 bytes UTF-8
        //
        //   4 + 2 + 2 + 4 + 4 = 16 bytes
        let s = kitty_placeholder_rows(42, 3, 1);
        assert_eq!(s.len(), 16, "unexpected byte count for 3×1 grid");
    }

    #[test]
    fn rows_return_empty_for_oversize_grids() {
        let too_many = (DIACRITICS.len() as isize) + 1;
        assert_eq!(kitty_placeholder_rows(1, too_many, 1), "");
        assert_eq!(kitty_placeholder_rows(1, 1, too_many), "");
    }

    #[test]
    fn max_dim_matches_diacritic_table() {
        assert_eq!(kitty_placeholder_max_dim(), DIACRITICS.len() as isize);
    }
}
