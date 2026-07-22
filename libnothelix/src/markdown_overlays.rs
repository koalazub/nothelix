use comrak::nodes::{AstNode, LineColumn, ListType, NodeValue, Sourcepos};
use comrak::{Arena, Options, parse_document};

const SCOPE_BOLD: &str = "markup.bold";
const SCOPE_ITALIC: &str = "markup.italic";
const SCOPE_CODE: &str = "markup.raw.inline";
const SCOPE_LINK: &str = "markup.link.text";
const SCOPE_LIST: &str = "markup.list";

enum Markup {
    Heading(usize),
    Wrapped(&'static str),
    InlineCode(usize),
    Bullet,
    Escape,
    ForeignRenderer,
    Passthrough,
}

impl Markup {
    fn of(value: &NodeValue) -> Self {
        match value {
            NodeValue::Heading(heading) => Self::Heading(heading.level as usize),
            NodeValue::Strong => Self::Wrapped(SCOPE_BOLD),
            NodeValue::Emph => Self::Wrapped(SCOPE_ITALIC),
            NodeValue::Link(_) => Self::Wrapped(SCOPE_LINK),
            NodeValue::Code(code) => Self::InlineCode(code.num_backticks),
            NodeValue::Item(list) if list.list_type == ListType::Bullet => Self::Bullet,
            NodeValue::Escaped => Self::Escape,
            NodeValue::Math(_) | NodeValue::Table(_) => Self::ForeignRenderer,
            _ => Self::Passthrough,
        }
    }
}

struct CommentBody {
    markdown: String,
    line_starts: Vec<usize>,
}

impl CommentBody {
    fn strip(source: &str, char_base: usize) -> Self {
        let mut markdown = String::new();
        let mut line_starts = Vec::new();
        let mut line_abs = char_base;
        for chunk in source.split_inclusive('\n') {
            let line = chunk.strip_suffix('\n').unwrap_or(chunk);
            match comment_prefix_chars(line) {
                Some(prefix) => {
                    line_starts.push(line_abs + prefix);
                    markdown.extend(line.chars().skip(prefix));
                }
                None => line_starts.push(line_abs),
            }
            markdown.push('\n');
            line_abs += chunk.chars().count();
        }
        Self {
            markdown,
            line_starts,
        }
    }

    fn absolute(&self, pos: LineColumn) -> usize {
        self.line_starts[pos.line - 1] + (pos.column - 1)
    }
}

struct Overlays {
    body: CommentBody,
    records: String,
}

impl Overlays {
    fn overlay(&mut self, char_off: usize, replacement: &str) {
        self.records.push('O');
        self.records.push('\t');
        self.records.push_str(&char_off.to_string());
        self.records.push('\t');
        self.records.push_str(replacement);
        self.records.push('\n');
    }

    fn hide(&mut self, char_off: usize) {
        self.overlay(char_off, "");
    }

    fn hide_range(&mut self, start: usize, end: usize) {
        for off in start..end {
            self.hide(off);
        }
    }

    fn hide_before(&mut self, outer_start: LineColumn, inner_start: LineColumn) {
        if outer_start.line == inner_start.line {
            self.hide_range(
                self.body.absolute(outer_start),
                self.body.absolute(inner_start),
            );
        }
    }

    fn hide_after(&mut self, inner_end: LineColumn, outer_end: LineColumn) {
        if inner_end.line == outer_end.line {
            self.hide_range(
                self.body.absolute(inner_end) + 1,
                self.body.absolute(outer_end) + 1,
            );
        }
    }

    fn style(&mut self, start: usize, end: usize, scope: &str) {
        if end <= start {
            return;
        }
        self.records.push('S');
        self.records.push('\t');
        self.records.push_str(&start.to_string());
        self.records.push('\t');
        self.records.push_str(&end.to_string());
        self.records.push('\t');
        self.records.push_str(scope);
        self.records.push('\n');
    }

    fn style_span(&mut self, span: Sourcepos, scope: &str) {
        self.style(
            self.body.absolute(span.start),
            self.body.absolute(span.end) + 1,
            scope,
        );
    }

