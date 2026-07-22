use super::cursor::{alphabetic_end, matching_brace, past_spaces_and_tabs};
use super::operators::is_big_operator;

#[derive(Debug)]
struct MathSpan {
    cmd: String,
    byte_start: usize,
    byte_end: usize,
    visual_col: usize,
    payload: SpanPayload,
}

#[derive(Debug)]
enum SpanPayload {
    BigOp { sub_text: String, sup_text: String },
    Frac { num_text: String, den_text: String },
}

impl SpanPayload {
    fn wire_fields(&self) -> (&'static str, &str, &str) {
        match self {
            Self::BigOp { sub_text, sup_text } => ("big_op", sub_text, sup_text),
            Self::Frac { num_text, den_text } => ("frac", num_text, den_text),
        }
    }
}

fn is_frac_command(name: &str) -> bool {
    matches!(name, "frac" | "dfrac" | "tfrac")
}

fn parse_math_spans(text: &str) -> Vec<MathSpan> {
    let bytes = text.as_bytes();
    let mut spans = Vec::with_capacity(bytes.len() / 32);
    let mut i = 0;
    let mut visual_col = 0usize;

    while i < bytes.len() {
        if bytes[i] != b'\\' {
            if bytes[i] == b'{' || bytes[i] == b'}' {
                i += 1;
                continue;
            }
            i += utf8_char_len(bytes[i]);
            visual_col += 1;
            continue;
        }

        let byte_start = i;
        let span_col = visual_col;
        let name_end = alphabetic_end(bytes, i + 1);
        if name_end == i + 1 {
            i += 2;
            visual_col += 1;
            continue;
        }
        let name = &text[i + 1..name_end];

        let payload_end = if is_big_operator(name) {
            let (limits, after) = scan_op_limits(bytes, text, name_end);
            limits.into_payload().map(|payload| (payload, after))
        } else if is_frac_command(name) {
            scan_frac(bytes, text, name_end).map(|(num_text, den_text, after)| {
                (SpanPayload::Frac { num_text, den_text }, after)
            })
        } else {
            None
        };

        i = match payload_end {
            Some((payload, byte_end)) => {
                spans.push(MathSpan {
                    cmd: name.to_string(),
                    byte_start,
                    byte_end,
                    visual_col: span_col,
                    payload,
                });
                byte_end
            }
            None => name_end,
        };
        visual_col += 1;
    }
    spans
}

pub fn parse_math_spans_json(text: String) -> String {
    let spans = parse_math_spans(&text);
    let mut rows = String::with_capacity(spans.len() * 64 + text.len());
    for span in &spans {
        push_row(&mut rows, span);
    }
    rows
}

fn push_row(rows: &mut String, span: &MathSpan) {
    let (kind, first, second) = span.payload.wire_fields();
    rows.push_str(kind);
    for field in [
        span.cmd.as_str(),
        &span.byte_start.to_string(),
        &span.byte_end.to_string(),
        &span.visual_col.to_string(),
    ] {
        rows.push('\t');
        rows.push_str(field);
    }
    rows.push('\t');
    escape_tsv_field(first, rows);
    rows.push('\t');
    escape_tsv_field(second, rows);
    rows.push('\n');
}

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

impl Limits {
    fn into_payload(self) -> Option<SpanPayload> {
        if self.sub.is_none() && self.sup.is_none() {
            return None;
        }
        Some(SpanPayload::BigOp {
            sub_text: self.sub.unwrap_or_default(),
            sup_text: self.sup.unwrap_or_default(),
        })
    }
}

fn scan_op_limits(bytes: &[u8], text: &str, after_cmd: usize) -> (Limits, usize) {
    let mut sub = None;
    let mut sup = None;
    let mut cursor = past_spaces_and_tabs(bytes, after_cmd);
    for _ in 0..2 {
        let slot = match bytes.get(cursor) {
            Some(b'_') => &mut sub,
            Some(b'^') => &mut sup,
            _ => break,
        };
        let (content, after) = scan_limit_group(bytes, text, cursor);
        if slot.is_none() {
            *slot = content;
        }
        cursor = past_spaces_and_tabs(bytes, after);
    }
    (Limits { sub, sup }, cursor)
}

