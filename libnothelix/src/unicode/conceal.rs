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
/// Single-line math (`$...$`, `\(...\)`) is scanned per-line so `$5` on one
/// line can't accidentally match `$10` on another. Multi-line `$$...$$` blocks
/// are detected by finding `# $$` open/close lines and joining the content
/// between them.
///
/// Returns tab-separated format: `"char_offset\treplacement\n..."`
/// All offsets are CHAR offsets (not byte offsets).
pub fn compute_conceal_overlays_for_comments(text: String) -> String {
    let byte_to_char = build_byte_to_char_map(&text);
    let doc_char_len = text.chars().count();
    let mut out = String::new();

    let lines: Vec<(usize, &str)> = line_ranges(&text);

    // Helper: emit one overlay from a byte offset in the document.
    let mut emit = |doc_byte_offset: usize, repl: &str| {
        let char_offset = byte_to_char.get(doc_byte_offset).copied().unwrap_or_else(|| {
            text[..doc_byte_offset.min(text.len())].chars().count()
        });
        if char_offset < doc_char_len {
            out.push_str(&char_offset.to_string());
            out.push('\t');
            out.push_str(repl);
            out.push('\n');
        }
    };

    let mut i = 0;
    while i < lines.len() {
        let (line_byte_start, line) = lines[i];
        let trimmed = line.trim_end_matches('\n').trim_end_matches('\r');
        let content = match trimmed.strip_prefix("# ") {
            Some(c) => c,
            None => {
                i += 1;
                continue;
            }
        };

        // Detect multi-line $$ block: a line whose content is just "$$"
        if content.trim() == "$$" {
            // Find the closing # $$ line
            let open_line = i;
            let mut close_line = None;
            for (j, &(_, jline)) in lines.iter().enumerate().skip(i + 1) {
                let jt = jline
                    .trim_end_matches('\n')
                    .trim_end_matches('\r');
                if let Some(jc) = jt.strip_prefix("# ") {
                    if jc.trim() == "$$" {
                        close_line = Some(j);
                        break;
                    }
                } else {
                    break;
                }
            }

            if let Some(close) = close_line {
                // Hide the opening "# $$" line entirely (both the `#`, the
                // following space, and the two `$` chars) so the delimiter
                // line renders as visually empty and the display-math block
                // sits between two blank lines — matches how Jupyter renders
                // $$...$$ with vertical breathing room around the equation.
                emit(line_byte_start, "");
                emit(line_byte_start + 1, "");
                let open_content_start = line_byte_start + (trimmed.len() - content.len());
                let dollar1 = content.find('$').unwrap_or(0);
                emit(open_content_start + dollar1, "");
                emit(open_content_start + dollar1 + 1, "");

                // Join all content lines into a single string so environments
                // like \begin{cases}...\end{cases} span correctly. Track each
                // line's start offset in the joined string → document byte map.
                let mut joined = String::new();
                let mut offset_map: Vec<(usize, usize)> = Vec::new(); // (joined_offset, doc_byte_offset)

                for &(k_byte_start, k_line) in lines.iter().take(close).skip(open_line + 1) {
                    let k_trimmed = k_line.trim_end_matches('\n').trim_end_matches('\r');
                    if let Some(k_content) = k_trimmed.strip_prefix("# ") {
                        let k_content_start = k_byte_start + (k_trimmed.len() - k_content.len());

                        // Indent display-math content so it stands out from
                        // surrounding prose. Replace the single space after
                        // `#` with a wider run of spaces; this is the one
                        // position inside a comment line no scanner overlay
                        // ever targets, so there's no conflict.
                        if !k_content.is_empty() {
                            emit(k_content_start - 1, "      ");
                        }

                        offset_map.push((joined.len(), k_content_start));
                        joined.push_str(k_content);
                        joined.push('\n');
                    }
                }

                // Scan the joined block as a single math unit
                for (overlay_offset, replacement) in scan_to_vec(&joined) {
                    // Map overlay offset in joined string → document byte offset
                    let doc_offset = map_joined_offset(&offset_map, overlay_offset);
                    emit(doc_offset, &replacement);
                }

                // Hide the closing "# $$" line the same way as the opener.
                let (close_byte_start, close_line_text) = lines[close];
                let close_trimmed = close_line_text
                    .trim_end_matches('\n')
                    .trim_end_matches('\r');
                if let Some(close_content) = close_trimmed.strip_prefix("# ") {
                    emit(close_byte_start, "");
                    emit(close_byte_start + 1, "");
                    let close_content_start =
                        close_byte_start + (close_trimmed.len() - close_content.len());
                    let d = close_content.find('$').unwrap_or(0);
                    emit(close_content_start + d, "");
                    emit(close_content_start + d + 1, "");
                }

                i = close + 1;
                continue;
            }
            // No closing $$ found — fall through to single-line processing
        }

        // Single-line processing: inline $...$ and \(...\)
        if content.is_empty() {
            i += 1;
            continue;
        }

        let content_byte_start = line_byte_start + (trimmed.len() - content.len());
        let regions = find_math_regions(content);
        let content_bytes = content.as_bytes();

        for &(region_start, region_end) in &regions {
            if region_end <= region_start {
                continue;
            }

            // Hide opening delimiter (\( or $$ or $).
            if region_start >= 2
                && matches!(
                    (content_bytes[region_start - 2], content_bytes[region_start - 1]),
                    (b'\\', b'(') | (b'$', b'$')
                )
            {
                emit(content_byte_start + region_start - 2, "");
                emit(content_byte_start + region_start - 1, "");
            } else if region_start >= 1 && content_bytes[region_start - 1] == b'$' {
                emit(content_byte_start + region_start - 1, "");
            }

            // Hide closing delimiter (\) or $$ or $).
            if region_end + 1 < content_bytes.len()
                && matches!(
                    (content_bytes[region_end], content_bytes[region_end + 1]),
                    (b'\\', b')') | (b'$', b'$')
                )
            {
                emit(content_byte_start + region_end, "");
                emit(content_byte_start + region_end + 1, "");
            } else if region_end < content_bytes.len() && content_bytes[region_end] == b'$' {
                emit(content_byte_start + region_end, "");
            }

            // Emit overlays for the math content.
            let math_text = &content[region_start..region_end];
            for (offset, replacement) in scan_to_vec(math_text) {
                emit(content_byte_start + region_start + offset, &replacement);
            }
        }

        // Hide markdown escape backslashes OUTSIDE math regions.
        // \(a\) → (a), \[2 marks\] → [2 marks], etc.
        {
            let mut j = 0;
            while j + 1 < content_bytes.len() {
                if content_bytes[j] == b'\\' && matches!(content_bytes[j + 1], b'(' | b')' | b'[' | b']') {
                    let inside_math = regions.iter().any(|&(start, end)| {
                        let region_open = start.saturating_sub(2);
                        let region_close = (end + 2).min(content_bytes.len());
                        j >= region_open && j < region_close
                    });
                    if !inside_math {
                        emit(content_byte_start + j, "");
                        j += 2;
                        continue;
                    }
                }
                j += 1;
            }
        }

        i += 1;
    }

    out
}


/// Build a lookup table from byte offset → char offset. Every byte
/// position maps to the char index of the character that contains that
/// byte (so mid-character bytes map correctly too).
/// Map an offset in the joined $$ block string back to a document byte offset.
/// `offset_map` is (joined_offset, doc_byte_offset) for each line start.
fn map_joined_offset(offset_map: &[(usize, usize)], joined_offset: usize) -> usize {
    // Find the last line whose joined_offset <= the target
    let mut best_joined = 0;
    let mut best_doc = 0;
    for &(jo, doc) in offset_map {
        if jo <= joined_offset {
            best_joined = jo;
            best_doc = doc;
        } else {
            break;
        }
    }
    best_doc + (joined_offset - best_joined)
}

pub(super) fn build_byte_to_char_map(text: &str) -> Vec<usize> {
    let mut map = vec![0usize; text.len() + 1];
    let mut char_idx = 0;
    for (byte_idx, ch) in text.char_indices() {
        for slot in &mut map[byte_idx..byte_idx + ch.len_utf8()] {
            *slot = char_idx;
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
