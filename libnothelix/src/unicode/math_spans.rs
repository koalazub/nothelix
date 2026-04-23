//! Parse LaTeX math source into structural spans.
//!
//! Consumers (math-render.scm, math-format.rs, hypothetical future
//! tools) used to each reimplement the same walker — scanning for
//! `\begin{env}`, `\frac{..}{..}`, `\sum_{...}^{...}` — in their own
//! language. Every re-implementation drifts. This module exposes one
//! canonical parse so the consumers become span-iterators over a
//! JSON list instead of ad-hoc string walkers.
//!
//! The output isn't meant to be a full LaTeX AST (that's a much bigger
//! project — see `unicode::scanner` for the overlay-emitting walker).
//! It just covers the shapes the rendering layer cares about:
//! big-operator limits and fractions. Adding new shapes means adding a
//! variant and a detector — every consumer auto-benefits.

use serde::Serialize;

/// Big operators that take `_{sub}^{sup}` limits as their payload.
/// Mirrors the list in `scanner::is_big_operator`; keep in sync.
const BIG_OPS: &[&str] = &[
    "sum", "prod", "coprod",
    "int", "iint", "iiint", "iiiint", "oint", "oiint", "oiiint",
    "bigcup", "bigcap", "bigvee", "bigwedge",
    "bigoplus", "bigotimes", "bigodot", "biguplus", "bigsqcup",
    "lim", "liminf", "limsup",
    "min", "max", "sup", "inf",
    "argmin", "argmax",
];

const FRAC_CMDS: &[&str] = &["frac", "dfrac", "tfrac"];

#[derive(Debug, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum MathSpan {
    /// A big operator with its limits (if any).
    BigOp {
        /// Operator command name (`"sum"`, `"int"`, …) — the Unicode
        /// glyph selection is the consumer's business.
        cmd: String,
        /// Byte offset where the leading `\` sits.
        byte_start: usize,
        /// Byte offset one past the last consumed byte (past the last
        /// limit group, or past the command name if no limits).
        byte_end: usize,
        /// Visible column where the concealed operator glyph will
        /// appear, accounting for prior `\cmd`-to-glyph collapses and
        /// hidden braces. Zero-based.
        visual_col: usize,
        /// Raw text of the subscript limit (or empty).
        sub_text: String,
        /// Raw text of the superscript limit (or empty).
        sup_text: String,
    },
    /// A `\frac{num}{den}` (or `\dfrac`/`\tfrac`).
    Frac {
        cmd: String,
        byte_start: usize,
        byte_end: usize,
        visual_col: usize,
        num_text: String,
        den_text: String,
    },
}

/// Walk `text` once and emit every interesting LaTeX shape as a
/// [`MathSpan`]. Returns spans in source order.
///
/// Visual column is tracked incrementally by the outer loop so the
/// cost is O(N) total, not O(N·K) for K spans. Each `\cmd` collapses
/// to one visible glyph (1 col), braces are hidden (0 cols), other
/// chars are 1 col (we're not trying to handle wide-glyph width
/// correctness — the renderer will show them at whatever width the
/// terminal decides).
pub fn parse_math_spans(text: &str) -> Vec<MathSpan> {
    let bytes = text.as_bytes();
    let mut spans = Vec::with_capacity(bytes.len() / 32);
    let mut i = 0;
    let mut visual_col = 0usize;

    while i < bytes.len() {
        if bytes[i] != b'\\' {
            // Skip braces (hidden by conceal) without incrementing col.
            if bytes[i] == b'{' || bytes[i] == b'}' {
                i += 1;
                continue;
            }
            i += utf8_char_len(bytes[i]);
            visual_col += 1;
            continue;
        }

        // `\...` — record the starting visual column before we advance
        // past the command, since the emitted span is anchored there.
        let span_start = i;
        let span_visual_col = visual_col;

        let name_end = scan_name(bytes, i + 1);
        if name_end == i + 1 {
            // `\<non-letter>` — counts as one visible grapheme.
            i += 2;
            visual_col += 1;
            continue;
        }
        let name = &text[i + 1..name_end];

        if BIG_OPS.contains(&name) {
            let (limits, after) = scan_op_limits(bytes, text, name_end);
            if limits.sub.is_some() || limits.sup.is_some() {
                spans.push(MathSpan::BigOp {
                    cmd: name.to_string(),
                    byte_start: span_start,
                    byte_end: after,
                    visual_col: span_visual_col,
                    sub_text: limits.sub.unwrap_or_default(),
                    sup_text: limits.sup.unwrap_or_default(),
                });
                // The concealed glyph occupies one visual col; the rest
                // of the `\cmd…limits…` range is hidden by the renderer.
                i = after;
                visual_col += 1;
                continue;
            }
            i = name_end;
            visual_col += 1;
            continue;
        }

        if FRAC_CMDS.contains(&name) {
            if let Some((num, den, after)) = scan_frac(bytes, text, name_end) {
                spans.push(MathSpan::Frac {
                    cmd: name.to_string(),
                    byte_start: span_start,
                    byte_end: after,
                    visual_col: span_visual_col,
                    num_text: num,
                    den_text: den,
                });
                i = after;
                visual_col += 1;
                continue;
            }
        }

        // Any other `\cmd` — one visual glyph (concealed to a symbol).
        i = name_end;
        visual_col += 1;
    }
    spans
}

