//! Markdown/LaTeX → Typst conversion for notebook export.
//!
//! Reference: <https://typst.app/docs/guides/guide-for-latex-users>/
//!
//! Key differences from LaTeX:
//! - Display math: `$ content $` (space after opening, before closing $)
//! - Inline math: `$content$` (no spaces)
//! - Greek: bare names — `alpha`, not `\alpha`
//! - Subscripts: `_(n-2)` parens, not `_{n-2}` braces
//! - Fractions: `(a)/(b)` or `frac(a, b)`
//! - Text in math: `"otherwise"` (quoted strings)
//! - Headings: `= Title`, `== Subtitle`
//! - Bold: `*text*`, Italic: `_text_`

/// Convert a full markdown cell (with LaTeX math) to Typst.
pub fn md_to_typst(md: &str) -> Result<String, String> {
    let mut out = String::new();
    let mut in_display_math = false;

    for line in md.lines() {
        let trimmed = line.trim();

        if trimmed == "$$" {
            out.push_str("$\n");
            in_display_math = !in_display_math;
            continue;
        }

        if in_display_math {
            out.push_str("  ");
            out.push_str(&latex_to_typst_math(trimmed)?);
            out.push('\n');
            continue;
        }

        out.push_str(&md_line_to_typst(line)?);
        out.push('\n');
    }

    Ok(out)
}

fn md_line_to_typst(line: &str) -> Result<String, String> {
    let trimmed = line.trim_start();

    if let Some(rest) = trimmed.strip_prefix('#') {
        let extra = rest.bytes().take_while(|&b| b == b'#').count();
        let level = 1 + extra;
        let content = rest[extra..].trim_start();
        return Ok(format!(
            "{} {}",
            "=".repeat(level),
            inline_to_typst(content)?
        ));
    }

    if trimmed.is_empty() {
        return Ok(String::new());
    }

    inline_to_typst(line)
}

fn inline_to_typst(line: &str) -> Result<String, String> {
    let mut out = String::new();
    let bytes = line.as_bytes();
    let len = bytes.len();
    let mut i = 0;

    while i < len {
        if i + 1 < len && bytes[i] == b'$' && bytes[i + 1] == b'$' {
            i += 2;
            let start = i;
            while i + 1 < len && !(bytes[i] == b'$' && bytes[i + 1] == b'$') {
                i += 1;
            }
            out.push_str("$ ");
            out.push_str(&latex_to_typst_math(&line[start..i])?);
            out.push_str(" $");
            if i + 1 < len {
                i += 2;
            }
            continue;
        }

        if bytes[i] == b'$' {
            i += 1;
            let start = i;
            while i < len && bytes[i] != b'$' {
                i += 1;
            }
            out.push('$');
            out.push_str(&latex_to_typst_math(&line[start..i])?);
            out.push('$');
            if i < len {
                i += 1;
            }
            continue;
        }

        if bytes[i] == b'\\' && i + 1 < len && matches!(bytes[i + 1], b'(' | b')' | b'[' | b']') {
            out.push(bytes[i + 1] as char);
            i += 2;
            continue;
        }

        if i + 1 < len && bytes[i] == b'*' && bytes[i + 1] == b'*' {
            i += 2;
            let start = i;
            while i + 1 < len && !(bytes[i] == b'*' && bytes[i + 1] == b'*') {
                i += 1;
            }
            out.push('*');
            out.push_str(&line[start..i]);
            out.push('*');
            if i + 1 < len {
                i += 2;
            }
            continue;
        }

        if bytes[i] == b'*' {
            i += 1;
            let start = i;
            while i < len && bytes[i] != b'*' {
                i += 1;
            }
            out.push('_');
            out.push_str(&line[start..i]);
            out.push('_');
            if i < len {
                i += 1;
            }
            continue;
        }

        let start = i;
        i += 1;
        while i < len && !matches!(bytes[i], b'$' | b'\\' | b'*') {
            i += 1;
        }
        out.push_str(&line[start..i]);
    }

    Ok(out)
}

/// Convert LaTeX math to Typst math.
pub fn latex_to_typst_math(latex: &str) -> Result<String, String> {
    mitex::convert_math(latex, None)
        .map_err(|e| format!("math conversion failed for `{latex}`: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(s: &str) -> String {
        md_line_to_typst(s).unwrap()
    }

    #[test]
    fn display_math_block() {
        let md = "$$\ny_n = x_n - \\alpha^2\\, x_{n-2}\n$$";
        let typst = md_to_typst(md).unwrap();
        let compact: String = typst.chars().filter(|c| !c.is_whitespace()).collect();
        assert!(compact.contains("alpha"), "should convert \\alpha, got:\n{typst}");
        assert!(compact.contains("_(n-2)"), "should convert subscript, got:\n{typst}");
        assert!(!typst.contains("$$"), "should not have $$, got:\n{typst}");
        assert!(!typst.contains("\\alpha"), "should strip backslash, got:\n{typst}");
    }

    #[test]
    fn inline_math() {
        let md = "Given $\\alpha \\in \\mathbb{R}$.";
        let typst = md_to_typst(md).unwrap();
        let compact: String = typst.chars().filter(|c| !c.is_whitespace()).collect();
        assert!(compact.contains("$alphainbb(R)$"), "got:\n{typst}");
    }

    #[test]
    fn heading_conversion() {
        assert_eq!(line("# Title"), "= Title");
        assert_eq!(line("## Sub"), "== Sub");
        assert_eq!(line("### Deep"), "=== Deep");
    }

    #[test]
    fn bold_italic() {
        assert_eq!(line("This is **bold** text"), "This is *bold* text");
        assert_eq!(line("This is *italic* text"), "This is _italic_ text");
    }

    #[test]
    fn markdown_escape_parens() {
        assert_eq!(
            line("\\(a\\) Some text \\[2 marks\\]"),
            "(a) Some text [2 marks]"
        );
    }

    #[test]
    fn heading_with_inline_math_converts() {
        let result = line("# \\(c\\) subspace of $\\mathbb{R}^3$.");
        let compact: String = result.chars().filter(|c| !c.is_whitespace()).collect();
        assert!(compact.starts_with("=(c)"), "escapes stripped: {result}");
        assert!(compact.contains("bb(R)"), "math converted: {result}");
        assert!(!result.contains("\\mathbb"), "no raw latex: {result}");
    }

    #[test]
    fn unicode_text_passes_through_intact() {
        assert_eq!(line("rate α – naïve ℝ case"), "rate α – naïve ℝ case");
        assert_eq!(line("# étude ✓"), "= étude ✓");
    }

    #[test]
    fn bad_math_surfaces_error_with_source() {
        let err = md_to_typst("cost $x }$ done").unwrap_err();
        assert!(err.contains("x }"), "error names the snippet: {err}");
    }
}
