//! In-buffer markdown rendering: parse markdown comment text with comrak and
//! emit display overlays + style spans that the fork applies WITHOUT mutating
//! the buffer.
//!
//! Output is TSV, one record per line, all offsets absolute document char
//! indices (`char_base` + local):
//!   `O<TAB>CHAR_OFF<TAB>REPLACEMENT`  grapheme overlay (empty REPLACEMENT hides)
//!   `S<TAB>START<TAB>END<TAB>SCOPE`   style span over a char range
//!
//! The plugin hands one `@markdown` cell's buffer text (lines still carry the
//! Julia `# ` comment prefix) plus the cell's absolute char base. Math and
//! table nodes emit nothing — they have their own renderers (math_image /
//! table_image) and stay source-visible.

#![allow(clippy::needless_pass_by_value)]

use comrak::nodes::{AstNode, ListType, NodeValue, Sourcepos};
use comrak::{Arena, Options, parse_document};

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

    fn hide_range(&mut self, start: usize, end: usize) {
        for off in start..end {
            self.hide(off);
        }
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

/// Absolute char offset of each stripped line's first char in the original
/// document, so comrak's (line, column) positions map back through the Julia
/// comment prefixes.
struct LineMap {
    starts: Vec<usize>,
}

impl LineMap {
    fn abs(&self, pos: comrak::nodes::LineColumn) -> usize {
        self.starts[pos.line - 1] + (pos.column - 1)
    }

    fn abs_start(&self, sp: Sourcepos) -> usize {
        self.abs(sp.start)
    }

    fn abs_end_exclusive(&self, sp: Sourcepos) -> usize {
        self.abs(sp.end) + 1
    }
}

fn parse_options() -> Options<'static> {
    let mut opts = Options::default();
    opts.extension.table = true;
    opts.extension.math_dollars = true;
    opts.render.sourcepos = true;
    opts.parse.sourcepos_chars = true;
    opts.parse.escaped_char_spans = true;
    opts
}

/// Scan one markdown cell's text. `char_base` is the absolute document char
/// index of `text`'s first char.
pub fn scan_markdown_overlays(text: String, char_base: isize) -> String {
    let base = char_base.max(0) as usize;
    let mut stripped = String::new();
    let mut starts = Vec::new();
    let mut line_abs = base;

    for (line, had_nl) in lines_with_nl(&text) {
        let line_char_len = line.chars().count() + usize::from(had_nl);
        match comment_prefix_chars(line) {
            Some(prefix) => {
                starts.push(line_abs + prefix);
                stripped.extend(line.chars().skip(prefix));
            }
            None => starts.push(line_abs),
        }
        stripped.push('\n');
        line_abs += line_char_len;
    }

    let map = LineMap { starts };
    let arena = Arena::new();
    let root = parse_document(&arena, &stripped, &parse_options());
    let mut emit = Emit { out: String::new() };
    for child in root.children() {
        walk(child, &map, &mut emit);
    }
    emit.out
}

fn walk<'a>(node: &'a AstNode<'a>, map: &LineMap, emit: &mut Emit) {
    let (value_kind, sp) = {
        let data = node.data.borrow();
        (discriminant_info(&data.value), data.sourcepos)
    };

    match value_kind {
        Kind::Skip => {}
        Kind::Heading(level) => {
            if let Some(inner) = child_span(node) {
                hide_gap(map, emit, sp.start, inner.start);
                emit.style(
                    map.abs(inner.start),
                    map.abs(inner.end) + 1,
                    &format!("markup.heading.{}", level.clamp(1, 6)),
                );
            }
            walk_children(node, map, emit);
        }
        Kind::Styled(scope) => {
            if let Some(inner) = child_span(node) {
                hide_gap(map, emit, sp.start, inner.start);
                hide_after(map, emit, inner.end, sp.end);
                emit.style(map.abs(inner.start), map.abs(inner.end) + 1, scope);
            }
            walk_children(node, map, emit);
        }
        Kind::Code(backticks) => {
            let start = map.abs_start(sp);
            let end = map.abs_end_exclusive(sp);
            if end > start + 2 * backticks {
                emit.hide_range(start, start + backticks);
                emit.hide_range(end - backticks, end);
                emit.style(start + backticks, end - backticks, SCOPE_CODE);
            }
        }
        Kind::Link => {
            if let Some(inner) = child_span(node) {
                hide_gap(map, emit, sp.start, inner.start);
                hide_after(map, emit, inner.end, sp.end);
                emit.style(map.abs(inner.start), map.abs(inner.end) + 1, SCOPE_LINK);
            }
            walk_children(node, map, emit);
        }
        Kind::BulletItem => {
            let start = map.abs_start(sp);
            emit.overlay(start, "•");
            emit.style(start, start + 1, SCOPE_LIST);
            walk_children(node, map, emit);
        }
        Kind::Escaped => {
            emit.hide(map.abs_start(sp));
        }
        Kind::Recurse => walk_children(node, map, emit),
    }
}

