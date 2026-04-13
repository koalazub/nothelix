//! Typst math → Unicode concealment.
//!
//! Scans Typst `$...$` math regions and replaces known symbol names
//! (pi → π, alpha → α, etc.) with their Unicode equivalents.
//!
//! Only replaces names that are in the curated symbol table — single
//! letter variables like `x`, `n`, `t` are left as-is.

use super::overlay::Overlay;

/// Common Typst math symbol names → Unicode replacements.
/// Only names that have a distinct visual symbol — NOT single letters.
static TYPST_SYMBOLS: &[(&str, &str)] = &[
    // Greek lowercase
    ("alpha", "α"), ("beta", "β"), ("gamma", "γ"), ("delta", "δ"),
    ("epsilon", "ε"), ("zeta", "ζ"), ("eta", "η"), ("theta", "θ"),
    ("iota", "ι"), ("kappa", "κ"), ("lambda", "λ"), ("mu", "μ"),
    ("nu", "ν"), ("xi", "ξ"), ("pi", "π"), ("rho", "ρ"),
    ("sigma", "σ"), ("tau", "τ"), ("upsilon", "υ"), ("phi", "φ"),
    ("chi", "χ"), ("psi", "ψ"), ("omega", "ω"),
    // Greek uppercase
    ("Gamma", "Γ"), ("Delta", "Δ"), ("Theta", "Θ"), ("Lambda", "Λ"),
    ("Xi", "Ξ"), ("Pi", "Π"), ("Sigma", "Σ"), ("Phi", "Φ"),
    ("Psi", "Ψ"), ("Omega", "Ω"),
    // Operators / relations
    ("infinity", "∞"), ("partial", "∂"), ("nabla", "∇"),
    ("forall", "∀"), ("exists", "∃"), ("ell", "ℓ"),
    ("times", "×"), ("dots", "…"),
    ("approx", "≈"), ("equiv", "≡"), ("prop", "∝"),
    ("plus.minus", "±"), ("minus.plus", "∓"),
    ("subset", "⊂"), ("supset", "⊃"), ("union", "∪"), ("inter", "∩"),
    // Blackboard bold shorthand
    ("NN", "ℕ"), ("ZZ", "ℤ"), ("QQ", "ℚ"), ("RR", "ℝ"), ("CC", "ℂ"),
    // Arrows
    ("arrow.r", "→"), ("arrow.l", "←"),
    ("arrow.r.double", "⇒"), ("arrow.l.double", "⇐"),
];

/// Look up a Typst symbol name. Returns None for unknown names.
/// This is the ONLY lookup — no fallback to the Julia table, which
/// would match single letters and cause false positives.
fn lookup_symbol(name: &str) -> Option<&'static str> {
    TYPST_SYMBOLS
        .iter()
        .find(|&&(k, _)| k == name)
        .map(|&(_, v)| v)
}

/// Scan a Typst document and produce concealment overlays.
pub fn scan_typst_math(text: &str) -> Vec<Overlay> {
    let mut overlays = Vec::new();
    let bytes = text.as_bytes();
    let len = bytes.len();
    let mut i = 0;

    while i < len {
        if bytes[i] != b'$' {
            i += 1;
            continue;
        }

        // Found a $. Determine if it's display or inline.
        let dollar_start = i;
        i += 1;

        // Display math: $ on its own line (preceded by newline or start of file,
        // followed by newline or space)
        let is_display = (dollar_start == 0 || bytes[dollar_start - 1] == b'\n')
            && i < len
            && (bytes[i] == b'\n' || bytes[i] == b' ');

        // Find closing $
        let content_start = i;
        let closing = loop {
            if i >= len {
                break None;
            }
            if bytes[i] == b'$' {
                break Some(i);
            }
            i += 1;
        };

        let close = match closing {
            Some(c) => c,
            None => break,
        };

        // Hide the opening $
        overlays.push(Overlay::hide(dollar_start));
        // Hide the closing $
        overlays.push(Overlay::hide(close));

        // For display math, also hide the newline/space after opening $
        // and any whitespace before closing $
        if is_display && content_start < len && bytes[content_start] == b'\n' {
            overlays.push(Overlay::hide(content_start));
        }

        // Scan content for symbol replacements
        scan_content(&text[content_start..close], content_start, &mut overlays);

        i = close + 1;
    }

    overlays
}

