//! Markdown/LaTeX → Typst conversion for notebook export, driven by the
//! comrak AST. Math nodes go through mitex; everything else maps to Typst
//! markup directly.
//!
//! Reference: <https://typst.app/docs/guides/guide-for-latex-users>

use comrak::nodes::{AstNode, ListType, NodeValue};
use comrak::{Arena, Options, parse_document};

fn parse_options() -> Options<'static> {
    let mut opts = Options::default();
    opts.extension.table = true;
    opts.extension.math_dollars = true;
    opts.extension.strikethrough = true;
    opts
}

/// Convert a full markdown cell (with LaTeX math) to Typst.
pub fn md_to_typst(md: &str) -> Result<String, String> {
    let arena = Arena::new();
    let root = parse_document(&arena, md, &parse_options());
    let mut out = String::new();
    for block in root.children() {
        block_to_typst(block, &mut out)?;
    }
    Ok(out)
}

fn block_to_typst<'a>(node: &'a AstNode<'a>, out: &mut String) -> Result<(), String> {
    let value = node.data.borrow().value.clone();
    match value {
        NodeValue::Heading(h) => {
            out.push_str(&"=".repeat(h.level as usize));
            out.push(' ');
            out.push_str(&inlines_to_typst(node)?);
            out.push_str("\n\n");
        }
        NodeValue::Paragraph => {
            out.push_str(&inlines_to_typst(node)?);
            out.push_str("\n\n");
        }
        NodeValue::List(list) => {
            let marker = match list.list_type {
                ListType::Bullet => "- ",
                ListType::Ordered => "+ ",
            };
            for item in node.children() {
                out.push_str(marker);
                let mut body = String::new();
                for child in item.children() {
                    block_to_typst(child, &mut body)?;
                }
                out.push_str(body.trim_end());
                out.push('\n');
            }
            out.push('\n');
        }
        NodeValue::CodeBlock(cb) => {
            out.push_str("```");
            out.push_str(&cb.info);
            out.push('\n');
            out.push_str(&cb.literal);
            if !cb.literal.ends_with('\n') {
                out.push('\n');
            }
            out.push_str("```\n\n");
        }
        NodeValue::Table(table) => {
            out.push_str("#table(\n  columns: ");
            out.push_str(&table.num_columns.to_string());
            out.push(',');
            for row in node.children() {
                out.push_str("\n ");
                for cell in row.children() {
                    out.push_str(" [");
                    out.push_str(&inlines_to_typst(cell)?);
                    out.push_str("],");
                }
            }
            out.push_str("\n)\n\n");
        }
        NodeValue::BlockQuote => {
            let mut body = String::new();
            for child in node.children() {
                block_to_typst(child, &mut body)?;
            }
            out.push_str("#quote(block: true)[\n");
            out.push_str(body.trim_end());
            out.push_str("\n]\n\n");
        }
        NodeValue::ThematicBreak => {
            out.push_str("#line(length: 100%)\n\n");
        }
        NodeValue::HtmlBlock(html) => {
            out.push_str(&html.literal);
            out.push('\n');
        }
        _ => {
            out.push_str(&inlines_to_typst(node)?);
            out.push_str("\n\n");
        }
    }
    Ok(())
}

fn inlines_to_typst<'a>(node: &'a AstNode<'a>) -> Result<String, String> {
    let mut out = String::new();
    for child in node.children() {
        inline_to_typst(child, &mut out)?;
    }
    Ok(out)
}

fn inline_to_typst<'a>(node: &'a AstNode<'a>, out: &mut String) -> Result<(), String> {
    let value = node.data.borrow().value.clone();
    match value {
        NodeValue::Text(text) => out.push_str(&escape_typst(&text)),
        NodeValue::SoftBreak => out.push(' '),
        NodeValue::LineBreak => out.push_str("\\ "),
        NodeValue::Strong => {
            out.push('*');
            out.push_str(&inlines_to_typst(node)?);
            out.push('*');
        }
        NodeValue::Emph => {
            out.push('_');
            out.push_str(&inlines_to_typst(node)?);
            out.push('_');
        }
        NodeValue::Strikethrough => {
            out.push_str("#strike[");
            out.push_str(&inlines_to_typst(node)?);
            out.push(']');
        }
        NodeValue::Code(code) => {
            out.push('`');
            out.push_str(&code.literal);
            out.push('`');
        }
        NodeValue::Math(math) => {
            let converted = latex_to_typst_math(&math.literal)?;
            if math.display_math {
                out.push_str("$ ");
                out.push_str(&converted);
                out.push_str(" $");
            } else {
                out.push('$');
                out.push_str(&converted);
                out.push('$');
            }
        }
        NodeValue::Link(link) => {
            out.push_str("#link(\"");
            out.push_str(&link.url);
            out.push_str("\")[");
            out.push_str(&inlines_to_typst(node)?);
            out.push(']');
        }
        _ => out.push_str(&inlines_to_typst(node)?),
    }
    Ok(())
}