fn scan_limit_group(bytes: &[u8], text: &str, i: usize) -> (Option<String>, usize) {
    let Some(&marked) = bytes.get(i + 1) else {
        return (None, i + 1);
    };
    if marked == b'{' {
        let close = matching_brace(bytes, i + 2);
        if close >= bytes.len() {
            return (None, close);
        }
        return (Some(text[i + 2..close].to_string()), close + 1);
    }
    let ch_len = utf8_char_len(marked);
    (
        Some(text[i + 1..i + 1 + ch_len].to_string()),
        i + 1 + ch_len,
    )
}

fn scan_frac(bytes: &[u8], text: &str, after_cmd: usize) -> Option<(String, String, usize)> {
    let (num, after_num) = scan_brace_group(bytes, text, after_cmd)?;
    let (den, after_den) = scan_brace_group(bytes, text, after_num)?;
    Some((num, den, after_den))
}

fn scan_brace_group(bytes: &[u8], text: &str, from: usize) -> Option<(String, usize)> {
    let open = past_spaces_and_tabs(bytes, from);
    if bytes.get(open) != Some(&b'{') {
        return None;
    }
    let close = matching_brace(bytes, open + 1);
    if close >= bytes.len() {
        return None;
    }
    Some((text[open + 1..close].to_string(), close + 1))
}

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

    fn limits(text: &str) -> (String, String) {
        match &parse_math_spans(text)[0].payload {
            SpanPayload::BigOp { sub_text, sup_text } => (sub_text.clone(), sup_text.clone()),
            other => panic!("expected BigOp, got {other:?}"),
        }
    }

    #[test]
    fn sum_with_both_limits() {
        let spans = parse_math_spans("\\sum_{k=0}^{n} f(k)");
        assert_eq!(spans.len(), 1);
        assert_eq!(spans[0].cmd, "sum");
        assert_eq!(limits("\\sum_{k=0}^{n} f(k)"), ("k=0".into(), "n".into()));
    }

    #[test]
    fn int_inline_limits() {
        assert_eq!(limits("\\int_0^1 f(t) dt"), ("0".into(), "1".into()));
    }

    #[test]
    fn frac_simple() {
        let spans = parse_math_spans("a + \\frac{x+1}{y} + b");
        assert_eq!(spans.len(), 1);
        match &spans[0].payload {
            SpanPayload::Frac { num_text, den_text } => {
                assert_eq!(num_text, "x+1");
                assert_eq!(den_text, "y");
            }
            other => panic!("expected Frac, got {other:?}"),
        }
    }

    #[test]
    fn multiple_spans_in_source_order() {
        assert_eq!(
            parse_math_spans("\\int_0^1 \\frac{a}{b} + \\sum_{i=1}^n i").len(),
            3
        );
    }

    #[test]
    fn bare_sum_without_limits_is_ignored() {
        assert!(parse_math_spans("\\sum f(x)").is_empty());
    }

    #[test]
    fn visual_col_accounts_for_cmd_collapse() {
        assert_eq!(parse_math_spans("\\omega \\int_0^1 f")[0].visual_col, 2);
    }

    #[test]
    fn visual_col_monotonic_across_multiple_operators() {
        let spans = parse_math_spans("\\int_0^1 \\sum_{i=1}^n \\prod_{j=1}^m x");
        assert_eq!(spans.len(), 3);
        let cols: Vec<usize> = spans.iter().map(|s| s.visual_col).collect();
        assert!(
            cols.windows(2).all(|w| w[0] < w[1]),
            "not monotonic: {cols:?}"
        );
    }

    #[test]
    fn greek_limits_round_trip_verbatim() {
        assert_eq!(
            limits("\\sum_{k\\in\\Z}^n c_k"),
            ("k\\in\\Z".into(), "n".into())
        );
    }

    #[test]
    fn wire_rows_carry_kind_command_and_offsets() {
        let rows = parse_math_spans_json("\\int_0^1 \\frac{a}{b}".to_string());
        assert_eq!(
            rows.lines().collect::<Vec<_>>(),
            vec!["big_op\tint\t0\t9\t0\t0\t1", "frac\tfrac\t9\t20\t1\ta\tb"]
        );
    }

    #[test]
    fn wire_rows_escape_tab_and_newline_in_limits() {
        let mut escaped = String::new();
        escape_tsv_field("a\tb\nc\\d", &mut escaped);
        assert_eq!(escaped, "a\\tb\\nc\\\\d");
    }
}
