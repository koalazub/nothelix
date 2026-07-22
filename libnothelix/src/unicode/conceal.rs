use serde_json::json;

use super::char_offsets::CharOffsets;
use super::math_regions::find_math_regions;
use super::scanner::{ScannerOptions, scan_region};
use crate::error::{Result, ffi};

#[cfg(feature = "native")]
pub use comments::compute_conceal_overlays_for_comments_with_options;

pub fn compute_conceal_overlays(text: String) -> String {
    ffi(document_overlays_json(&text))
}

fn document_overlays_json(text: &str) -> Result<String> {
    let regions = find_math_regions(text);
    if regions.is_empty() {
        return Ok("[]".to_string());
    }
    let offsets = CharOffsets::of(text);
    let mut overlays: Vec<serde_json::Value> = Vec::new();
    for (start, end) in regions {
        if end <= start {
            continue;
        }
        for (offset, replacement) in scan_region(&text[start..end], ScannerOptions::default()) {
            if let Some(char_offset) = offsets.visible(start + offset)? {
                overlays.push(json!({"offset": char_offset, "replacement": replacement}));
            }
        }
    }
    Ok(json!(overlays).to_string())
}

#[cfg(feature = "native")]
mod comments {
    use std::ops::Range;

    use super::super::overlay::CharOffsetTsv;
    use super::{CharOffsets, Result, ScannerOptions, ffi, find_math_regions, scan_region};

    const COMMENT_PREFIX: &str = "# ";
    const DISPLAY_FENCE: &str = "$$";

    pub fn compute_conceal_overlays_for_comments_with_options(
        text: String,
        hide_math_layout: bool,
    ) -> String {
        ffi(comment_overlays_tsv(
            &text,
            ScannerOptions { hide_math_layout },
        ))
    }

    fn comment_overlays_tsv(text: &str, options: ScannerOptions) -> Result<String> {
        let offsets = CharOffsets::of(text);
        let lines = line_starts(text);
        let mut tsv = CharOffsetTsv::new(&offsets);

        let mut i = 0;
        while i < lines.len() {
            let (line_start, line) = lines[i];
            let Some(content) = comment_content(line) else {
                i += 1;
                continue;
            };
            if content.trim() == DISPLAY_FENCE
                && let Some(close) = display_block_close(&lines, i)
            {
                i = close + 1;
                continue;
            }
            if !content.is_empty() {
                conceal_comment_content(
                    content,
                    line_start + COMMENT_PREFIX.len(),
                    options,
                    &mut tsv,
                )?;
            }
            i += 1;
        }

        Ok(tsv.into_rows())
    }

    fn conceal_comment_content(
        content: &str,
        content_start: usize,
        options: ScannerOptions,
        tsv: &mut CharOffsetTsv,
    ) -> Result<()> {
        let bytes = content.as_bytes();
        let regions = find_math_regions(content);

        for &(start, end) in &regions {
            if end <= start || is_display_region(bytes, start) {
                continue;
            }
            for byte in opening_delimiter(bytes, start)
                .into_iter()
                .chain(closing_delimiter(bytes, end))
                .flatten()
            {
                tsv.hide(content_start + byte)?;
            }
            for (offset, replacement) in scan_region(&content[start..end], options) {
                tsv.push(content_start + start + offset, &replacement)?;
            }
        }

        for byte in markdown_escapes(bytes, &regions) {
            tsv.hide(content_start + byte)?;
        }
        Ok(())
    }

    fn comment_content(line: &str) -> Option<&str> {
        line.trim_end_matches('\n')
            .trim_end_matches('\r')
            .strip_prefix(COMMENT_PREFIX)
    }

    fn display_block_close(lines: &[(usize, &str)], open: usize) -> Option<usize> {
        for (j, &(_, line)) in lines.iter().enumerate().skip(open + 1) {
            match comment_content(line) {
                Some(body) if body.trim() == DISPLAY_FENCE => return Some(j),
                Some(_) => {}
                None => return None,
            }
        }
        None
    }

    fn is_display_region(bytes: &[u8], start: usize) -> bool {
        start >= 2 && bytes[start - 1] == b'$' && bytes[start - 2] == b'$'
    }

    fn opening_delimiter(bytes: &[u8], start: usize) -> Option<Range<usize>> {
        if start >= 2
            && matches!(
                (bytes[start - 2], bytes[start - 1]),
                (b'\\', b'(') | (b'$', b'$')
            )
        {
            Some(start - 2..start)
        } else if start >= 1 && bytes[start - 1] == b'$' {
            Some(start - 1..start)
        } else {
            None
        }
    }

    fn closing_delimiter(bytes: &[u8], end: usize) -> Option<Range<usize>> {
        if end + 1 < bytes.len()
            && matches!((bytes[end], bytes[end + 1]), (b'\\', b')') | (b'$', b'$'))
        {
            Some(end..end + 2)
        } else if end < bytes.len() && bytes[end] == b'$' {
            Some(end..end + 1)
        } else {
            None
        }
    }