    fn walk<'a>(&mut self, node: &'a AstNode<'a>) {
        let (markup, span) = {
            let data = node.data.borrow();
            (Markup::of(&data.value), data.sourcepos)
        };

        match markup {
            Markup::ForeignRenderer => {}
            Markup::Heading(level) => {
                if let Some(inner) = child_span(node) {
                    self.hide_before(span.start, inner.start);
                    self.style_span(inner, &format!("markup.heading.{}", level.clamp(1, 6)));
                }
                self.walk_children(node);
            }
            Markup::Wrapped(scope) => {
                if let Some(inner) = child_span(node) {
                    self.hide_before(span.start, inner.start);
                    self.hide_after(inner.end, span.end);
                    self.style_span(inner, scope);
                }
                self.walk_children(node);
            }
            Markup::InlineCode(backticks) => {
                let start = self.body.absolute(span.start);
                let end = self.body.absolute(span.end) + 1;
                if end > start + 2 * backticks {
                    self.hide_range(start, start + backticks);
                    self.hide_range(end - backticks, end);
                    self.style(start + backticks, end - backticks, SCOPE_CODE);
                }
            }
            Markup::Bullet => {
                let start = self.body.absolute(span.start);
                self.overlay(start, "•");
                self.style(start, start + 1, SCOPE_LIST);
                self.walk_children(node);
            }
            Markup::Escape => {
                let start = self.body.absolute(span.start);
                self.hide(start);
            }
            Markup::Passthrough => self.walk_children(node),
        }
    }

    fn walk_children<'a>(&mut self, node: &'a AstNode<'a>) {
        for child in node.children() {
            self.walk(child);
        }
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

pub fn scan_markdown_overlays(text: String, char_base: isize) -> String {
    let body = CommentBody::strip(&text, char_base.max(0) as usize);
    let arena = Arena::new();
    let root = parse_document(&arena, &body.markdown, &parse_options());
    let mut overlays = Overlays {
        body,
        records: String::new(),
    };
    for child in root.children() {
        overlays.walk(child);
    }
    overlays.records
}

fn child_span<'a>(node: &'a AstNode<'a>) -> Option<Sourcepos> {
    let first = node.first_child()?.data.borrow().sourcepos;
    let last = node.last_child()?.data.borrow().sourcepos;
    Some(Sourcepos {
        start: first.start,
        end: last.end,
    })
}

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
        let out = scan("# **hi**\n");
        assert!(out.contains("O\t2\t\n"), "hide first *: {out}");
        assert!(out.contains("O\t3\t\n"), "hide second *: {out}");
        assert!(out.contains("O\t6\t\n"), "hide closing *: {out}");
        assert!(out.contains("O\t7\t\n"), "hide closing *: {out}");
        assert!(out.contains("S\t4\t6\tmarkup.bold"), "bold span: {out}");
    }

    #[test]
    fn heading_hides_hashes_and_styles_rest() {
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
        assert!(out.contains("O\t2\t\n"), "hide [: {out}");
        assert!(out.contains("O\t6\t\n"), "hide ]: {out}");
        assert!(out.contains("O\t7\t\n"), "hide (: {out}");
        assert!(out.contains("O\t9\t\n"), "hide ): {out}");
        assert!(
            out.contains("S\t3\t6\tmarkup.link.text"),
            "label span: {out}"
        );
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
        let out = scan("# plain\n# **b**\n");
        assert!(out.contains("O\t10\t\n"), "second-line offset: {out}");
        assert!(
            out.contains("S\t12\t13\tmarkup.bold"),
            "second-line span: {out}"
        );
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
        assert!(
            out.contains("S\t104\t105\tmarkup.bold"),
            "base-shifted span: {out}"
        );
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
        let out = scan("# héß **b**\n");
        assert!(
            out.contains("O\t6\t\n"),
            "hide * after multibyte text: {out}"
        );
        assert!(out.contains("S\t8\t9\tmarkup.bold"), "bold span: {out}");
    }
}
