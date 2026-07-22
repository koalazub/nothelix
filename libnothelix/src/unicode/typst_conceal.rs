use super::char_offsets::CharOffsets;
use super::overlay::{CharOffsetTsv, Overlay};
use super::script::Script;
use crate::error::{Result, ffi};

static TYPST_SYMBOLS: &[(&str, &str)] = &[
    ("CC", "ℂ"),
    ("Delta", "Δ"),
    ("Gamma", "Γ"),
    ("Lambda", "Λ"),
    ("NN", "ℕ"),
    ("Omega", "Ω"),
    ("Phi", "Φ"),
    ("Pi", "Π"),
    ("Psi", "Ψ"),
    ("QQ", "ℚ"),
    ("RR", "ℝ"),
    ("Sigma", "Σ"),
    ("Theta", "Θ"),
    ("Xi", "Ξ"),
    ("ZZ", "ℤ"),
    ("alpha", "α"),
    ("approx", "≈"),
    ("arrow.l", "←"),
    ("arrow.l.double", "⇐"),
    ("arrow.r", "→"),
    ("arrow.r.double", "⇒"),
    ("beta", "β"),
    ("chi", "χ"),
    ("delta", "δ"),
    ("dots", "…"),
    ("ell", "ℓ"),
    ("epsilon", "ε"),
    ("equiv", "≡"),
    ("eta", "η"),
    ("exists", "∃"),
    ("forall", "∀"),
    ("gamma", "γ"),
    ("infinity", "∞"),
    ("inter", "∩"),
    ("iota", "ι"),
    ("kappa", "κ"),
    ("lambda", "λ"),
    ("minus.plus", "∓"),
    ("mu", "μ"),
    ("nabla", "∇"),
    ("nu", "ν"),
    ("omega", "ω"),
    ("partial", "∂"),
    ("phi", "φ"),
    ("pi", "π"),
    ("plus.minus", "±"),
    ("prop", "∝"),
    ("psi", "ψ"),
    ("rho", "ρ"),
    ("sigma", "σ"),
    ("subset", "⊂"),
    ("supset", "⊃"),
    ("tau", "τ"),
    ("theta", "θ"),
    ("times", "×"),
    ("union", "∪"),
    ("upsilon", "υ"),
    ("xi", "ξ"),
    ("zeta", "ζ"),
];

fn lookup_symbol(name: &str) -> Option<&'static str> {
    TYPST_SYMBOLS
        .binary_search_by_key(&name, |&(k, _)| k)
        .ok()
        .map(|i| TYPST_SYMBOLS[i].1)
}

fn relation_glyph(pair: (u8, u8)) -> Option<&'static str> {
    match pair {
        (b'<', b'=') => Some("≤"),
        (b'>', b'=') => Some("≥"),
        (b'!', b'=') => Some("≠"),
        (b'-', b'>') => Some("→"),
        (b'<', b'-') => Some("←"),
        (b'=', b'>') => Some("⇒"),
        _ => None,
    }
}

fn word_end(bytes: &[u8], mut i: usize) -> usize {
    while i < bytes.len() && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'.') {
        i += 1;
    }
    i
}

fn closing_dollar(bytes: &[u8], from: usize) -> Option<usize> {
    (from..bytes.len()).find(|&j| bytes[j] == b'$')
}

fn scan_typst_math(text: &str) -> Vec<Overlay> {
    let bytes = text.as_bytes();
    let mut overlays = Vec::new();
    let mut i = 0;

    while i < bytes.len() {
        if bytes[i] != b'$' {
            i += 1;
            continue;
        }

        let open = i;
        let content_start = open + 1;
        let opens_a_display_block = (open == 0 || bytes[open - 1] == b'\n')
            && matches!(bytes.get(content_start), Some(b'\n' | b' '));

        let Some(close) = closing_dollar(bytes, content_start) else {
            break;
        };

        overlays.push(Overlay::hide(open));
        overlays.push(Overlay::hide(close));
        if opens_a_display_block && bytes.get(content_start) == Some(&b'\n') {
            overlays.push(Overlay::hide(content_start));
        }

        scan_content(&text[content_start..close], content_start, &mut overlays);
        i = close + 1;
    }

    overlays
}