/// FFI entry point — parse and emit one span per line in tab-separated
/// form. Matches the line-based wire shape the rest of the plugin uses
/// (see `conceal::compute_conceal_overlays_for_comments`), so the
/// Scheme consumer doesn't need a JSON parser.
///
/// Format per line:
///   `big_op\tCMD\tBYTE_START\tBYTE_END\tVISUAL_COL\tSUB\tSUP`
///   `frac\tCMD\tBYTE_START\tBYTE_END\tVISUAL_COL\tNUM\tDEN`
///
/// Literal tabs / newlines inside sub/sup/num/den text are escaped as
/// `\t` / `\n` so the row shape stays intact. Unescape on the Scheme
/// side (`string-replace` passes, or just tolerate — math source
/// almost never contains tabs).
pub fn parse_math_spans_json(text: String) -> String {
    use std::fmt::Write as _;
    let spans = parse_math_spans(&text);
    // Pre-size the output: each span costs ~64 bytes of framing plus its
    // own text. Prevents most String reallocations in the hot path.
    let mut out = String::with_capacity(spans.len() * 64 + text.len());
    for span in spans {
        match span {
            MathSpan::BigOp { cmd, byte_start, byte_end, visual_col, sub_text, sup_text } => {
                out.push_str("big_op\t");
                out.push_str(&cmd);
                let _ = write!(out, "\t{byte_start}\t{byte_end}\t{visual_col}\t");
                escape_tsv_field(&sub_text, &mut out);
                out.push('\t');
                escape_tsv_field(&sup_text, &mut out);
                out.push('\n');
            }
            MathSpan::Frac { cmd, byte_start, byte_end, visual_col, num_text, den_text } => {
                out.push_str("frac\t");
                out.push_str(&cmd);
                let _ = write!(out, "\t{byte_start}\t{byte_end}\t{visual_col}\t");
                escape_tsv_field(&num_text, &mut out);
                out.push('\t');
                escape_tsv_field(&den_text, &mut out);
                out.push('\n');
            }
        }
    }
    out
}

/// Escape the three TSV-sensitive characters (`\`, `\t`, `\n`) in a
/// single pass directly into `out`. The common case — fields that
/// contain none of these — is one `push_str` with no allocation,
/// replacing the old three-`.replace()` chain that always allocated
/// three intermediate strings regardless of content.
fn escape_tsv_field(s: &str, out: &mut String) {
    if !s.bytes().any(|b| matches!(b, b'\\' | b'\t' | b'\n')) {
        out.push_str(s);
        return;
    }
    for ch in s.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '\t' => out.push_str("\\t"),
            '\n' => out.push_str("\\n"),
            c => out.push(c),
        }
    }
}

struct Limits {
    sub: Option<String>,
    sup: Option<String>,
}

/// Consume up to two consecutive `_{..}` / `^{..}` (or inline `_x` /
/// `^x`) groups after `after_cmd`. Either order.
fn scan_op_limits(bytes: &[u8], text: &str, after_cmd: usize) -> (Limits, usize) {
    let mut sub = None;
    let mut sup = None;
    let mut cursor = skip_ws(bytes, after_cmd);
    for _ in 0..2 {
        if cursor >= bytes.len() {
            break;
        }
        let which = match bytes[cursor] {
            b'_' => true,
            b'^' => false,
            _ => break,
        };
        let (content, after) = scan_limit_group(bytes, text, cursor);
        let slot = if which { &mut sub } else { &mut sup };
        if slot.is_none() {
            *slot = content;
        }
        cursor = skip_ws(bytes, after);
    }
    (Limits { sub, sup }, cursor)
}

/// Parse a `_{..}` / `^{..}` / `_x` / `^x` starting at `bytes[i]`
/// (which must be `_` or `^`). Returns `(Some(content), after_idx)`
/// or `(None, after_idx)` if malformed.
fn scan_limit_group(bytes: &[u8], text: &str, i: usize) -> (Option<String>, usize) {
    if i + 1 >= bytes.len() {
        return (None, i + 1);
    }
    if bytes[i + 1] == b'{' {
        let close = find_matching_brace(bytes, i + 2);
        if close >= bytes.len() {
            return (None, close);
        }
        let body = &text[i + 2..close];
        return (Some(body.to_string()), close + 1);
    }
    // Inline single-char limit.
    let ch_len = utf8_char_len(bytes[i + 1]);
    let body = &text[i + 1..i + 1 + ch_len];
    (Some(body.to_string()), i + 1 + ch_len)
}