/// Scan math content for known symbol names and sub/superscript digits.
fn scan_content(content: &str, base: usize, overlays: &mut Vec<Overlay>) {
    let bytes = content.as_bytes();
    let len = bytes.len();
    let mut i = 0;

    while i < len {
        // Words: alphabetic + dots (for "plus.minus" etc.)
        if bytes[i].is_ascii_alphabetic() {
            let start = i;
            while i < len && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'.') {
                i += 1;
            }
            let word = &content[start..i];
            if let Some(repl) = lookup_symbol(word) {
                overlays.push(Overlay::at(base + start, repl));
                for k in (start + 1)..i {
                    overlays.push(Overlay::hide(base + k));
                }
            }
            continue;
        }

        // ^digit → superscript
        if bytes[i] == b'^' && i + 1 < len && bytes[i + 1].is_ascii_digit() {
            if let Some(sup) = super_digit(bytes[i + 1]) {
                overlays.push(Overlay::at(base + i, sup));
                overlays.push(Overlay::hide(base + i + 1));
                i += 2;
                continue;
            }
        }

        // _digit → subscript
        if bytes[i] == b'_' && i + 1 < len && bytes[i + 1].is_ascii_digit() {
            if let Some(sub) = sub_digit(bytes[i + 1]) {
                overlays.push(Overlay::at(base + i, sub));
                overlays.push(Overlay::hide(base + i + 1));
                i += 2;
                continue;
            }
        }

        // <= → ≤, >= → ≥, != → ≠
        if i + 1 < len {
            let pair = (bytes[i], bytes[i + 1]);
            let repl = match pair {
                (b'<', b'=') => Some("≤"),
                (b'>', b'=') => Some("≥"),
                (b'!', b'=') => Some("≠"),
                (b'-', b'>') => Some("→"),
                (b'<', b'-') => Some("←"),
                (b'=', b'>') => Some("⇒"),
                _ => None,
            };
            if let Some(r) = repl {
                overlays.push(Overlay::at(base + i, r));
                overlays.push(Overlay::hide(base + i + 1));
                i += 2;
                continue;
            }
        }

        i += 1;
    }
}

fn super_digit(b: u8) -> Option<&'static str> {
    match b {
        b'0' => Some("⁰"), b'1' => Some("¹"), b'2' => Some("²"), b'3' => Some("³"),
        b'4' => Some("⁴"), b'5' => Some("⁵"), b'6' => Some("⁶"), b'7' => Some("⁷"),
        b'8' => Some("⁸"), b'9' => Some("⁹"), _ => None,
    }
}

fn sub_digit(b: u8) -> Option<&'static str> {
    match b {
        b'0' => Some("₀"), b'1' => Some("₁"), b'2' => Some("₂"), b'3' => Some("₃"),
        b'4' => Some("₄"), b'5' => Some("₅"), b'6' => Some("₆"), b'7' => Some("₇"),
        b'8' => Some("₈"), b'9' => Some("₉"), _ => None,
    }
}

/// Convert overlays to tab-separated format for the Steel plugin.
pub fn typst_overlays_to_tsv(text: String) -> String {
    let byte_to_char = super::conceal::build_byte_to_char_map(&text);
    let doc_char_len = text.chars().count();
    let overlays = scan_typst_math(&text);
    let mut out = String::new();

    for overlay in &overlays {
        let char_offset = byte_to_char
            .get(overlay.offset)
            .copied()
            .unwrap_or_else(|| text[..overlay.offset.min(text.len())].chars().count());
        if char_offset < doc_char_len {
            out.push_str(&char_offset.to_string());
            out.push('\t');
            out.push_str(&overlay.replacement);
            out.push('\n');
        }
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inline_math_alpha() {
        let text = "for $alpha in RR$.";
        let overlays = scan_typst_math(text);
        assert!(overlays.iter().any(|o| o.replacement == "α"), "no α: {overlays:?}");
        assert!(overlays.iter().any(|o| o.replacement == "ℝ"), "no ℝ: {overlays:?}");
        // $ delimiters hidden
        let hidden: Vec<_> = overlays.iter().filter(|o| o.replacement.is_empty()).collect();
        assert!(hidden.len() >= 2, "$ not hidden: {overlays:?}");
    }

    #[test]
    fn display_math_block() {
        let text = "defined by\n$\n  y_n = x_n - alpha^2  x_(n-2)\n$\nfor some";
        let overlays = scan_typst_math(text);
        assert!(overlays.iter().any(|o| o.replacement == "α"), "no α: {overlays:?}");
        assert!(overlays.iter().any(|o| o.replacement == "²"), "no ²: {overlays:?}");
        // Single-letter vars x, y, n should NOT be replaced
        assert!(!overlays.iter().any(|o| o.replacement == "x" || o.replacement == "n" || o.replacement == "y"),
            "single letters replaced: {overlays:?}");
    }

    #[test]
    fn no_false_positives_in_text() {
        let text = "Consider the system defined by something.";
        let overlays = scan_typst_math(text);
        assert!(overlays.is_empty(), "text outside $ got overlays: {overlays:?}");
    }

    #[test]
    fn operator_conceal() {
        let text = "$x <= y$";
        let overlays = scan_typst_math(text);
        assert!(overlays.iter().any(|o| o.replacement == "≤"), "no ≤: {overlays:?}");
    }

    #[test]
    fn dollar_hidden() {
        let text = "value $pi$ ok";
        let overlays = scan_typst_math(text);
        let hidden: Vec<_> = overlays.iter().filter(|o| o.replacement.is_empty()).collect();
        assert!(hidden.len() >= 2, "$ delimiters not hidden: {overlays:?}");
        assert!(overlays.iter().any(|o| o.replacement == "π"), "no π: {overlays:?}");
    }
}
