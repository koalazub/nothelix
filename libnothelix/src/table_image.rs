//! Markdown pipe tables rendered as transparent inline images via Typst.
//!
//! A markdown table can't be aligned as inline overlay text — the fork's
//! overlay layer renders one grapheme per source char, leaving no room to
//! pad columns. So a table is typeset by Typst into a transparent,
//! theme-coloured table image and placed inline exactly like display math
//! (see math-image.scm). The render path is shared with math via
//! `compile_typst_to_svg`.

#![allow(clippy::needless_pass_by_value)]

use crate::math_image::{compile_typst_to_svg, math_json};

/// Column alignment parsed from the `|:--|--:|:-:|` rule line.
#[derive(Clone, Copy)]
enum Align {
    Left,
    Center,
    Right,
}

impl Align {
    fn typst(self) -> &'static str {
        match self {
            Align::Left => "left",
            Align::Center => "center",
            Align::Right => "right",
        }
    }
}

struct ParsedTable {
    aligns: Vec<Align>,
    header: Vec<String>,
    body: Vec<Vec<String>>,
}

/// Render a run of markdown table source lines (already stripped of any
/// `# ` comment prefix) into a transparent table image. Returns the same
/// JSON shape as `render_math_to_svg` (`b64`/`width`/`height`/`error`).
pub fn render_table_to_svg(block: String, font_size_pt: isize, text_color: String) -> String {
    let pt = font_size_pt.clamp(8, 96) as f64;
    let color = sanitize_hex_color(&text_color);
    let lines: Vec<&str> = block.lines().collect();

    let Some(table) = parse_table(&lines) else {
        return math_json("", 0, 0, "not a markdown table");
    };

    let doc = build_table_document(&table, pt, &color);
    match compile_typst_to_svg(doc) {
        Ok((b64, width, height)) => math_json(&b64, width, height, ""),
        Err(e) => math_json("", 0, 0, &e),
    }
}

fn parse_table(lines: &[&str]) -> Option<ParsedTable> {
    let mut header: Option<Vec<String>> = None;
    let mut aligns: Option<Vec<Align>> = None;
    let mut body: Vec<Vec<String>> = Vec::new();

    for line in lines {
        let trimmed = line.trim();
        if !trimmed.contains('|') {
            return None;
        }
        let cells = split_cells(trimmed);
        if is_separator(&cells) {
            if header.is_none() || aligns.is_some() {
                return None;
            }
            aligns = Some(cells.iter().map(|c| parse_align(c)).collect());
        } else if header.is_none() {
            header = Some(cells.iter().map(|c| md_inline_to_plain(c)).collect());
        } else {
            body.push(cells.iter().map(|c| md_inline_to_plain(c)).collect());
        }
    }

    let (header, aligns) = (header?, aligns?);
    if body.is_empty() {
        return None;
    }

    let ncols = header
        .len()
        .max(body.iter().map(Vec::len).max().unwrap_or(0))
        .max(1);
    let aligns = (0..ncols)
        .map(|i| aligns.get(i).copied().unwrap_or(Align::Left))
        .collect();

    Some(ParsedTable {
        aligns,
        header,
        body,
    })
}

fn build_table_document(table: &ParsedTable, font_size_pt: f64, color: &str) -> String {
    let ncols = table.aligns.len();
    let aligns = table
        .aligns
        .iter()
        .map(|a| a.typst())
        .collect::<Vec<_>>()
        .join(", ");

    let mut header_cells = String::new();
    for i in 0..ncols {
        let cell = table.header.get(i).map_or("", String::as_str);
        header_cells.push_str(&format!("strong(\"{}\"), ", typst_escape(cell)));
    }

    let mut body_cells = String::new();
    for row in &table.body {
        for i in 0..ncols {
            let cell = row.get(i).map_or("", String::as_str);
            body_cells.push_str(&format!("\"{}\", ", typst_escape(cell)));
        }
    }

    format!(
        "#set page(width: auto, height: auto, margin: 4pt, fill: none)\n\
         #set text(size: {font_size_pt:.1}pt, fill: rgb(\"{color}\"))\n\
         #table(\n\
         \x20 columns: {ncols},\n\
         \x20 align: ({aligns}),\n\
         \x20 stroke: 0.5pt + rgb(\"{color}\"),\n\
         \x20 inset: 6pt,\n\
         \x20 table.header({header_cells}),\n\
         \x20 {body_cells}\n\
         )"
    )
}

/// Escape a cell for a Typst string literal: only `\` and `"` are special.
fn typst_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        if c == '\\' || c == '"' {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

fn parse_align(cell: &str) -> Align {
    let t = cell.trim();
    let left = t.starts_with(':');
    let right = t.ends_with(':');
    match (left, right) {
        (true, true) => Align::Center,
        (false, true) => Align::Right,
        _ => Align::Left,
    }
}

/// Normalise a caller-supplied colour to 6-digit hex, falling back to a
/// light grey legible on dark themes. Mirrors `math_image::sanitize_hex_color`.
fn sanitize_hex_color(input: &str) -> String {
    let hex = input.trim().trim_start_matches('#');
    let expanded = match hex.len() {
        3 => hex.chars().flat_map(|c| [c, c]).collect::<String>(),
        6 => hex.to_string(),
        _ => return "e8e8e8".to_string(),
    };
    if expanded.bytes().all(|b| b.is_ascii_hexdigit()) {
        expanded.to_ascii_lowercase()
    } else {
        "e8e8e8".to_string()
    }
}

/// Split a table line into trimmed cells, honouring a leading/trailing `|`
/// and treating `\|` as a literal pipe.
fn split_cells(line: &str) -> Vec<String> {
    let mut s = line.trim();
    s = s.strip_prefix('|').unwrap_or(s);
    s = s.strip_suffix('|').unwrap_or(s);

    let mut cells = Vec::new();
    let mut cur = String::new();
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\\'
            && let Some(&next) = chars.peek()
        {
            cur.push(c);
            cur.push(next);
            chars.next();
            continue;
        }
        if c == '|' {
            cells.push(cur.trim().to_string());
            cur.clear();
        } else {
            cur.push(c);
        }
    }
    cells.push(cur.trim().to_string());
    cells
}