/// Parse `\frac{num}{den}` at `after_cmd` (right after the name).
fn scan_frac(bytes: &[u8], text: &str, after_cmd: usize) -> Option<(String, String, usize)> {
    let j = skip_ws(bytes, after_cmd);
    if j >= bytes.len() || bytes[j] != b'{' {
        return None;
    }
    let num_close = find_matching_brace(bytes, j + 1);
    if num_close >= bytes.len() {
        return None;
    }
    let num = text[j + 1..num_close].to_string();
    let k = skip_ws(bytes, num_close + 1);
    if k >= bytes.len() || bytes[k] != b'{' {
        return None;
    }
    let den_close = find_matching_brace(bytes, k + 1);
    if den_close >= bytes.len() {
        return None;
    }
    let den = text[k + 1..den_close].to_string();
    Some((num, den, den_close + 1))
}

fn scan_name(bytes: &[u8], mut i: usize) -> usize {
    while i < bytes.len() && bytes[i].is_ascii_alphabetic() {
        i += 1;
    }
    i
}

fn skip_ws(bytes: &[u8], mut i: usize) -> usize {
    while i < bytes.len() && matches!(bytes[i], b' ' | b'\t') {
        i += 1;
    }
    i
}

/// Byte-index of the byte just past the matching closing brace,
/// starting from `j` which points JUST past an opening `{`.
fn find_matching_brace(bytes: &[u8], mut j: usize) -> usize {
    let mut depth = 1i32;
    while j < bytes.len() && depth > 0 {
        match bytes[j] {
            b'{' => depth += 1,
            b'}' => depth -= 1,
            _ => {}
        }
        if depth == 0 {
            return j; // caller advances past `}` itself
        }
        j += 1;
    }
    j
}

/// UTF-8 char length from leading byte.
fn utf8_char_len(b: u8) -> usize {
    match b {
        0..=0x7F => 1,
        0xC0..=0xDF => 2,
        0xE0..=0xEF => 3,
        _ => 4,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sum_with_both_limits() {
        let spans = parse_math_spans("\\sum_{k=0}^{n} f(k)");
        assert_eq!(spans.len(), 1);
        match &spans[0] {
            MathSpan::BigOp { cmd, sub_text, sup_text, .. } => {
                assert_eq!(cmd, "sum");
                assert_eq!(sub_text, "k=0");
                assert_eq!(sup_text, "n");
            }
            other => panic!("expected BigOp, got {other:?}"),
        }
    }

    #[test]
    fn int_inline_limits() {
        let spans = parse_math_spans("\\int_0^1 f(t) dt");
        assert_eq!(spans.len(), 1);
        match &spans[0] {
            MathSpan::BigOp { sub_text, sup_text, .. } => {
                assert_eq!(sub_text, "0");
                assert_eq!(sup_text, "1");
            }
            _ => panic!(),
        }
    }

    #[test]
    fn frac_simple() {
        let spans = parse_math_spans("a + \\frac{x+1}{y} + b");
        assert_eq!(spans.len(), 1);
        match &spans[0] {
            MathSpan::Frac { num_text, den_text, .. } => {
                assert_eq!(num_text, "x+1");
                assert_eq!(den_text, "y");
            }
            _ => panic!(),
        }
    }

    #[test]
    fn multiple_spans_in_source_order() {
        let spans = parse_math_spans("\\int_0^1 \\frac{a}{b} + \\sum_{i=1}^n i");
        assert_eq!(spans.len(), 3);
    }

    #[test]
    fn bare_sum_without_limits_is_ignored() {
        let spans = parse_math_spans("\\sum f(x)");
        assert!(spans.is_empty());
    }

    #[test]
    fn visual_col_accounts_for_cmd_collapse() {
        // `\omega` (6 bytes) renders as one glyph → col 1, then a space
        // (col 2); the `\int` span anchored at byte 7 should report
        // visual_col == 2. Previously this asserted directly against a
        // helper; now it exercises the incremental counter embedded in
        // `parse_math_spans` itself.
        let spans = parse_math_spans("\\omega \\int_0^1 f");
        match &spans[0] {
            MathSpan::BigOp { visual_col, .. } => assert_eq!(*visual_col, 2),
            _ => panic!(),
        }
    }

    #[test]
    fn visual_col_monotonic_across_multiple_operators() {
        // Regression guard against the old O(N²) per-span recomputation:
        // column values must be strictly monotonic in source order.
        let spans = parse_math_spans("\\int_0^1 \\sum_{i=1}^n \\prod_{j=1}^m x");
        assert_eq!(spans.len(), 3);
        let cols: Vec<usize> = spans.iter().map(|s| match s {
            MathSpan::BigOp { visual_col, .. } | MathSpan::Frac { visual_col, .. } => *visual_col,
        }).collect();
        assert!(cols.windows(2).all(|w| w[0] < w[1]), "cols not monotonic: {cols:?}");
    }

    #[test]
    fn greek_limits_round_trip_verbatim() {
        let spans = parse_math_spans("\\sum_{k\\in\\Z}^n c_k");
        match &spans[0] {
            MathSpan::BigOp { sub_text, sup_text, .. } => {
                assert_eq!(sub_text, "k\\in\\Z");
                assert_eq!(sup_text, "n");
            }
            _ => panic!(),
        }
    }
}