enum Kind {
    Heading(usize),
    Styled(&'static str),
    Code(usize),
    Link,
    BulletItem,
    Escaped,
    Skip,
    Recurse,
}

fn discriminant_info(value: &NodeValue) -> Kind {
    match value {
        NodeValue::Heading(h) => Kind::Heading(h.level as usize),
        NodeValue::Strong => Kind::Styled(SCOPE_BOLD),
        NodeValue::Emph => Kind::Styled(SCOPE_ITALIC),
        NodeValue::Code(c) => Kind::Code(c.num_backticks),
        NodeValue::Link(_) => Kind::Link,
        NodeValue::Item(list) if list.list_type == ListType::Bullet => Kind::BulletItem,
        NodeValue::Escaped => Kind::Escaped,
        NodeValue::Math(_) | NodeValue::Table(_) => Kind::Skip,
        _ => Kind::Recurse,
    }
}

fn walk_children<'a>(node: &'a AstNode<'a>, map: &LineMap, emit: &mut Emit) {
    for child in node.children() {
        walk(child, map, emit);
    }
}

fn child_span<'a>(node: &'a AstNode<'a>) -> Option<Sourcepos> {
    let first = node.first_child()?.data.borrow().sourcepos;
    let last = node.last_child()?.data.borrow().sourcepos;
    Some(Sourcepos {
        start: first.start,
        end: last.end,
    })
}

/// Hide the marker chars between a node's start and its first child's start,
/// when both sit on the same line.
fn hide_gap(
    map: &LineMap,
    emit: &mut Emit,
    outer_start: comrak::nodes::LineColumn,
    inner_start: comrak::nodes::LineColumn,
) {
    if outer_start.line == inner_start.line {
        emit.hide_range(map.abs(outer_start), map.abs(inner_start));
    }
}

/// Hide the marker chars between a node's last child's end and the node's
/// own end, when both sit on the same line.
fn hide_after(
    map: &LineMap,
    emit: &mut Emit,
    inner_end: comrak::nodes::LineColumn,
    outer_end: comrak::nodes::LineColumn,
) {
    if inner_end.line == outer_end.line {
        emit.hide_range(map.abs(inner_end) + 1, map.abs(outer_end) + 1);
    }
}

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
        let out = scan("# | a | b |\n# |---|---|\n# | x | y |\n");
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

    #[test]
    fn inline_math_spans_left_alone() {
        let out = scan("# span of $\\mathbb{R}^3$ and $x_1 * y_2$\n");
        assert!(out.is_empty(), "inline math untouched: {out:?}");
    }

    #[test]
    fn backslash_escape_only_before_punctuation() {
        let out = scan("# a \\* b and \\mathbb\n");
        assert!(out.contains("O\t4\t\n"), "hide escape before *: {out}");
        assert!(!out.contains("O\t13\t\n"), "\\m is not an escape: {out}");
    }

    #[test]
    fn multibyte_text_keeps_offsets_aligned() {
        // "# héß **b**": h=2 é=3 ß=4 sp=5 *=6 *=7 b=8 *=9 *=10
        let out = scan("# héß **b**\n");
        assert!(out.contains("O\t6\t\n"), "hide * after multibyte text: {out}");
        assert!(out.contains("S\t8\t9\tmarkup.bold"), "bold span: {out}");
    }
}
