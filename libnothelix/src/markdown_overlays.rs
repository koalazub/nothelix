//! In-buffer markdown rendering: scan markdown comment text and emit display
//! overlays + style spans that the fork applies WITHOUT mutating the buffer.
//!
//! Output is TSV, one record per line, all offsets absolute document char
//! indices (`char_base` + local):
//!   `O<TAB>CHAR_OFF<TAB>REPLACEMENT`  grapheme overlay (empty REPLACEMENT hides)
//!   `S<TAB>START<TAB>END<TAB>SCOPE`   style span over a char range
//!
//! The plugin hands one `@markdown` cell's buffer text (lines still carry the
//! Julia `# ` comment prefix) plus the cell's absolute char base. Math (`$$`)
//! and table (`|`) lines are skipped — they have their own renderers
//! (math_image / table_image) and stay source-visible.

#![allow(clippy::needless_pass_by_value)]

const SCOPE_BOLD: &str = "markup.bold";
const SCOPE_ITALIC: &str = "markup.italic";
const SCOPE_CODE: &str = "markup.raw.inline";
const SCOPE_LINK: &str = "markup.link.text";
const SCOPE_LIST: &str = "markup.list";

struct Emit {
    out: String,
}

impl Emit {
    fn overlay(&mut self, char_off: usize, replacement: &str) {
        self.out.push('O');
        self.out.push('\t');
        self.out.push_str(&char_off.to_string());
        self.out.push('\t');
        self.out.push_str(replacement);
        self.out.push('\n');
    }

    fn hide(&mut self, char_off: usize) {
        self.overlay(char_off, "");
    }

    fn style(&mut self, start: usize, end: usize, scope: &str) {
        if end <= start {
            return;
        }
        self.out.push('S');
        self.out.push('\t');
        self.out.push_str(&start.to_string());
        self.out.push('\t');
        self.out.push_str(&end.to_string());
        self.out.push('\t');
        self.out.push_str(scope);
        self.out.push('\n');
    }
}

/// Scan one markdown cell's text. `char_base` is the absolute document char
/// index of `text`'s first char.
pub fn scan_markdown_overlays(text: String, char_base: isize) -> String {
    let base = char_base.max(0) as usize;
    let mut emit = Emit { out: String::new() };
    let mut line_char_start = base;
    let mut in_math = false;

    for (line, had_nl) in lines_with_nl(&text) {
        let line_char_len = line.chars().count() + usize::from(had_nl);

        let Some(prefix_chars) = comment_prefix_chars(line) else {
            line_char_start += line_char_len;
            continue;
        };
        let body: String = line.chars().skip(prefix_chars).collect();
        let body_trim = body.trim();

        if body_trim == "$$" {
            in_math = !in_math;
            line_char_start += line_char_len;
            continue;
        }
        if in_math || is_single_line_math(body_trim) || body_trim.starts_with('|') {
            line_char_start += line_char_len;
            continue;
        }

        let body_abs = line_char_start + prefix_chars;
        scan_line(&body, body_abs, &mut emit);

        line_char_start += line_char_len;
    }

    emit.out
}

/// Split into lines, reporting whether each had a trailing newline (so the
/// caller can count the `\n` as one document char).
fn lines_with_nl(text: &str) -> Vec<(&str, bool)> {
    let mut v = Vec::new();
    let mut start = 0;
    for (i, b) in text.bytes().enumerate() {
        if b == b'\n' {
            v.push((&text[start..i], true));
            start = i + 1;
        }
    }
    if start < text.len() {
        v.push((&text[start..], false));
    }
    v
}

/// Number of leading chars that make up the Julia comment prefix (`# ` → 2,
/// bare `#` → 1), or `None` if the line is not a comment.
fn comment_prefix_chars(line: &str) -> Option<usize> {
    let mut chars = line.chars();
    if chars.next()? != '#' {
        return None;
    }
    match chars.next() {
        Some(' ') => Some(2),
        _ => Some(1),
    }
}

