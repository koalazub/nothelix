//! Markdown/LaTeX → Typst conversion for notebook export.
//!
//! Reference: https://typst.app/docs/guides/guide-for-latex-users/
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
pub fn md_to_typst(md: &str) -> String {
    let mut out = String::new();
    let mut in_display_math = false;

    for line in md.lines() {
        let trimmed = line.trim();

        if trimmed == "$$" {
            if in_display_math {
                out.push_str("$\n"); // close display math
            } else {
                out.push_str("$\n"); // open display math (content on next line)
            }
            in_display_math = !in_display_math;
            continue;
        }

        if in_display_math {
            out.push_str("  ");
            out.push_str(&latex_to_typst_math(line.trim()));
            out.push('\n');
            continue;
        }

        out.push_str(&md_line_to_typst(line));
        out.push('\n');
    }

    out
}

/// Convert a markdown text line (not math) to Typst markup.
fn md_line_to_typst(line: &str) -> String {
    let trimmed = line.trim_start();

    // Headings: # → =
    if let Some(rest) = trimmed.strip_prefix('#') {
        let extra = rest.bytes().take_while(|&b| b == b'#').count();
        let level = 1 + extra;
        let content = rest[extra..].trim_start();
        // Strip markdown escapes from heading content
        let content = content
            .replace("\\[", "[")
            .replace("\\]", "]")
            .replace("\\(", "(")
            .replace("\\)", ")");
        return format!("{} {content}", "=".repeat(level));
    }

    if trimmed.is_empty() {
        return String::new();
    }

    let mut out = String::new();
    let bytes = line.as_bytes();
    let len = bytes.len();
    let mut i = 0;

    while i < len {
        // Single-line $$...$$ display math
        if i + 1 < len && bytes[i] == b'$' && bytes[i + 1] == b'$' {
            i += 2;
            let start = i;
            while i + 1 < len && !(bytes[i] == b'$' && bytes[i + 1] == b'$') {
                i += 1;
            }
            out.push_str("$ ");
            out.push_str(&latex_to_typst_math(&line[start..i]));
            out.push_str(" $");
            if i + 1 < len { i += 2; }
            continue;
        }

        // Inline $...$
        if bytes[i] == b'$' {
            i += 1;
            let start = i;
            while i < len && bytes[i] != b'$' {
                i += 1;
            }
            out.push('$');
            out.push_str(&latex_to_typst_math(&line[start..i]));
            out.push('$');
            if i < len { i += 1; }
            continue;
        }

        // Markdown escapes: \( \) \[ \]
        if bytes[i] == b'\\' && i + 1 < len && matches!(bytes[i + 1], b'(' | b')' | b'[' | b']') {
            out.push(bytes[i + 1] as char);
            i += 2;
            continue;
        }

        // **bold** → *bold*
        if i + 1 < len && bytes[i] == b'*' && bytes[i + 1] == b'*' {
            i += 2;
            let start = i;
            while i + 1 < len && !(bytes[i] == b'*' && bytes[i + 1] == b'*') {
                i += 1;
            }
            out.push('*');
            out.push_str(&line[start..i]);
            out.push('*');
            if i + 1 < len { i += 2; }
            continue;
        }

        // *italic* → _italic_
        if bytes[i] == b'*' {
            i += 1;
            let start = i;
            while i < len && bytes[i] != b'*' {
                i += 1;
            }
            out.push('_');
            out.push_str(&line[start..i]);
            out.push('_');
            if i < len { i += 1; }
            continue;
        }

        out.push(bytes[i] as char);
        i += 1;
    }

    out
}