/// A `|:--|--:|:-:|` rule line: every cell is dashes/colons with a dash.
fn is_separator(cells: &[String]) -> bool {
    !cells.is_empty()
        && cells.iter().all(|c| {
            let t = c.trim();
            !t.is_empty() && t.chars().all(|ch| ch == '-' || ch == ':') && t.contains('-')
        })
}

/// Strip markdown inline markup to visible text: `` `code` `` → `code`,
/// `[label](url)` → `label`, `**bold**` → `bold`, `\x` → `x`.
fn md_inline_to_plain(cell: &str) -> String {
    let bytes = cell.as_bytes();
    let mut out = String::new();
    let mut i = 0;

    while i < cell.len() {
        let c = bytes[i];

        if c == b'\\' && i + 1 < cell.len() {
            let ch = cell[i + 1..].chars().next().unwrap();
            out.push(ch);
            i += 1 + ch.len_utf8();
            continue;
        }

        if c == b'`'
            && let Some(rel) = cell[i + 1..].find('`')
        {
            out.push_str(&cell[i + 1..i + 1 + rel]);
            i = i + 1 + rel + 1;
            continue;
        }

        if c == b'['
            && let Some(close) = cell[i + 1..].find(']')
        {
            let label = &cell[i + 1..i + 1 + close];
            let after = i + 1 + close + 1;
            if cell.as_bytes().get(after) == Some(&b'(')
                && let Some(rp) = cell[after + 1..].find(')')
            {
                out.push_str(&md_inline_to_plain(label));
                i = after + 1 + rp + 1;
                continue;
            }
        }

        if c == b'*'
            && cell.as_bytes().get(i + 1) == Some(&b'*')
            && let Some(rel) = cell[i + 2..].find("**")
        {
            out.push_str(&md_inline_to_plain(&cell[i + 2..i + 2 + rel]));
            i = i + 2 + rel + 2;
            continue;
        }

        let ch = cell[i..].chars().next().unwrap();
        out.push(if ch == '\t' { ' ' } else { ch });
        i += ch.len_utf8();
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "| File | Description | URL |\n\
                          |:-----|:-----------:|----:|\n\
                          | a.h5 | strain data | link |";

    #[test]
    fn parses_header_separator_body() {
        let lines: Vec<&str> = SAMPLE.lines().collect();
        let t = parse_table(&lines).expect("should parse");
        assert_eq!(t.header, vec!["File", "Description", "URL"]);
        assert_eq!(t.body.len(), 1);
        assert_eq!(t.aligns.len(), 3);
    }

    #[test]
    fn alignment_from_separator_colons() {
        assert!(matches!(parse_align(":---"), Align::Left));
        assert!(matches!(parse_align(":--:"), Align::Center));
        assert!(matches!(parse_align("---:"), Align::Right));
        assert!(matches!(parse_align("---"), Align::Left));
    }

    #[test]
    fn rejects_without_separator() {
        let lines = ["| a | b |", "| c | d |"];
        assert!(parse_table(&lines).is_none());
    }

    #[test]
    fn rejects_non_table() {
        let lines = ["just prose", "more prose"];
        assert!(parse_table(&lines).is_none());
    }

    #[test]
    fn escaped_pipe_stays_in_cell() {
        let md = "| a \\| b | c |\n|--|--|\n| d | e |";
        let lines: Vec<&str> = md.lines().collect();
        let t = parse_table(&lines).expect("should parse");
        assert_eq!(t.header[0], "a | b");
    }

    #[test]
    fn inline_markup_stripped() {
        let md = "| `code` | [lbl](http://x) | **b** |\n|--|--|--|\n| x | y | z |";
        let lines: Vec<&str> = md.lines().collect();
        let t = parse_table(&lines).expect("should parse");
        assert_eq!(t.header, vec!["code", "lbl", "b"]);
    }

    #[test]
    fn quotes_and_backslashes_escaped_for_typst() {
        assert_eq!(typst_escape(r#"a"b\c"#), r#"a\"b\\c"#);
    }

    #[test]
    fn renders_table_to_svg() {
        let json = render_table_to_svg(SAMPLE.to_string(), 14, "e8e8e8".to_string());
        assert!(json.contains("\"error\":\"\""), "typst error: {json}");
        assert!(json.contains("\"b64\":\"PHN2Zy"), "expected svg payload: {json}");
        assert!(!json.contains("\"width\":0"), "zero width: {json}");
    }

    #[test]
    fn non_table_input_reports_error() {
        let json = render_table_to_svg("hello world".to_string(), 14, "e8e8e8".to_string());
        assert!(
            !json.contains("\"error\":\"\""),
            "expected error for non-table: {json}"
        );
    }
}
