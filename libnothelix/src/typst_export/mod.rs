mod block;
mod inline;
mod math;

use comrak::{Arena, Options, parse_document};

use crate::error::Result;

#[cfg(feature = "render")]
pub use math::latex_to_typst_math;

fn markdown_extensions() -> Options<'static> {
    let mut options = Options::default();
    options.extension.table = true;
    options.extension.math_dollars = true;
    options.extension.strikethrough = true;
    options
}

pub fn md_to_typst(md: &str) -> Result<String> {
    let arena = Arena::new();
    let root = parse_document(&arena, md, &markdown_extensions());
    let mut out = String::new();
    for node in root.children() {
        block::block_to_typst(node, &mut out)?;
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn conv(md: &str) -> String {
        md_to_typst(md).expect("converts").trim_end().to_string()
    }

    #[test]
    fn display_math_block() {
        let md = "$$\ny_n = x_n - \\alpha^2\\, x_{n-2}\n$$";
        let typst = conv(md);
        let compact: String = typst.chars().filter(|c| !c.is_whitespace()).collect();
        assert!(
            compact.contains("alpha"),
            "should convert \\alpha, got:\n{typst}"
        );
        assert!(
            compact.contains("_(n-2)"),
            "should convert subscript, got:\n{typst}"
        );
        assert!(!typst.contains("$$"), "should not have $$, got:\n{typst}");
        assert!(
            !typst.contains("\\alpha"),
            "should strip backslash, got:\n{typst}"
        );
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
        let err = md_to_typst("cost $x }$ done")
            .expect_err("unconvertible math must fail")
            .to_string();
        assert!(err.contains("x }"), "error names the snippet: {err}");
    }

    #[test]
    fn table_exports_as_typst_table() {
        let typst = conv("| a | b |\n|---|---|\n| $x^2$ | **y** |");
        let compact: String = typst.chars().filter(|c| !c.is_whitespace()).collect();
        assert!(
            compact.starts_with("#table(columns:2,"),
            "table call: {typst}"
        );
        assert!(compact.contains("[a],[b],"), "header cells: {typst}");
        assert!(
            compact.contains("[$x^(2)$],[*y*],"),
            "body cells converted: {typst}"
        );
    }

    #[test]
    fn lists_and_code_blocks() {
        let typst = conv("- one\n- two $\\pi$\n\n```julia\nx = 1\n```");
        assert!(typst.contains("- one\n- two $pi"), "bullets: {typst}");
        assert!(
            typst.contains("```julia\nx = 1\n```"),
            "code fence: {typst}"
        );
    }

    #[test]
    fn typst_markup_in_text_is_escaped() {
        let typst = conv("cost #5 at 3*4 [note]");
        assert_eq!(typst, "cost \\#5 at 3\\*4 \\[note\\]");
    }
}
