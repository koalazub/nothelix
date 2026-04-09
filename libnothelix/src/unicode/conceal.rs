//! Document-level conceal computation.
//!
//! The Rust FFI layer exposes two entry points:
//!   - `compute_conceal_overlays` — scan a whole document.
//!   - `compute_conceal_overlays_for_comments` — scan only Julia comment
//!     lines (lines starting with `# `), which is the format converted
//!     notebooks use for their markdown cells.
//!
//! Both return document-relative CHARACTER offsets (not byte offsets) so
//! the Scheme layer can hand them straight to Helix's overlay API. Char
//! offsets are mapped from byte offsets via `build_byte_to_char_map`.

use serde_json::json;

use super::math_regions::find_math_regions;
use super::scanner::{latex_overlays, scan_to_vec};

/// Scan an entire document for math regions and return a JSON array of
/// `{"offset": char_off, "replacement": str}` pairs.
pub fn compute_conceal_overlays(text: String) -> String {
    let byte_to_char = build_byte_to_char_map(&text);
    let doc_char_len = text.chars().count();
    let regions = find_math_regions(&text);
    if regions.is_empty() {
        return "[]".to_string();
    }

    let mut all_overlays: Vec<serde_json::Value> = Vec::new();

    for (region_start, region_end) in regions {
        if region_end <= region_start {
            continue;
        }
        let math_text = &text[region_start..region_end];
        let json_str = latex_overlays(math_text.to_string());

        if let Ok(v) = serde_json::from_str::<serde_json::Value>(&json_str) {
            if let Some(arr) = v.as_array() {
                for obj in arr {
                    if let Some(offset) = obj.get("offset").and_then(|o| o.as_i64()) {
                        let replacement = obj
                            .get("replacement")
                            .and_then(|o| o.as_str())
                            .unwrap_or("");
                        let byte_off = region_start + offset as usize;
                        let char_off = byte_to_char.get(byte_off).copied().unwrap_or_else(|| {
                            text[..byte_off.min(text.len())].chars().count()
                        });
                        if char_off >= doc_char_len {
                            continue;
                        }
                        all_overlays.push(json!({
                            "offset": char_off,
                            "replacement": replacement
                        }));
                    }
                }
            }
        }
    }

    json!(all_overlays).to_string()
}

/// Like `compute_conceal_overlays`, but only scans lines that start with `# `
/// (Julia/notebook comment lines that contain markdown with LaTeX math).
///
/// Processes each comment line independently so `$` on different lines
/// cannot match each other (e.g. `$5` on one line and `$10` on another).
/// Returns a tab-separated format for zero-overhead Scheme parsing:
///   `"char_offset1\treplacement1\nchar_offset2\treplacement2\n..."`
///
/// All offsets are CHAR offsets (not byte offsets) because Helix's overlay
/// system uses character positions.
pub fn compute_conceal_overlays_for_comments(text: String) -> String {
    let byte_to_char = build_byte_to_char_map(&text);
    let doc_char_len = text.chars().count();
    let mut out = String::new();

    for (line_byte_start, line) in line_ranges(&text) {
        let trimmed = line.trim_end_matches('\n').trim_end_matches('\r');
        let content = match trimmed.strip_prefix("# ") {
            Some(c) => c,
            None => continue,
        };
        if content.is_empty() {
            continue;
        }

        let content_byte_start = line_byte_start + (trimmed.len() - content.len());
        let regions = find_math_regions(content);

        let content_bytes = content.as_bytes();
        for (region_start, region_end) in regions {
            if region_end <= region_start {
                continue;
            }

            // Helper: emit one overlay (byte offset in doc → char offset).
            let mut emit = |byte_off_in_content: usize, repl: &str| {
                let byte_offset = content_byte_start + byte_off_in_content;
                let char_offset = byte_to_char.get(byte_offset).copied().unwrap_or_else(|| {
                    text[..byte_offset.min(text.len())].chars().count()
                });
                if char_offset < doc_char_len {
                    out.push_str(&char_offset.to_string());
                    out.push('\t');
                    out.push_str(repl);
                    out.push('\n');
                }
            };

            // Hide opening delimiter.
            if region_start >= 2
                && content_bytes[region_start - 2] == b'\\'
                && content_bytes[region_start - 1] == b'('
            {
                emit(region_start - 2, "");
                emit(region_start - 1, "");
            } else if region_start >= 2
                && content_bytes[region_start - 2] == b'$'
                && content_bytes[region_start - 1] == b'$'
            {
                emit(region_start - 2, "");
                emit(region_start - 1, "");
            } else if region_start >= 1 && content_bytes[region_start - 1] == b'$' {
                emit(region_start - 1, "");
            }

            // Hide closing delimiter.
            if region_end + 1 < content_bytes.len()
                && content_bytes[region_end] == b'\\'
                && content_bytes[region_end + 1] == b')'
            {
                emit(region_end, "");
                emit(region_end + 1, "");
            } else if region_end + 1 < content_bytes.len()
                && content_bytes[region_end] == b'$'
                && content_bytes[region_end + 1] == b'$'
            {
                emit(region_end, "");
                emit(region_end + 1, "");
            } else if region_end < content_bytes.len() && content_bytes[region_end] == b'$' {
                emit(region_end, "");
            }

            // Emit overlays for the math content.
            let math_text = &content[region_start..region_end];
            for (offset, replacement) in scan_to_vec(math_text) {
                emit(region_start + offset, &replacement);
            }
        }
    }

    out
}

/// Build a lookup table from byte offset → char offset. Every byte
/// position maps to the char index of the character that contains that
/// byte (so mid-character bytes map correctly too).
fn build_byte_to_char_map(text: &str) -> Vec<usize> {
    let mut map = vec![0usize; text.len() + 1];
    let mut char_idx = 0;
    for (byte_idx, ch) in text.char_indices() {
        for b in byte_idx..byte_idx + ch.len_utf8() {
            map[b] = char_idx;
        }
        char_idx += 1;
    }
    map[text.len()] = char_idx;
    map
}

/// Iterate text line-by-line, yielding `(byte_offset, line_text)` for each
/// line. Unlike `str::lines()`, this preserves the byte offsets needed for
/// overlay placement.
fn line_ranges(text: &str) -> Vec<(usize, &str)> {
    let mut result = Vec::new();
    let mut start = 0;
    for line in text.split('\n') {
        result.push((start, line));
        start += line.len() + 1;
    }
    result
}