fn is_single_line_math(body: &str) -> bool {
    body.starts_with("$$") && body.ends_with("$$") && body.chars().count() > 4
}

/// Scan one markdown line body. `abs0` is the absolute char offset of `body`'s
/// first char.
fn scan_line(body: &str, abs0: usize, emit: &mut Emit) {
    let chars: Vec<char> = body.chars().collect();
    let abs = |i: usize| abs0 + i;

    // Heading: leading run of '#' then a space.
    if let Some((hashes, text_start)) = heading_prefix(&chars) {
        for i in 0..text_start {
            emit.hide(abs(i));
        }
        emit.style(abs(text_start), abs(chars.len()), &heading_scope(hashes));
        return;
    }

    // List bullet at line start: `- `, `* `, `+ ` → render a bullet glyph.
    if chars.len() >= 2 && matches!(chars[0], '-' | '*' | '+') && chars[1] == ' ' {
        emit.overlay(abs(0), "•");
        emit.style(abs(0), abs(1), SCOPE_LIST);
        scan_inline(&chars, 2, abs0, emit);
        return;
    }

    scan_inline(&chars, 0, abs0, emit);
}

fn heading_prefix(chars: &[char]) -> Option<(usize, usize)> {
    if chars.first() != Some(&'#') {
        return None;
    }
    let mut hashes = 0;
    while chars.get(hashes) == Some(&'#') {
        hashes += 1;
    }
    if chars.get(hashes) == Some(&' ') {
        Some((hashes, hashes + 1))
    } else {
        None
    }
}

fn heading_scope(level: usize) -> String {
    format!("markup.heading.{}", level.clamp(1, 6))
}

/// Scan inline markdown (bold/italic/code/link) over `chars[start..]`.
fn scan_inline(chars: &[char], start: usize, abs0: usize, emit: &mut Emit) {
    let abs = |i: usize| abs0 + i;
    let n = chars.len();
    let mut i = start;

    while i < n {
        let c = chars[i];

        if c == '\\' && i + 1 < n {
            emit.hide(abs(i));
            i += 2;
            continue;
        }

        if c == '`'
            && let Some(j) = find_char(chars, i + 1, '`')
        {
            emit.hide(abs(i));
            emit.hide(abs(j));
            emit.style(abs(i + 1), abs(j), SCOPE_CODE);
            i = j + 1;
            continue;
        }

        if (c == '*' || c == '_')
            && chars.get(i + 1) == Some(&c)
            && let Some(j) = find_double(chars, i + 2, c)
        {
            emit.hide(abs(i));
            emit.hide(abs(i + 1));
            emit.hide(abs(j));
            emit.hide(abs(j + 1));
            emit.style(abs(i + 2), abs(j), SCOPE_BOLD);
            i = j + 2;
            continue;
        }

        if (c == '*' || c == '_')
            && let Some(j) = find_char(chars, i + 1, c)
        {
            emit.hide(abs(i));
            emit.hide(abs(j));
            emit.style(abs(i + 1), abs(j), SCOPE_ITALIC);
            i = j + 1;
            continue;
        }

        if c == '['
            && let Some(close) = find_char(chars, i + 1, ']')
            && chars.get(close + 1) == Some(&'(')
            && let Some(rparen) = find_char(chars, close + 2, ')')
        {
            emit.hide(abs(i));
            emit.hide(abs(close));
            for k in (close + 1)..=rparen {
                emit.hide(abs(k));
            }
            emit.style(abs(i + 1), abs(close), SCOPE_LINK);
            i = rparen + 1;
            continue;
        }

        i += 1;
    }
}

fn find_char(chars: &[char], from: usize, target: char) -> Option<usize> {
    (from..chars.len()).find(|&i| chars[i] == target)
}