/// Convert LaTeX math to Typst math.
pub fn latex_to_typst_math(latex: &str) -> String {
    let mut s = latex.to_string();

    // ── Environments (before brace mangling) ──

    // \begin{cases}...\end{cases} → cases(...)
    while let Some(start) = s.find("\\begin{cases}") {
        if let Some(end) = s.find("\\end{cases}") {
            let inner = s[start + 13..end].to_string();
            let rows: Vec<String> = inner
                .split("\\\\")
                .map(|row| {
                    let parts: Vec<&str> = row.splitn(2, '&').collect();
                    if parts.len() == 2 {
                        format!("{} \"{}\"", parts[0].trim(), parts[1].trim())
                    } else {
                        row.trim().to_string()
                    }
                })
                .filter(|r| !r.trim().is_empty())
                .collect();
            s = format!("{}cases(\n  {}\n){}", &s[..start], rows.join(",\n  "), &s[end + 11..]);
        } else {
            break;
        }
    }

    // Matrices
    for env in &["pmatrix", "bmatrix", "vmatrix", "matrix"] {
        let open = format!("\\begin{{{env}}}");
        let close = format!("\\end{{{env}}}");
        while let Some(start) = s.find(&open) {
            if let Some(end) = s.find(&close) {
                let inner = s[start + open.len()..end].to_string();
                let rows: Vec<String> = inner
                    .split("\\\\")
                    .map(|row| row.split('&').map(|c| c.trim()).collect::<Vec<_>>().join(", "))
                    .filter(|r| !r.trim().is_empty())
                    .collect();
                s = format!("{}mat({}){}", &s[..start], rows.join("; "), &s[end + close.len()..]);
            } else {
                break;
            }
        }
    }

    // ── Structured commands (need brace parsing) ──

    // \frac{a}{b} → (a)/(b)
    while let Some(start) = s.find("\\frac{") {
        let after = &s[start + 6..];
        if let Some(num_end) = matching_brace(after) {
            let num = after[..num_end].to_string();
            let rest = &after[num_end + 1..];
            if let Some(brace_rest) = rest.strip_prefix('{') {
                if let Some(den_end) = matching_brace(brace_rest) {
                    let den = brace_rest[..den_end].to_string();
                    let total = start + 6 + num_end + 1 + 1 + den_end + 1;
                    s = format!("{}({num})/({den}){}", &s[..start], &s[total..]);
                    continue;
                }
            }
        }
        break;
    }

    // \sqrt{x} → sqrt(x), \sqrt[n]{x} → root(n, x)
    while let Some(start) = s.find("\\sqrt") {
        let after = &s[start + 5..];
        if after.starts_with('[') {
            if let Some(bracket_end) = after.find(']') {
                let n = &after[1..bracket_end];
                let rest = &after[bracket_end + 1..];
                if let Some(brace_rest) = rest.strip_prefix('{') {
                    if let Some(end) = matching_brace(brace_rest) {
                        let x = &brace_rest[..end];
                        let total = start + 5 + bracket_end + 1 + 1 + end + 1;
                        s = format!("{}root({n}, {x}){}", &s[..start], &s[total..]);
                        continue;
                    }
                }
            }
        } else if let Some(brace_after) = after.strip_prefix('{') {
            if let Some(end) = matching_brace(brace_after) {
                let x = &brace_after[..end];
                let total = start + 5 + 1 + end + 1;
                s = format!("{}sqrt({x}){}", &s[..start], &s[total..]);
                continue;
            }
        }
        break;
    }

    // \text{...} → "..."
    while let Some(start) = s.find("\\text{") {
        let after = &s[start + 6..];
        if let Some(end) = matching_brace(after) {
            let text = &after[..end];
            let total = start + 6 + end + 1;
            s = format!("{}\"{text}\"{}", &s[..start], &s[total..]);
        } else {
            break;
        }
    }

    // \operatorname{DFT} → upright("DFT") (Typst upright text in math)
    while let Some(start) = s.find("\\operatorname{") {
        let after = &s[start + 14..];
        if let Some(end) = matching_brace(after) {
            let name = &after[..end];
            let total = start + 14 + end + 1;
            s = format!("{}upright(\"{name}\"){}", &s[..start], &s[total..]);
        } else {
            break;
        }
    }

    // \mathrm{...} → upright(...)
    while let Some(start) = s.find("\\mathrm{") {
        let after = &s[start + 8..];
        if let Some(end) = matching_brace(after) {
            let inner = &after[..end];
            let total = start + 8 + end + 1;
            s = format!("{}upright({inner}){}", &s[..start], &s[total..]);
        } else {
            break;
        }
    }

    // \mathbf{v} → bold(v), \mathbb{R} → RR, \mathcal{F} → cal(F)
    for (cmd, func) in &[
        ("mathbf", "bold"), ("textbf", "bold"), ("boldsymbol", "bold"),
        ("mathcal", "cal"), ("cal", "cal"),
        ("mathfrak", "frak"), ("frak", "frak"),
        ("mathbb", "bb"),
    ] {
        let pat = format!("\\{cmd}{{");
        while let Some(start) = s.find(&pat) {
            let after = &s[start + pat.len()..];
            if let Some(end) = matching_brace(after) {
                let inner = &after[..end];
                let replacement = if *func == "bb" && inner.len() == 1 {
                    format!("{inner}{inner}") // \mathbb{R} → RR
                } else {
                    format!("{func}({inner})")
                };
                let total = start + pat.len() + end + 1;
                s = format!("{}{replacement}{}", &s[..start], &s[total..]);
            } else {
                break;
            }
        }
    }

    // ── Simple replacements ──

    s = s.replace("\\left", "");
    s = s.replace("\\right", "");
    s = s.replace("\\|", "||");

    // Spacing
    s = s.replace("\\,", " ");
    s = s.replace("\\;", " ");
    s = s.replace("\\!", "");
    s = s.replace("\\quad", "quad");
    s = s.replace("\\qquad", "wide");

    // Greek letters: \alpha → alpha (strip backslash, but only whole words)
    // Insert a space before the replacement if preceded by an alphanumeric char,
    // so j\omega → j omega, not jomega.
    for name in GREEK_AND_SYMBOLS {
        let from = format!("\\{name}");
        let mut pos = 0;
        while let Some(idx) = s[pos..].find(&from) {
            let abs = pos + idx;
            let after = abs + from.len();
            if s.as_bytes().get(after).is_none_or(|b| !b.is_ascii_alphabetic()) {
                let needs_space = abs > 0
                    && s.as_bytes()[abs - 1].is_ascii_alphanumeric();
                let prefix = if needs_space { " " } else { "" };
                s = format!("{}{prefix}{name}{}", &s[..abs], &s[after..]);
                pos = abs + prefix.len() + name.len();
            } else {
                pos = after;
            }
        }
    }

    // Operators
    s = s.replace("\\leq", "<=");
    s = s.replace("\\geq", ">=");
    s = s.replace("\\neq", "!=");
    s = s.replace("\\le", "<=");
    s = s.replace("\\ge", ">=");
    s = s.replace("\\in", "in");
    s = s.replace("\\notin", "in.not");
    s = s.replace("\\subset", "subset");
    s = s.replace("\\supset", "supset");
    s = s.replace("\\cup", "union");
    s = s.replace("\\cap", "inter");
    s = s.replace("\\to", "->");
    s = s.replace("\\rightarrow", "->");
    s = s.replace("\\leftarrow", "<-");
    s = s.replace("\\Rightarrow", "=>");
    s = s.replace("\\ldots", "dots");
    s = s.replace("\\cdots", "dots.c");
    s = s.replace("\\dots", "dots");
    s = s.replace("\\pm", "plus.minus");
    s = s.replace("\\mp", "minus.plus");

    // Subscripts/superscripts: _{...} → _(...), ^{...} → ^(...)
    s = s.replace("_{", "_(");
    s = s.replace("^{", "^(");
    s = s.replace('}', ")");

    s
}

