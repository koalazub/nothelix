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

use std::collections::HashSet;
use std::sync::OnceLock;

use crate::parse::matching_brace;

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
            if i + 1 < len {
                i += 2;
            }
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
            if i < len {
                i += 1;
            }
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
            if i + 1 < len {
                i += 2;
            }
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
            if i < len {
                i += 1;
            }
            continue;
        }

        out.push(bytes[i] as char);
        i += 1;
    }

    out
}

/// Convert LaTeX math to Typst math.
///
/// Phase order matters.  We perform the simplest, boundary-sensitive
/// command substitutions first so that adjacent commands like
/// `\sigma\sqrt{x}` do not merge into a single unrecognized token.
/// Structured commands and environments come next, and finally braces
/// are rewritten for Typst's subscript/superscript syntax.
pub fn latex_to_typst_math(latex: &str) -> String {
    let mut s = latex.to_string();

    // ── Phase 1: simple command → Typst symbol substitutions ──
    // These run before structured commands so adjacent simple commands stay
    // separate tokens (`\sigma\sqrt` → `sigma sqrt`, not `\sigmasqrt`).
    s = apply_simple_math_replacements(&s);

    // \text{...} → "..." early so cases/matrices can reason about strings.
    s = replace_all_braced(&s, "\\text{", |text| format!("\"{}\"", text.escape_default()));

    // ── Phase 2: environments ──

    // \begin{cases}...\end{cases} → cases(...)
    while let Some(start) = s.find("\\begin{cases}") {
        if let Some(end) = s.find("\\end{cases}") {
            let inner = s[start + 13..end].to_string();
            let rows: Vec<String> = inner
                .split("\\\\")
                .map(|row| {
                    let parts: Vec<&str> = row.splitn(2, '&').collect();
                    if parts.len() == 2 {
                        let cond = parts[1].trim();
                        if cond.starts_with('"') && cond.ends_with('"') && cond.len() >= 2 {
                            format!("{} {}", parts[0].trim(), cond)
                        } else {
                            format!("{} \"{}\"", parts[0].trim(), cond)
                        }
                    } else {
                        row.trim().to_string()
                    }
                })
                .filter(|r| !r.trim().is_empty())
                .collect();
            s = format!(
                "{}cases(\n  {}\n){}",
                &s[..start],
                rows.join(",\n  "),
                &s[end + 11..]
            );
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
                    .map(|row| row.split('&').map(str::trim).collect::<Vec<_>>().join(", "))
                    .filter(|r| !r.trim().is_empty())
                    .collect();
                s = format!(
                    "{}mat({}){}",
                    &s[..start],
                    rows.join("; "),
                    &s[end + close.len()..]
                );
            } else {
                break;
            }
        }
    }

    // Aligned / gather environments: Typst math already supports & and \ line breaks.
    for env in &["aligned", "align*", "align", "gather*", "gather"] {
        let open = format!("\\begin{{{env}}}");
        let close = format!("\\end{{{env}}}");
        while let Some(start) = s.find(&open) {
            if let Some(end) = s.find(&close) {
                let inner = s[start + open.len()..end].to_string();
                let body = inner.replace("\\\\", "\\");
                s = format!("{}{}{}", &s[..start], body, &s[end + close.len()..]);
            } else {
                break;
            }
        }
    }

    // ── Phase 3: structured commands (need brace parsing) ──

    s = s.replace("\\dfrac{", "\\frac{");
    s = s.replace("\\tfrac{", "\\frac{");

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

    // \binom{n}{k} → binom(n, k)
    while let Some(start) = s.find("\\binom{") {
        let after = &s[start + 7..];
        if let Some(n_end) = matching_brace(after) {
            let n = after[..n_end].to_string();
            let rest = &after[n_end + 1..];
            if let Some(brace_rest) = rest.strip_prefix('{') {
                if let Some(k_end) = matching_brace(brace_rest) {
                    let k = brace_rest[..k_end].to_string();
                    let total = start + 7 + n_end + 1 + 1 + k_end + 1;
                    s = format!("{}binom({n}, {k}){}", &s[..start], &s[total..]);
                    continue;
                }
            }
        }
        break;
    }

    // Math accents: \hat{x} → hat(x), \tilde{x} → tilde(x), etc.
    // Multi-letter arguments are quoted so Typst treats them as text.
    for (cmd, func) in &[
        ("hat", "hat"),
        ("widehat", "hat"),
        ("tilde", "tilde"),
        ("widetilde", "tilde"),
        ("bar", "macron"),
        ("overline", "overline"),
        ("underline", "underline"),
        ("vec", "arrow"),
        ("dot", "dot"),
        ("ddot", "dot.double"),
    ] {
        let pat = format!("\\{cmd}{{");
        s = replace_all_braced(&s, &pat, |inner| {
            let arg = if inner.len() > 1 && inner.chars().all(|c| c.is_alphabetic()) {
                format!("\"{inner}\"")
            } else {
                inner.to_string()
            };
            format!("{func}({arg})")
        });
    }

    // \operatorname{DFT} → op("DFT") (Typst math operator with proper spacing)
    s = replace_all_braced(&s, "\\operatorname{", |name| format!("op(\"{name}\")"));

    // \mathrm{...} → upright(...)
    s = replace_all_braced(&s, "\\mathrm{", |inner| format!("upright({inner})"));

    // \mathbf{v} → bold(v), \mathbb{R} → RR, \mathcal{F} → cal(F)
    for (cmd, func) in &[
        ("mathbf", "bold"),
        ("textbf", "bold"),
        ("boldsymbol", "bold"),
        ("bm", "bold"),
        ("mathcal", "cal"),
        ("cal", "cal"),
        ("mathfrak", "frak"),
        ("frak", "frak"),
        ("mathbb", "bb"),
        ("mathscr", "scr"),
        ("mathsf", "sans"),
        ("mathtt", "mono"),
    ] {
        let pat = format!("\\{cmd}{{");
        s = replace_all_braced(&s, &pat, |inner| {
            if *func == "bb" && inner.len() == 1 {
                format!("{inner}{inner}") // \mathbb{R} → RR
            } else {
                format!("{func}({inner})")
            }
        });
    }

    // \overset{a}{b} → attach(b, t: a), \underset{a}{b} → attach(b, b: a)
    for (cmd, dir) in [("overset", "t"), ("underset", "b"), ("stackrel", "t")] {
        let pat = format!("\\{cmd}{{");
        while let Some(start) = s.find(&pat) {
            let after = &s[start + pat.len()..];
            if let Some(a_end) = matching_brace(after) {
                let a = after[..a_end].to_string();
                let rest = &after[a_end + 1..];
                if let Some(body_rest) = rest.strip_prefix('{') {
                    if let Some(b_end) = matching_brace(body_rest) {
                        let b = body_rest[..b_end].to_string();
                        let total = start + pat.len() + a_end + 1 + 1 + b_end + 1;
                        s = format!("{}attach({}, {}: {}){}", &s[..start], b, dir, a, &s[total..]);
                        continue;
                    }
                }
            }
            break;
        }
    }

    // \overbrace{x}^{text} → overbrace(x, "text")
    for (cmd, func, marker) in [("overbrace", "overbrace", "^{"), ("underbrace", "underbrace", "_{")] {
        let pat = format!("\\{cmd}{{");
        while let Some(start) = s.find(&pat) {
            let after = &s[start + pat.len()..];
            if let Some(body_end) = matching_brace(after) {
                let body = after[..body_end].to_string();
                let rest = &after[body_end + 1..];
                let (ann, consumed) = if let Some(inner) = rest.strip_prefix(marker) {
                    if let Some(end) = matching_brace(inner) {
                        (inner[..end].to_string(), marker.len() + end + 1)
                    } else {
                        (String::new(), 0)
                    }
                } else {
                    (String::new(), 0)
                };
                let total = start + pat.len() + body_end + 1 + consumed;
                if ann.is_empty() {
                    s = format!("{}{}({}){}", &s[..start], func, body, &s[total..]);
                } else {
                    let ann_out = if ann.chars().all(|c| c.is_alphabetic() || c == ' ') {
                        format!("\"{}\"", ann)
                    } else {
                        ann
                    };
                    s = format!("{}{}({}, {}){}", &s[..start], func, body, ann_out, &s[total..]);
                }
                continue;
            }
            break;
        }
    }

    // \mathit is the math default; just strip the command.
    s = replace_all_braced(&s, "\\mathit{", |inner| inner.to_string());

    // \| ... \| norm syntax already handled by \| replacement in phase 1.
    // Ensure any remaining double-pipe from a single \| stays doubled.
    s = s.replace("\\|", "||");

    // Literal braces.
    s = s.replace("\\{", "brace.l");
    s = s.replace("\\}", "brace.r");

    // ── Phase 4: final brace/space cleanups ──

    // Spacing: in math mode literal spaces are mostly ignored, but they keep
    // adjacent rendered tokens apart after backslash commands are removed.
    s = s.replace("\\,", " ");
    s = s.replace("\\;", " ");
    s = s.replace("\\!", "");
    s = s.replace("\\quad", " ");
    s = s.replace("\\qquad", " ");
    s = s.replace("\\ ", " ");

    // Subscripts/superscripts: _{...} → _(...), ^{...} → ^(...)
    s = s.replace("_{", "_(");
    s = s.replace("^{", "^(");
    s = s.replace('}', ")");

    normalize_math_spacing(&s)
}

