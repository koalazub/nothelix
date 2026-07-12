// Steel's `register_fn` marshals values from the Steel VM and requires
// the registered fn's signature to take owned types (`String`), not
// borrows. The owned type is load-bearing for the FFI dispatcher.
#![allow(clippy::needless_pass_by_value)]

//! Document-level conceal computation.
//!
//! The Rust FFI layer exposes two entry points:
//!   - `compute_conceal_overlays` — scan a whole document.
//!   - `compute_conceal_overlays_for_comments_with_options` — scan only
//!     Julia comment lines (lines starting with `# `), which is the format
//!     converted notebooks use for their markdown cells.
//!
//! Both return document-relative CHARACTER offsets (not byte offsets) so
//! the Scheme layer can hand them straight to Helix's overlay API. Char
//! offsets are mapped from byte offsets via `build_byte_to_char_map`.

use serde_json::json;

use super::math_regions::find_math_regions;
use super::scanner::{ScannerOptions, scan_to_vec_opts};

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
        for (offset, replacement) in scan_to_vec_opts(math_text, ScannerOptions::default()) {
            let byte_off = region_start + offset;
            let char_off = byte_to_char
                .get(byte_off)
                .copied()
                .unwrap_or_else(|| text[..byte_off.min(text.len())].chars().count());
            if char_off >= doc_char_len {
                continue;
            }
            all_overlays.push(json!({
                "offset": char_off,
                "replacement": replacement
            }));
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
///
/// Exposed as the `-with-options` FFI so the math-render plugin can pass
/// `hide_math_layout=true` per-call without a process-global flag.
pub fn compute_conceal_overlays_for_comments_with_options(
    text: String,
    hide_math_layout: bool,
) -> String {
    let opts = ScannerOptions { hide_math_layout };
    let byte_to_char = build_byte_to_char_map(&text);
    let doc_char_len = text.chars().count();
    let mut out = String::new();

    let lines: Vec<(usize, &str)> = line_ranges(&text);

    // Helper: emit one overlay from a byte offset in the document.
    let mut emit = |doc_byte_offset: usize, repl: &str| {
        let char_offset = byte_to_char
            .get(doc_byte_offset)
            .copied()
            .unwrap_or_else(|| text[..doc_byte_offset.min(text.len())].chars().count());
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
        let content = if let Some(c) = trimmed.strip_prefix("# ") {
            c
        } else {
            i += 1;
            continue;
        };

        // A `$$ ... $$` display block is owned by the Typst image renderer
        // (math-image.scm); inline conceal skips it so the image is the
        // sole visual rather than three renderers compositing.
        if content.trim() == "$$" {
            let mut close_line = None;
            for (j, &(_, jline)) in lines.iter().enumerate().skip(i + 1) {
                let jt = jline.trim_end_matches('\n').trim_end_matches('\r');
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
                i = close + 1;
                continue;
            }
        }

        // A markdown pipe table is rendered as a transparent Typst image by
        // table-image.scm (overlays can't align columns — one grapheme per
        // source char). The `# | ... |` lines carry no `$` math, so inline
        // conceal naturally leaves them untouched; nothing to do here.

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

            // `$$ x $$` display regions belong to the image renderer.
            if region_start >= 2
                && content_bytes[region_start - 1] == b'$'
                && content_bytes[region_start - 2] == b'$'
            {
                continue;
            }

            // Hide opening delimiter (\( or $$ or $).
            if region_start >= 2
                && matches!(
                    (
                        content_bytes[region_start - 2],
                        content_bytes[region_start - 1]
                    ),
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
            for (offset, replacement) in scan_to_vec_opts(math_text, opts) {
                emit(content_byte_start + region_start + offset, &replacement);
            }
        }

        // Hide markdown escape backslashes OUTSIDE math regions.
        // \(a\) → (a), \[2 marks\] → [2 marks], etc.
        {
            let mut j = 0;
            while j + 1 < content_bytes.len() {
                if content_bytes[j] == b'\\'
                    && matches!(content_bytes[j + 1], b'(' | b')' | b'[' | b']')
                {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pipe_table_lines_produce_no_overlays() {
        // Tables render as Typst images (table-image.scm); conceal must leave
        // their `# | ... |` lines untouched — no box-drawing overlays, no tofu.
        let text = "# | A | B |\n# |---|---|\n# | x | y |\n";
        let tsv = compute_conceal_overlays_for_comments_with_options(text.to_string(), false);
        assert!(!tsv.contains('│'), "no box overlays: {tsv:?}");
        assert!(!tsv.contains('├'), "no rule overlays: {tsv:?}");
    }

    #[test]
    fn inline_math_on_pipe_line_still_conceals() {
        let text = "# cost | $\\alpha$ |\n";
        let tsv = compute_conceal_overlays_for_comments_with_options(text.to_string(), false);
        assert!(tsv.contains('α'), "math should still conceal: {tsv:?}");
        assert!(!tsv.contains('│'), "no box rows: {tsv:?}");
    }

    /// Apply the byte-offset overlays to the (ASCII-LaTeX) input to get the
    /// visible text a reader would see. Input is ASCII so every overlay offset
    /// is a char boundary; an empty replacement hides that byte.
    fn apply_overlays(input: &str) -> String {
        let tsv = compute_conceal_overlays_for_comments_with_options(input.to_string(), false);
        let mut reps: std::collections::HashMap<usize, &str> = std::collections::HashMap::new();
        for line in tsv.lines() {
            if let Some((off, rep)) = line.split_once('\t')
                && let Ok(n) = off.parse::<usize>()
            {
                reps.insert(n, rep);
            }
        }
        let mut out = String::new();
        for (i, ch) in input.char_indices() {
            match reps.get(&i) {
                Some(rep) => out.push_str(rep),
                None => out.push(ch),
            }
        }
        out
    }

    #[test]
    fn transpose_top_renders_superscript_t() {
        assert_eq!(apply_overlays("# $A^\\top$\n"), "# Aᵀ\n");
        assert_eq!(apply_overlays("# $A^{\\top}$\n"), "# Aᵀ\n");
        assert_eq!(apply_overlays("# $A^\\intercal$\n"), "# Aᵀ\n");
        assert_eq!(apply_overlays("# $A^{\\mathsf{T}}$\n"), "# Aᵀ\n");
    }

    #[test]
    fn plain_letter_transpose_still_renders() {
        assert_eq!(apply_overlays("# $A^T$\n"), "# Aᵀ\n");
    }

    #[test]
    fn command_superscripts_render_raised_glyph() {
        assert_eq!(apply_overlays("# $90^\\circ$\n"), "# 90°\n");
        assert_eq!(apply_overlays("# $f^\\prime$\n"), "# f′\n");
        assert_eq!(apply_overlays("# $A^\\dagger$\n"), "# A†\n");
        assert_eq!(apply_overlays("# $V^\\perp$\n"), "# V⊥\n");
    }

    #[test]
    fn command_subscripts_render_lowered_glyph() {
        assert_eq!(apply_overlays("# $v_\\perp$\n"), "# v⊥\n");
        assert_eq!(apply_overlays("# $v_\\parallel$\n"), "# v∥\n");
    }

    #[test]
    fn unmapped_command_superscript_leaves_caret() {
        assert_eq!(apply_overlays("# $x^\\alpha$\n"), "# x^α\n");
    }

    #[test]
    fn font_command_digits_render() {
        assert_eq!(apply_overlays("# $\\mathbf{0}$\n"), "# 𝟎\n");
        assert_eq!(apply_overlays("# $\\mathbf{1}$\n"), "# 𝟏\n");
        assert_eq!(
            apply_overlays("# $A\\mathbf{x} = \\mathbf{0}$\n"),
            "# A𝐱 = 𝟎\n"
        );
        assert_eq!(apply_overlays("# $\\mathbb{1}$\n"), "# 𝟙\n");
        assert_eq!(apply_overlays("# $\\mathtt{0}$\n"), "# 𝟶\n");
    }

    #[test]
    fn mathbb_conceals_to_double_struck() {
        let text = "# subspace of $\\mathbb{R}^3$.\n";
        let tsv = compute_conceal_overlays_for_comments_with_options(text.to_string(), false);
        assert!(tsv.contains('ℝ'), "double-struck R emitted: {tsv:?}");
        assert!(tsv.contains('³'), "superscript 3 emitted: {tsv:?}");
    }
}