/// Escape chars that are markup in Typst so markdown text renders literally.
fn escape_typst(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for ch in text.chars() {
        if matches!(ch, '\\' | '#' | '$' | '[' | ']' | '*' | '_' | '@' | '<' | '>') {
            out.push('\\');
        }
        out.push(ch);
    }
    out
}

/// Convert LaTeX math to Typst math.
pub fn latex_to_typst_math(latex: &str) -> Result<String, String> {
    mitex::convert_math(latex, None)
        .map_err(|e| format!("math conversion failed for `{latex}`: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn conv(md: &str) -> String {
        md_to_typst(md).unwrap().trim_end().to_string()
    }

    #[test]
    fn display_math_block() {
        let md = "$$\ny_n = x_n - \\alpha^2\\, x_{n-2}\n$$";
        let typst = conv(md);
        let compact: String = typst.chars().filter(|c| !c.is_whitespace()).collect();
        assert!(compact.contains("alpha"), "should convert \\alpha, got:\n{typst}");
        assert!(compact.contains("_(n-2)"), "should convert subscript, got:\n{typst}");
        assert!(!typst.contains("$$"), "should not have $$, got:\n{typst}");
        assert!(!typst.contains("\\alpha"), "should strip backslash, got:\n{typst}");
    }

    #[test]
    fn inline_math() {
        let typst = conv("Given $\\alpha \\in \\mathbb{R}$.");
        let compact: String = typst.chars().filter(|c| !c.is_whitespace()).collect();
        assert!(compact.contains("$alphainbb(R)$"), "got:\n{typst}");
    }

    #[test]
    fn heading_conversion() {
        assert_eq!(conv("# Title"), "= Title");
        assert_eq!(conv("## Sub"), "== Sub");
        assert_eq!(conv("### Deep"), "=== Deep");
    }

    #[test]
    fn bold_italic() {
        assert_eq!(conv("This is **bold** text"), "This is *bold* text");
        assert_eq!(conv("This is *italic* text"), "This is _italic_ text");
    }

    #[test]
    fn markdown_escapes_become_typst_escapes() {
        assert_eq!(
            conv("\\(a\\) Some text \\[2 marks\\]"),
            "(a) Some text \\[2 marks\\]"
        );
    }

    #[test]
    fn heading_with_inline_math_converts() {
        let typst = conv("# \\(c\\) subspace of $\\mathbb{R}^3$.");
        let compact: String = typst.chars().filter(|c| !c.is_whitespace()).collect();
        assert!(compact.starts_with("=(c)"), "escapes stripped: {typst}");
        assert!(compact.contains("bb(R)"), "math converted: {typst}");
        assert!(!typst.contains("\\mathbb"), "no raw latex: {typst}");
    }

    #[test]
    fn unicode_text_passes_through_intact() {
        assert_eq!(conv("rate α – naïve ℝ case"), "rate α – naïve ℝ case");
        assert_eq!(conv("# étude ✓"), "= étude ✓");
    }

    #[test]
    fn bad_math_surfaces_error_with_source() {
        let err = md_to_typst("cost $x }$ done").unwrap_err();
        assert!(err.contains("x }"), "error names the snippet: {err}");
    }

    #[test]
    fn table_exports_as_typst_table() {
        let typst = conv("| a | b |\n|---|---|\n| $x^2$ | **y** |");
        let compact: String = typst.chars().filter(|c| !c.is_whitespace()).collect();
        assert!(compact.starts_with("#table(columns:2,"), "table call: {typst}");
        assert!(compact.contains("[a],[b],"), "header cells: {typst}");
        assert!(compact.contains("[$x^(2)$],[*y*],"), "body cells converted: {typst}");
    }

    #[test]
    fn lists_and_code_blocks() {
        let typst = conv("- one\n- two $\\pi$\n\n```julia\nx = 1\n```");
        assert!(typst.contains("- one\n- two $pi"), "bullets: {typst}");
        assert!(typst.contains("```julia\nx = 1\n```"), "code fence: {typst}");
    }

    #[test]
    fn typst_markup_in_text_is_escaped() {
        let typst = conv("cost #5 at 3*4 [note]");
        assert_eq!(typst, "cost \\#5 at 3\\*4 \\[note\\]");
    }
}