static GREEK_AND_SYMBOLS: &[&str] = &[
    // Greek lowercase
    "alpha", "beta", "gamma", "delta", "epsilon", "varepsilon", "zeta", "eta", "theta",
    "vartheta", "iota", "kappa", "lambda", "mu", "nu", "xi", "pi", "rho",
    "sigma", "tau", "upsilon", "phi", "varphi", "chi", "psi", "omega",
    // Greek uppercase
    "Gamma", "Delta", "Theta", "Lambda", "Xi", "Pi", "Sigma", "Upsilon",
    "Phi", "Psi", "Omega",
    // Math operators (LaTeX \cos → Typst cos, etc.)
    "cos", "sin", "tan", "cot", "sec", "csc",
    "arccos", "arcsin", "arctan",
    "cosh", "sinh", "tanh",
    "exp", "log", "ln", "lg",
    "lim", "liminf", "limsup",
    "max", "min", "sup", "inf",
    "det", "dim", "ker", "arg",
    "deg", "gcd", "hom", "mod",
    // Signal processing / engineering
    "sinc", "rect", "sgn", "sign", "diag",
    "Re", "Im", "conj", "tr", "rank",
    "var", "cov", "corr",
    // Symbols
    "infty", "infinity", "partial", "nabla", "forall", "exists", "ell",
    "cdot", "times", "approx", "equiv", "sim", "propto",
    "sum", "prod", "int", "iint", "iiint", "oint",
    "star", "ast", "circ", "oplus", "otimes",
];