    fn markdown_escapes(bytes: &[u8], regions: &[(usize, usize)]) -> Vec<usize> {
        let mut escapes = Vec::new();
        let mut j = 0;
        while j + 1 < bytes.len() {
            if bytes[j] == b'\\'
                && matches!(bytes[j + 1], b'(' | b')' | b'[' | b']')
                && !regions.iter().any(|&(start, end)| {
                    (start.saturating_sub(2)..(end + 2).min(bytes.len())).contains(&j)
                })
            {
                escapes.push(j);
                j += 2;
                continue;
            }
            j += 1;
        }
        escapes
    }

    fn line_starts(text: &str) -> Vec<(usize, &str)> {
        let mut lines = Vec::new();
        let mut start = 0;
        for line in text.split('\n') {
            lines.push((start, line));
            start += line.len() + 1;
        }
        lines
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tsv(input: &str) -> String {
        compute_conceal_overlays_for_comments_with_options(input.to_string(), false)
    }

    fn overlays(input: &str) -> Vec<(usize, String)> {
        tsv(input)
            .lines()
            .filter(|l| !l.is_empty())
            .map(|line| {
                let (offset, replacement) = line.split_once('\t').unwrap();
                (offset.parse().unwrap(), replacement.to_string())
            })
            .collect()
    }

    fn hidden(input: &str) -> Vec<usize> {
        overlays(input)
            .into_iter()
            .filter(|(_, r)| r.is_empty())
            .map(|(o, _)| o)
            .collect()
    }

    fn conceals(input: &str, glyph: &str) -> bool {
        overlays(input).iter().any(|(_, r)| r == glyph)
    }

    fn apply_overlays(input: &str) -> String {
        let replacements: std::collections::HashMap<usize, String> =
            overlays(input).into_iter().collect();
        let mut out = String::new();
        for (i, ch) in input.char_indices() {
            match replacements.get(&i) {
                Some(replacement) => out.push_str(replacement),
                None => out.push(ch),
            }
        }
        out
    }

    #[test]
    fn document_scan_places_overlays_at_region_offsets() {
        let input = r#"some text $\alpha + \beta$ more text"#;
        let json = compute_conceal_overlays(input.to_string());
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        let offsets: Vec<usize> = parsed
            .as_array()
            .unwrap()
            .iter()
            .map(|o| o["offset"].as_u64().unwrap() as usize)
            .collect();
        assert!(!offsets.is_empty());
        assert!(offsets.contains(&"some text $".len()), "{offsets:?}");
    }

    #[test]
    fn document_without_math_yields_an_empty_array() {
        assert_eq!(
            compute_conceal_overlays("plain text no math".to_string()),
            "[]"
        );
    }

    #[test]
    fn pipe_table_lines_produce_no_overlays() {
        let rows = tsv("# | A | B |\n# |---|---|\n# | x | y |\n");
        assert!(!rows.contains('│'), "no box overlays: {rows:?}");
        assert!(!rows.contains('├'), "no rule overlays: {rows:?}");
    }

    #[test]
    fn inline_math_on_pipe_line_still_conceals() {
        let rows = tsv("# cost | $\\alpha$ |\n");
        assert!(rows.contains('α'), "math should still conceal: {rows:?}");
        assert!(!rows.contains('│'), "no box rows: {rows:?}");
    }

    #[test]
    fn fourier_transform_lines_conceal() {
        assert_eq!(
            apply_overlays(
                "# $\\hat{f}(\\xi) = \\int_{-\\infty}^{\\infty} f(x) \\, e^{-2\\pi i x \\xi} \\, dx$\n"
            ),
            "# f\u{302}(ξ) = ∫_-∞^∞ f(x) \u{2006} e^-2π i x ξ \u{2006} dx\n"
        );
    }

    #[test]
    fn parseval_line_conceals_cleanly() {
        assert_eq!(
            apply_overlays("# $\\int |f(x)|^2 \\, dx = \\int |\\hat{f}(\\xi)|^2 \\, d\\xi$\n"),
            "# ∫ |f(x)|² \u{2006} dx = ∫ |f\u{302}(ξ)|² \u{2006} dξ\n"
        );
    }

    #[test]
    fn combining_mark_scans_nested_content() {
        assert_eq!(apply_overlays("# $\\hat{\\mathbf{b}}$\n"), "# 𝐛\u{302}\n");
        assert_eq!(apply_overlays("# $\\vec{\\mathbf{v}}$\n"), "# 𝐯\u{20D7}\n");
        assert_eq!(apply_overlays("# $\\bar{\\mathbb{R}}$\n"), "# ℝ\u{304}\n");
        assert_eq!(apply_overlays("# $\\hat{b}$\n"), "# b\u{302}\n");
        assert_eq!(
            apply_overlays(
                "# \\(d\\) Plot $\\mathbf{b}$, $\\hat{\\mathbf{b}}$,  and $\\mathbf{r}$ as vectors in $\\mathbb{R}^3$\n"
            ),
            "# (d) Plot 𝐛, 𝐛\u{302},  and 𝐫 as vectors in ℝ³\n"
        );
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
        let rows = tsv("# subspace of $\\mathbb{R}^3$.\n");
        assert!(rows.contains('ℝ'), "double-struck R emitted: {rows:?}");
        assert!(rows.contains('³'), "superscript 3 emitted: {rows:?}");
    }

    #[test]
    fn simple_alpha_conceals_and_hides_delimiters() {
        assert!(conceals("# $\\alpha$\n", "α"));
        assert!(hidden("# $\\alpha$\n").len() >= 2);
    }

    #[test]
    fn non_comment_lines_are_skipped() {
        assert!(tsv("x = rand(10)\nprintln(\"value: $x\")\n").is_empty());
    }

    #[test]
    fn comment_lines_conceal_amid_code() {
        assert!(conceals(
            "x = 1\n# The value $\\beta$ is cool\ny = 2\n",
            "β"
        ));
    }

    #[test]
    fn a_lone_dollar_per_line_opens_no_region() {
        assert!(overlays("# cost is $5\n# price is $10\n").is_empty());
    }

    #[test]
    fn markdown_escaped_parens_hide_only_their_backslashes() {
        assert_eq!(hidden("# \\(a\\) Find the eigenvalues\n").len(), 2);
    }

    #[test]
    fn markdown_escaped_brackets_hide_only_their_backslashes() {
        assert_eq!(hidden("# Some text \\[2 marks\\]\n").len(), 2);
    }

    #[test]
    fn backslash_parens_around_real_math_conceal() {
        assert!(!overlays("# \\(\\alpha + \\beta\\)\n").is_empty());
    }

    #[test]
    fn multiple_regions_on_one_line_all_conceal() {
        assert!(conceals("# $\\alpha$ and $\\beta$\n", "α"));
        assert!(conceals("# $\\alpha$ and $\\beta$\n", "β"));
    }

    #[test]
    fn font_command_conceals_in_comments() {
        assert!(conceals("# $\\mathbf{v}$\n", "𝐯"));
    }

    #[test]
    fn eigenvalue_equation_conceals_every_symbol() {
        let input = "# $A\\mathbf{v} = \\lambda\\mathbf{v}$\n";
        assert!(conceals(input, "λ"));
        assert!(conceals(input, "𝐯"));
    }

    #[test]
    fn offsets_are_char_positions_not_byte_positions() {
        let input = "# ═══ separator\n# $\\alpha$\n";
        let (offset, _) = overlays(input)
            .into_iter()
            .find(|(_, r)| r == "α")
            .expect("α overlay");
        assert!(offset < 30, "char offset {offset} looks like a byte offset");
    }

    #[test]
    fn no_overlay_lands_outside_a_math_region() {
        let input = "# \\(b\\) Verify the eigenvalue equation $A\\mathbf{v} = \\lambda\\mathbf{v}$ numerically for each eigenpair. What is the maximum residual $\\|A\\mathbf{v} - \\lambda\\mathbf{v}\\|$?\n";
        let word_span = |word: &str| {
            let byte = input.find(word).unwrap();
            let start = input[..byte].chars().count();
            start..start + word.chars().count()
        };
        for word in ["numerically", "maximum"] {
            let span = word_span(word);
            for (offset, replacement) in overlays(input) {
                assert!(
                    !span.contains(&offset),
                    "stray overlay inside {word:?} at {offset}: {replacement:?}"
                );
            }
        }
    }

    #[test]
    fn empty_document_yields_nothing() {
        assert!(tsv("").is_empty());
    }

    #[test]
    fn document_without_comment_lines_yields_nothing() {
        assert!(tsv("x = 1\ny = 2\nz = x + y\n").is_empty());
    }

    #[test]
    fn display_block_is_owned_by_the_image_renderer() {
        let input = "# $$\n# y_n = x_n - \\alpha^2\\, x_{n-2}\n# $$\n";
        assert!(overlays(input).is_empty(), "{:?}", overlays(input));
    }

    #[test]
    fn display_block_with_cases_is_owned_by_the_image_renderer() {
        let input = "# $$\n#    h_n = \\begin{cases}\n#    1 & 0 \\leq n \\leq 2 \\\\\\\\\n#    0 & \\text{otherwise}\n#    \\end{cases}\n# $$\n";
        assert!(overlays(input).is_empty(), "{:?}", overlays(input));
    }

    #[test]
    fn norm_and_superscript_conceal_together() {
        let input = "# $\\|x - P_K x\\|^2$\n";
        assert!(conceals(input, "‖"));
        assert!(conceals(input, "²"));
    }

    #[test]
    fn ldots_conceals_to_ellipsis() {
        assert!(conceals("# $K = 1, 2, \\ldots, 32$\n", "…"));
    }
}