/// Split adjacent identifiers in math mode so Typst does not merge them into a
/// single multi-letter variable (e.g. `dx` → `d x`, `ad` → `a d`).
///
/// Known Typst symbols and function names are kept intact.
fn normalize_math_spacing(s: &str) -> String {
    #[derive(Clone, Copy, PartialEq)]
    enum Tok<'a> {
        Word(&'a str),
        Number(&'a str),
        Str(&'a str),
        Sym(char),
        Ws(&'a str),
    }

    fn tokenize(input: &str) -> Vec<Tok<'_>> {
        let mut toks = Vec::new();
        let bytes = input.as_bytes();
        let mut i = 0;
        while i < bytes.len() {
            let c = bytes[i] as char;
            if matches!(c, ' ' | '\t' | '\n' | '\r') {
                let start = i;
                i += 1;
                while i < bytes.len() && matches!(bytes[i] as char, ' ' | '\t' | '\n' | '\r') {
                    i += 1;
                }
                toks.push(Tok::Ws(&input[start..i]));
            } else if c == '"' {
                let start = i;
                i += 1;
                while i < bytes.len() && bytes[i] != b'"' {
                    i += 1;
                }
                if i < bytes.len() {
                    i += 1;
                }
                toks.push(Tok::Str(&input[start..i]));
            } else if c.is_ascii_digit() {
                let start = i;
                i += 1;
                while i < bytes.len() && (bytes[i] as char).is_ascii_digit() {
                    i += 1;
                }
                toks.push(Tok::Number(&input[start..i]));
            } else if c.is_ascii_alphabetic() {
                let start = i;
                i += 1;
                while i < bytes.len() {
                    let c2 = bytes[i] as char;
                    if c2.is_ascii_alphanumeric() || c2 == '.' || c2 == '_' {
                        i += 1;
                    } else {
                        break;
                    }
                }
                toks.push(Tok::Word(&input[start..i]));
            } else {
                toks.push(Tok::Sym(c));
                i += 1;
            }
        }
        toks
    }

    fn is_whitelisted(word: &str) -> bool {
        static WHITELIST: OnceLock<HashSet<&'static str>> = OnceLock::new();
        let set = WHITELIST.get_or_init(|| {
            let mut set = HashSet::new();
            for &v in SIMPLE_MATH_COMMANDS.values() {
                if !v.is_empty() {
                    set.insert(v);
                }
            }
            for &extra in &[
                "sqrt", "root", "binom", "hat", "tilde", "bar", "macron", "overline",
                "underline", "dot", "dot.double", "arrow", "bold", "upright", "cal",
                "frak", "bb", "op", "cases", "mat", "dif",
                // Common blackboard-bold double letters produced by \mathbb{X}.
                "RR", "CC", "ZZ", "QQ", "NN", "HH", "PP", "FF", "EE", "II", "DD", "GG",
                "KK", "LL", "OO", "SS", "TT", "UU", "VV", "WW", "XX", "YY",
            ] {
                set.insert(extra);
            }
            set
        });
        set.contains(word)
    }

    fn needs_space(prev: Option<Tok>, cur: Tok) -> bool {
        match (prev, cur) {
            (Some(Tok::Word(_)), Tok::Word(_)) => true,
            (Some(Tok::Number(_)), Tok::Word(_) | Tok::Number(_)) => true,
            (Some(Tok::Word(_)), Tok::Number(_)) => true,
            (
                Some(Tok::Sym(c)),
                Tok::Word(_) | Tok::Number(_),
            ) if c == ')' || c == ']' || c == '}' => true,
            _ => false,
        }
    }

    let toks = tokenize(s);
    let mut out = String::with_capacity(s.len() * 2);
    let mut last: Option<Tok> = None;

    for tok in toks {
        if matches!(tok, Tok::Ws(_)) {
            if last.is_some() && !matches!(last, Some(Tok::Ws(_))) {
                out.push(' ');
            }
            last = Some(tok);
            continue;
        }

        if let Tok::Word(w) = tok {
            if w.len() > 1 && !is_whitelisted(w) {
                for (idx, ch) in w.chars().enumerate() {
                    if idx == 0 {
                        if needs_space(last, Tok::Word("")) {
                            out.push(' ');
                        }
                    } else {
                        out.push(' ');
                    }
                    out.push(ch);
                    last = Some(Tok::Word(""));
                }
                continue;
            }
        }

        if needs_space(last, tok) {
            out.push(' ');
        }
        match tok {
            Tok::Word(w) | Tok::Number(w) | Tok::Str(w) | Tok::Ws(w) => out.push_str(w),
            Tok::Sym(c) => out.push(c),
        }
        last = Some(tok);
    }

    out
}

/// Replace backslash commands that map directly to Typst symbols.
///
/// The replacements preserve word boundaries: `\sin` becomes `sin`, but
/// `\sinc` is not accidentally converted while searching for `\sin`.
fn apply_simple_math_replacements(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let bytes = s.as_bytes();
    let mut i = 0;

    while i < bytes.len() {
        if bytes[i] != b'\\' {
            out.push(bytes[i] as char);
            i += 1;
            continue;
        }

        // Find the end of the command name.
        let name_start = i + 1;
        let mut name_end = name_start;
        while name_end < bytes.len() && bytes[name_end].is_ascii_alphabetic() {
            name_end += 1;
        }

        if name_start == name_end {
            // Non-alphabetic escape (e.g. \[, \|, \\).  Keep the backslash for
            // now; later phases handle the ones we care about.
            out.push('\\');
            i += 1;
            continue;
        }

        let name = &s[name_start..name_end];

        // Helpers to avoid merging adjacent tokens after a command is removed.
        let prev = out.chars().last();
        let prev_is_boundary = |c: char| c.is_alphanumeric() || c == ')' || c == ']' || c == '}';
        let needs_space = prev.is_some_and(prev_is_boundary);

        if let Some(typst) = SIMPLE_MATH_COMMANDS.get(name) {
            if needs_space {
                out.push(' ');
            }
            out.push_str(typst);
            i = name_end;
        } else {
            // Unknown command: keep the backslash for later phases, but insert
            // a space first so e.g. `x\sqrt{y}` does not become `xsqrt(y)`.
            if needs_space {
                out.push(' ');
            }
            out.push('\\');
            i += 1;
        }
    }

    out
}

/// Mapping from LaTeX math command names to Typst math identifiers/symbols.
///
/// This is intentionally exhaustive for the audit corpus.  Commands whose
/// Typst name differs from the LaTeX name are listed explicitly.
static SIMPLE_MATH_COMMANDS: phf::Map<&'static str, &'static str> = phf::phf_map! {
    // Greek lowercase
    "alpha" => "alpha",
    "beta" => "beta",
    "gamma" => "gamma",
    "delta" => "delta",
    "epsilon" => "epsilon",
    "varepsilon" => "epsilon.alt",
    "zeta" => "zeta",
    "eta" => "eta",
    "theta" => "theta",
    "vartheta" => "theta.alt",
    "iota" => "iota",
    "kappa" => "kappa",
    "varkappa" => "kappa.alt",
    "lambda" => "lambda",
    "mu" => "mu",
    "nu" => "nu",
    "xi" => "xi",
    "pi" => "pi",
    "varpi" => "pi.alt",
    "rho" => "rho",
    "varrho" => "rho.alt",
    "sigma" => "sigma",
    "varsigma" => "sigma.alt",
    "tau" => "tau",
    "upsilon" => "upsilon",
    "phi" => "phi",
    "varphi" => "phi.alt",
    "chi" => "chi",
    "psi" => "psi",
    "omega" => "omega",
    // Greek uppercase
    "Gamma" => "Gamma",
    "Delta" => "Delta",
    "Theta" => "Theta",
    "Lambda" => "Lambda",
    "Xi" => "Xi",
    "Pi" => "Pi",
    "Sigma" => "Sigma",
    "Upsilon" => "Upsilon",
    "Phi" => "Phi",
    "Psi" => "Psi",
    "Omega" => "Omega",
    // Hebrew / misc letters
    "aleph" => "alef",
    "beth" => "bet",
    "gimel" => "gimel",
    "daleth" => "dalet",
    "ell" => "ell",
    "hbar" => "planck",
    "hslash" => "planck",
    "imath" => "dotless.i",
    "jmath" => "dotless.j",
    "wp" => "wp",
    "Re" => "Re",
    "Im" => "Im",
    "mho" => "mho",
    "eth" => "eth",
    "digamma" => "digamma",
    // Operators and named functions
    "cos" => "cos",
    "sin" => "sin",
    "tan" => "tan",
    "cot" => "cot",
    "sec" => "sec",
    "csc" => "csc",
    "arccos" => "arccos",
    "arcsin" => "arcsin",
    "arctan" => "arctan",
    "cosh" => "cosh",
    "sinh" => "sinh",
    "tanh" => "tanh",
    "exp" => "exp",
    "log" => "log",
    "ln" => "ln",
    "lg" => "lg",
    "lim" => "lim",
    "liminf" => "lim.inf",
    "limsup" => "lim.sup",
    "max" => "max",
    "min" => "min",
    "sup" => "sup",
    "inf" => "inf",
    "det" => "det",
    "dim" => "dim",
    "ker" => "ker",
    "arg" => "arg",
    "deg" => "deg",
    "gcd" => "gcd",
    "hom" => "hom",
    "mod" => "mod",
    "sgn" => "sgn",
    "sign" => "sign",
    "sinc" => "sinc",
    "rect" => "rect",
    "diag" => "diag",
    "tr" => "tr",
    "rank" => "rank",
    "var" => "var",
    "cov" => "cov",
    "corr" => "corr",
    "conj" => "conj",
    // Big operators
    "sum" => "sum",
    "prod" => "product",
    "coprod" => "coproduct",
    "int" => "integral",
    "iint" => "integral.double",
    "iiint" => "integral.triple",
    "oint" => "integral.cont",
    "bigcap" => "inter.big",
    "bigcup" => "union.big",
    "bigsqcup" => "union.sq.big",
    "bigvee" => "or.big",
    "bigwedge" => "and.big",
    "bigoplus" => "plus.circle.big",
    "bigotimes" => "times.circle.big",
    "bigodot" => "dot.circle.big",
    "bigsqcap" => "inter.sq.big",
    "biguplus" => "union.plus.big",
    // Differentials (commonly used after integrals)
    "dif" => "dif",
    "d" => "d",
    // Comparison
    "leq" => "<=",
    "le" => "<=",
    "leqslant" => "<=",
    "geq" => ">=",
    "ge" => ">=",
    "geqslant" => ">=",
    "neq" => "!=",
    "ne" => "!=",
    "equiv" => "equiv",
    "sim" => "tilde.op",
    "simeq" => "eq.tilde",
    "approx" => "approx",
    "cong" => "approx.eq",
    "asymp" => "asymp",
    "propto" => "prop",
    "doteq" => "eq.dot",
    "ll" => "lt.double",
    "gg" => "gt.double",
    "lll" => "lt.triple",
    "ggg" => "gt.triple",
    "lesssim" => "lt.approx",
    "gtrsim" => "gt.approx",
    "lessgtr" => "lt.gt",
    "gtrless" => "gt.lt",
    "prec" => "prec",
    "succ" => "succ",
    "preceq" => "prec.eq",
    "succeq" => "succ.eq",
    // Set theory
    "in" => "in",
    "notin" => "in.not",
    "ni" => "in.rev",
    "notni" => "in.rev.not",
    "subset" => "subset",
    "supset" => "supset",
    "subseteq" => "subset.eq",
    "supseteq" => "supset.eq",
    "nsubseteq" => "subset.eq.not",
    "nsupseteq" => "supset.eq.not",
    "subsetneq" => "subset.neq",
    "supsetneq" => "supset.neq",
    "sqsubset" => "subset.sq",
    "sqsupset" => "supset.sq",
    "sqsubseteq" => "subset.eq.sq",
    "sqsupseteq" => "supset.eq.sq",
    "cup" => "union",
    "cap" => "inter",
    "uplus" => "union.plus",
    "sqcap" => "inter.sq",
    "sqcup" => "union.sq",
    "setminus" => "without",
    "smallsetminus" => "without",
    "mid" => "|",
    "parallel" => "parallel",
    "nparallel" => "parallel.not",
    "perp" => "perp",
    // Logic
    "forall" => "forall",
    "exists" => "exists",
    "nexists" => "exists.not",
    "land" => "and",
    "wedge" => "and",
    "lor" => "or",
    "vee" => "or",
    "lnot" => "not",
    "neg" => "not",
    "top" => "top",
    "bot" => "bot",
    "vdash" => "tack",
    "vDash" => "tack.r",
    "Vdash" => "tack.r.double",
    "models" => "models",
    // More relations / operators
    "nleq" => "lt.eq.not",
    "ngeq" => "gt.eq.not",
    "nleqq" => "lt.eq.not",
    "ngeqq" => "gt.eq.not",
    "lneq" => "lt.neq",
    "gneq" => "gt.neq",
    "lneqq" => "lt.nequiv",
    "gneqq" => "gt.nequiv",
    "lnapprox" => "lt.napprox",
    "gnapprox" => "gt.napprox",
    "nsim" => "tilde.not",
    "ncong" => "tilde.equiv.not",
    "napprox" => "approx.not",
    "nasymp" => "asymp.not",
    "nequiv" => "equiv.not",
    "nprec" => "prec.not",
    "nsucc" => "succ.not",
    "npreceq" => "prec.eq.not",
    "nsucceq" => "succ.eq.not",
    "preccurlyeq" => "prec.curly.eq",
    "succcurlyeq" => "succ.curly.eq",
    "curlyeqprec" => "prec.curly",
    "curlyeqsucc" => "succ.curly",
    "precsim" => "prec.tilde",
    "succsim" => "succ.tilde",
    "precnsim" => "prec.ntilde",
    "succnsim" => "succ.ntilde",
    "bumpeq" => "tilde.eq.rev",
    "Bumpeq" => "tilde.equiv",
    "doteqdot" => "eq.dot",
    "eqcirc" => "eq.o",
    "circeq" => "eq.o",
    "triangleq" => "eq.delta",
    "gtrapprox" => "gt.approx",
    "lessapprox" => "lt.approx",
    "gtreqless" => "gt.eq.lt",
    "lesseqgtr" => "lt.eq.gt",
    "gtreqqless" => "gt.eq.lt",
    "lesseqqgtr" => "lt.eq.gt",
    "eqslantgtr" => "gt.equiv",
    "eqslantless" => "lt.equiv",
    "because" => "because",
    "therefore" => "therefore",
    "Colon" => "colon.double",
    "coloneqq" => "colon.eq",
    "eqqcolon" => "eq.colon",
    "Join" => "join",
    "ltimes" => "times.l",
    "rtimes" => "times.r",
    "leftthreetimes" => "times.three.l",
    "rightthreetimes" => "times.three.r",
    "curlyvee" => "or.curly",
    "curlywedge" => "and.curly",
    "veebar" => "xor",
    "intercal" => "interleave",
    "dotplus" => "plus.dot",
    "Cap" => "inter.double",
    "Cup" => "union.double",
    "Subset" => "subset.double",
    "Supset" => "supset.double",
    // Calculus / analysis
    "partial" => "partial",
    "nabla" => "nabla",
    "infty" => "infinity",
    "infinity" => "infinity",
    "prime" => "prime",
    // Arrows
    "to" => "arrow.r",
    "rightarrow" => "arrow.r",
    "leftarrow" => "arrow.l",
    "Rightarrow" => "arrow.r.double",
    "Leftarrow" => "arrow.l.double",
    "implies" => "arrow.r.double",
    "iff" => "arrow.l.r.double",
    "mapsto" => "arrow.r.bar",
    "longmapsto" => "arrow.r.long.bar",
    "gets" => "arrow.l",
    "uparrow" => "arrow.t",
    "downarrow" => "arrow.b",
    "Uparrow" => "arrow.t.double",
    "Downarrow" => "arrow.b.double",
    "leftrightarrow" => "arrow.l.r",
    "Leftrightarrow" => "arrow.l.r.double",
    "longrightarrow" => "arrow.r.long",
    "longleftarrow" => "arrow.l.long",
    "Longrightarrow" => "arrow.r.long.double",
    "Longleftarrow" => "arrow.l.long.double",
    "hookrightarrow" => "arrow.r.hook",
    "hookleftarrow" => "arrow.l.hook",
    "twoheadrightarrow" => "arrow.r.twohead",
    "twoheadleftarrow" => "arrow.l.twohead",
    "rightarrowtail" => "arrow.r.tail",
    "leftarrowtail" => "arrow.l.tail",
    "mapsfrom" => "arrow.l.bar",
    "longmapsfrom" => "arrow.l.long.bar",
    "nearrow" => "arrow.tr",
    "searrow" => "arrow.br",
    "swarrow" => "arrow.bl",
    "nwarrow" => "arrow.tl",
    "leadsto" => "arrow.r.squiggly",
    "rightsquigarrow" => "arrow.r.squiggly",
    "leftsquigarrow" => "arrow.l.squiggly",
    "rightleftharpoons" => "harpoons.rtlb",
    "leftrightharpoons" => "harpoons.ltrb",
    "rightharpoonup" => "harpoon.rt",
    "rightharpoondown" => "harpoon.rb",
    "leftharpoonup" => "harpoon.lt",
    "leftharpoondown" => "harpoon.lb",
    "upuparrows" => "arrows.tt",
    "downdownarrows" => "arrows.bb",
    "rightrightarrows" => "arrows.rr",
    "leftleftarrows" => "arrows.ll",
    "nrightarrow" => "arrow.r.not",
    "nleftarrow" => "arrow.l.not",
    "nRightarrow" => "arrow.r.double.not",
    "nLeftarrow" => "arrow.l.double.not",
    "nleftrightarrow" => "arrow.l.r.not",
    "nLeftrightarrow" => "arrow.l.r.double.not",
    // Dots
    "ldots" => "dots",
    "cdots" => "dots.c",
    "vdots" => "dots.v",
    "ddots" => "dots.down",
    // Binary operators
    "pm" => "plus.minus",
    "mp" => "minus.plus",
    "times" => "times",
    "div" => "div",
    "cdot" => "dot.op",
    "ast" => "ast",
    "star" => "star",
    "bullet" => "bullet",
    "circ" => "circle.small",
    "bigcirc" => "circle.big",
    "oplus" => "plus.circle",
    "ominus" => "minus.circle",
    "otimes" => "times.circle",
    "oslash" => "slash.circle",
    "odot" => "dot.circle",
    "amalg" => "amalg",
    "wr" => "wr",
    // Delimiters
    "lceil" => "ceil.l",
    "rceil" => "ceil.r",
    "lfloor" => "floor.l",
    "rfloor" => "floor.r",
    "langle" => "lt.closed",
    "rangle" => "gt.closed",
    "lbrace" => "brace.l",
    "rbrace" => "brace.r",
    "vert" => "|",
    "Vert" => "parallel",
    // Geometry / symbols
    "angle" => "angle",
    "measuredangle" => "angle.arc",
    "sphericalangle" => "angle.spheric",
    "triangle" => "triangle.t",
    "triangledown" => "triangle.b",
    "square" => "square",
    "blacksquare" => "square.filled",
    "Diamond" => "diamond",
    "diamond" => "diamond.small",
    "lozenge" => "lozenge",
    // Empty / nothing
    "emptyset" => "nothing",
    "varnothing" => "nothing",
    // Suits / markers
    "clubsuit" => "suit.club",
    "diamondsuit" => "suit.diamond",
    "heartsuit" => "suit.heart",
    "spadesuit" => "suit.spade",
    "checkmark" => "checkmark",
    "maltese" => "maltese",
    // Special
    "backslash" => "backslash",
    "colon" => "colon",
    "dots" => "dots",
    // TeX spacing / sizing / style noise — Typst infers these, so drop them.
    // These are intentionally *after* real commands so \leftarrow etc. win.
    "limits" => "",
    "nolimits" => "",
    "displaystyle" => "",
    "textstyle" => "",
    "scriptstyle" => "",
    "scriptscriptstyle" => "",
    "big" => "",
    "Big" => "",
    "bigg" => "",
    "Bigg" => "",
    "bigl" => "",
    "bigr" => "",
    "Bigl" => "",
    "Bigr" => "",
    "biggl" => "",
    "biggr" => "",
    "Biggl" => "",
    "Biggr" => "",
    "left" => "",
    "right" => "",
    "middle" => "",
};




/// Replace every `\prefix{...}` occurrence, mapping inner content through `transform`.
fn replace_all_braced(s: &str, prefix: &str, transform: impl Fn(&str) -> String) -> String {
    let mut s = s.to_string();
    while let Some(start) = s.find(prefix) {
        let after = &s[start + prefix.len()..];
        if let Some(end) = matching_brace(after) {
            let inner = &after[..end];
            let replacement = transform(inner);
            let total = start + prefix.len() + end + 1;
            s = format!("{}{replacement}{}", &s[..start], &s[total..]);
        } else {
            break;
        }
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_math_block() {
        let md = "$$\ny_n = x_n - \\alpha^2\\, x_{n-2}\n$$";
        let typst = md_to_typst(md);
        assert!(
            typst.contains("alpha"),
            "should convert \\alpha, got:\n{typst}"
        );
        assert!(
            typst.contains("_(n-2)"),
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
        assert_eq!(
            md_line_to_typst("This is **bold** text"),
            "This is *bold* text"
        );
        assert_eq!(
            md_line_to_typst("This is *italic* text"),
            "This is _italic_ text"
        );
    }

    #[test]
    fn frac_conversion() {
        let result = latex_to_typst_math("\\frac{a}{b}");
        assert_eq!(result, "(a)/(b)");
    }

    #[test]
    fn cases_conversion() {
        let latex =
            "h_n = \\begin{cases} 1 & 0 \\leq n \\leq 2 \\\\ 0 & \\text{otherwise} \\end{cases}";
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

    #[test]
    fn accent_conversion() {
        assert_eq!(latex_to_typst_math("\\hat{x}"), "hat(x)");
        assert_eq!(latex_to_typst_math("\\widetilde{G}"), "tilde(G)");
        assert_eq!(latex_to_typst_math("\\vec{v}"), "arrow(v)");
        assert_eq!(latex_to_typst_math("\\bar{x}"), "macron(x)");
    }

    #[test]
    fn adjacent_sigma_and_sqrt() {
        assert_eq!(
            latex_to_typst_math(r"\sigma\sqrt{2\pi}"),
            "sigma sqrt(2 pi)"
        );
        assert_eq!(latex_to_typst_math(r"\sigma"), "sigma");
        assert_eq!(latex_to_typst_math(r"\sqrt{x}"), "sqrt(x)");
    }
}