/// Find the matching `}` for content starting after the opening `{`.
fn matching_brace(s: &str) -> Option<usize> {
    let mut depth = 1;
    for (i, b) in s.bytes().enumerate() {
        match b {
            b'{' => depth += 1,
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(i);
                }
            }
            _ => {}
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_math_block() {
        let md = "$$\ny_n = x_n - \\alpha^2\\, x_{n-2}\n$$";
        let typst = md_to_typst(md);
        assert!(typst.contains("alpha"), "should convert \\alpha, got:\n{typst}");
        assert!(typst.contains("_(n-2)"), "should convert subscript, got:\n{typst}");
        assert!(!typst.contains("$$"), "should not have $$, got:\n{typst}");
        assert!(!typst.contains("\\alpha"), "should strip backslash, got:\n{typst}");
    }

    #[test]
    fn inline_math() {
        let md = "Given $\\alpha \\in \\mathbb{R}$.";
        let typst = md_to_typst(md);
        assert!(typst.contains("$alpha in RR$"), "got:\n{typst}");
    }

    #[test]
    fn heading_conversion() {
        assert_eq!(md_line_to_typst("# Title"), "= Title");
        assert_eq!(md_line_to_typst("## Sub"), "== Sub");
        assert_eq!(md_line_to_typst("### Deep"), "=== Deep");
    }

    #[test]
    fn bold_italic() {
        assert_eq!(md_line_to_typst("This is **bold** text"), "This is *bold* text");
        assert_eq!(md_line_to_typst("This is *italic* text"), "This is _italic_ text");
    }

    #[test]
    fn frac_conversion() {
        let result = latex_to_typst_math("\\frac{a}{b}");
        assert_eq!(result, "(a)/(b)");
    }

    #[test]
    fn cases_conversion() {
        let latex = "h_n = \\begin{cases} 1 & 0 \\leq n \\leq 2 \\\\ 0 & \\text{otherwise} \\end{cases}";
        let result = latex_to_typst_math(latex);
        assert!(result.contains("cases("), "got:\n{result}");
        assert!(result.contains("\"otherwise\""), "got:\n{result}");
        assert!(result.contains("<="), "got:\n{result}");
    }

    #[test]
    fn markdown_escape_parens() {
        let result = md_line_to_typst("\\(a\\) Some text \\[2 marks\\]");
        assert_eq!(result, "(a) Some text [2 marks]");
    }

    #[test]
    fn norm_and_blackboard() {
        let result = latex_to_typst_math("\\|x\\| \\in \\mathbb{R}");
        assert!(result.contains("||x||"), "got:\n{result}");
        assert!(result.contains("RR"), "got:\n{result}");
    }

    #[test]
    fn sqrt_conversion() {
        assert_eq!(latex_to_typst_math("\\sqrt{x}"), "sqrt(x)");
        assert_eq!(latex_to_typst_math("\\sqrt[3]{x}"), "root(3, x)");
    }
}