/// Find a doubled marker (`**` / `__`) starting at or after `from`.
fn find_double(chars: &[char], from: usize, marker: char) -> Option<usize> {
    let mut i = from;
    while i + 1 < chars.len() {
        if chars[i] == marker && chars[i + 1] == marker {
            return Some(i);
        }
        i += 1;
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scan(text: &str) -> String {
        scan_markdown_overlays(text.to_string(), 0)
    }

    #[test]
    fn bold_hides_markers_and_styles_inner() {
        // "# **hi**"  chars: # space * * h i * *
        let out = scan("# **hi**\n");
        assert!(out.contains("O\t2\t\n"), "hide first *: {out}");
        assert!(out.contains("O\t3\t\n"), "hide second *: {out}");
        assert!(out.contains("O\t6\t\n"), "hide closing *: {out}");
        assert!(out.contains("O\t7\t\n"), "hide closing *: {out}");
        assert!(out.contains("S\t4\t6\tmarkup.bold"), "bold span: {out}");
    }

    #[test]
    fn heading_hides_hashes_and_styles_rest() {
        // "# ## Title" -> julia prefix 2 chars, then "## Title"
        let out = scan("# ## Title\n");
        assert!(out.contains("O\t2\t\n"), "hide #: {out}");
        assert!(out.contains("O\t3\t\n"), "hide #: {out}");
        assert!(out.contains("O\t4\t\n"), "hide space: {out}");
        assert!(out.contains("markup.heading.2"), "h2 scope: {out}");
    }

    #[test]
    fn inline_code_styled() {
        let out = scan("# `x`\n");
        assert!(out.contains("markup.raw.inline"), "code scope: {out}");
        assert!(out.contains("O\t2\t\n"), "hide opening backtick: {out}");
    }

    #[test]
    fn link_hides_brackets_and_url() {
        let out = scan("# [lbl](u)\n");
        assert!(out.contains("markup.link.text"), "link scope: {out}");
        // "[lbl](u)" body starts at char 2: [ l b l ] ( u )
        assert!(out.contains("O\t2\t\n"), "hide [: {out}");
        assert!(out.contains("O\t6\t\n"), "hide ]: {out}");
        assert!(out.contains("O\t7\t\n"), "hide (: {out}");
        assert!(out.contains("O\t9\t\n"), "hide ): {out}");
        assert!(out.contains("S\t3\t6\tmarkup.link.text"), "label span: {out}");
    }

    #[test]
    fn list_bullet_substituted() {
        let out = scan("# - item\n");
        assert!(out.contains("O\t2\t•\n"), "bullet glyph: {out}");
        assert!(out.contains("markup.list"), "list scope: {out}");
    }

    #[test]
    fn math_lines_are_skipped() {
        let out = scan("# $$\n# x = 1\n# $$\n");
        assert!(out.is_empty(), "math block produced no overlays: {out:?}");
    }

    #[test]
    fn table_rows_are_skipped() {
        let out = scan("# | a | b |\n# |---|---|\n");
        assert!(out.is_empty(), "table rows skipped: {out:?}");
    }

    #[test]
    fn offsets_accumulate_across_lines() {
        // line 1 "# plain\n" = 8 chars (incl nl); bold on line 2.
        let out = scan("# plain\n# **b**\n");
        // body of line 2 starts at char 8 + prefix 2 = 10; markers at 10,11.
        assert!(out.contains("O\t10\t\n"), "second-line offset: {out}");
        assert!(out.contains("S\t12\t13\tmarkup.bold"), "second-line span: {out}");
    }

    #[test]
    fn plain_text_untouched() {
        let out = scan("# just words here\n");
        assert!(out.is_empty(), "no markup -> no overlays: {out:?}");
    }

    #[test]
    fn char_base_offsets_absolute() {
        let out = scan_markdown_overlays("# **b**\n".to_string(), 100);
        assert!(out.contains("O\t102\t\n"), "base-shifted offset: {out}");
        assert!(out.contains("S\t104\t105\tmarkup.bold"), "base-shifted span: {out}");
    }
}