fn scan_content(content: &str, base: usize, overlays: &mut Vec<Overlay>) {
    let bytes = content.as_bytes();
    let mut i = 0;

    while i < bytes.len() {
        if bytes[i].is_ascii_alphabetic() {
            let end = word_end(bytes, i);
            if let Some(glyph) = lookup_symbol(&content[i..end]) {
                overlays.push(Overlay::at(base + i, glyph));
                overlays.extend((i + 1..end).map(|k| Overlay::hide(base + k)));
            }
            i = end;
            continue;
        }

        if let Some(script) = Script::marked_by(bytes[i])
            && let Some(&next) = bytes.get(i + 1)
            && next.is_ascii_digit()
            && let Some(glyph) = script.of_char(next as char)
        {
            overlays.push(Overlay::at(base + i, glyph));
            overlays.push(Overlay::hide(base + i + 1));
            i += 2;
            continue;
        }

        if let Some(&next) = bytes.get(i + 1)
            && let Some(glyph) = relation_glyph((bytes[i], next))
        {
            overlays.push(Overlay::at(base + i, glyph));
            overlays.push(Overlay::hide(base + i + 1));
            i += 2;
            continue;
        }

        i += 1;
    }
}

#[allow(clippy::needless_pass_by_value)]
pub fn typst_overlays_to_tsv(text: String) -> String {
    ffi(typst_overlay_rows(&text))
}

fn typst_overlay_rows(text: &str) -> Result<String> {
    let offsets = CharOffsets::of(text);
    let mut tsv = CharOffsetTsv::new(&offsets);
    for overlay in scan_typst_math(text) {
        tsv.push(overlay.offset, &overlay.replacement)?;
    }
    Ok(tsv.into_rows())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn glyphs(text: &str) -> Vec<String> {
        scan_typst_math(text)
            .into_iter()
            .map(|o| o.replacement.into_owned())
            .collect()
    }

    fn hidden_count(text: &str) -> usize {
        glyphs(text).iter().filter(|r| r.is_empty()).count()
    }

    #[test]
    fn typst_symbols_sorted() {
        assert!(
            TYPST_SYMBOLS.windows(2).all(|w| w[0].0 < w[1].0),
            "TYPST_SYMBOLS must be sorted by key with no duplicates"
        );
    }

    #[test]
    fn inline_math_alpha() {
        let found = glyphs("for $alpha in RR$.");
        assert!(found.iter().any(|r| r == "α"), "no α: {found:?}");
        assert!(found.iter().any(|r| r == "ℝ"), "no ℝ: {found:?}");
        assert!(hidden_count("for $alpha in RR$.") >= 2);
    }

    #[test]
    fn display_math_block() {
        let text = "defined by\n$\n  y_n = x_n - alpha^2  x_(n-2)\n$\nfor some";
        let found = glyphs(text);
        assert!(found.iter().any(|r| r == "α"), "no α: {found:?}");
        assert!(found.iter().any(|r| r == "²"), "no ²: {found:?}");
        assert!(
            !found.iter().any(|r| r == "x" || r == "n" || r == "y"),
            "single letters replaced: {found:?}"
        );
    }

    #[test]
    fn no_false_positives_in_text() {
        assert!(scan_typst_math("Consider the system defined by something.").is_empty());
    }

    #[test]
    fn operator_conceal() {
        assert!(glyphs("$x <= y$").iter().any(|r| r == "≤"));
    }

    #[test]
    fn dollar_hidden() {
        assert!(hidden_count("value $pi$ ok") >= 2);
        assert!(glyphs("value $pi$ ok").iter().any(|r| r == "π"));
    }

    #[test]
    fn tsv_rows_carry_char_offsets() {
        let rows = typst_overlays_to_tsv("é $pi$".to_string());
        let pi = rows
            .lines()
            .find(|line| line.ends_with("\tπ"))
            .expect("π row");
        assert_eq!(pi.split_once('\t').unwrap().0, "3");
    }
}
