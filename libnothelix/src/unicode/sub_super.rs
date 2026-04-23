//! Sub/superscript maps + the font-command (`\mathbf` / `\mathbb` / ...) helper.
//!
//! These tables used to be inlined inside the scanner four times (once per
//! braced/inline × sub/super case). Hoisted here so every case shares a
//! single source of truth.

use super::symbol_table::SYMBOLS;

/// Characters that have a Unicode superscript form. Handles digits,
/// arithmetic operators, parentheses, and the two letters n/i which are
/// the only letters with dedicated superscript codepoints in common use.
pub(super) static SUPER_MAP: &[(&str, &str)] = &[
    ("0", "⁰"),
    ("1", "¹"),
    ("2", "²"),
    ("3", "³"),
    ("4", "⁴"),
    ("5", "⁵"),
    ("6", "⁶"),
    ("7", "⁷"),
    ("8", "⁸"),
    ("9", "⁹"),
    ("+", "⁺"),
    ("-", "⁻"),
    ("=", "⁼"),
    ("(", "⁽"),
    (")", "⁾"),
    ("n", "ⁿ"),
    ("i", "ⁱ"),
    ("T", "ᵀ"),
    ("*", "*"),
    ("a", "ᵃ"),
    ("b", "ᵇ"),
    ("c", "ᶜ"),
    ("d", "ᵈ"),
    ("e", "ᵉ"),
    ("g", "ᵍ"),
    ("h", "ʰ"),
    ("j", "ʲ"),
    ("k", "ᵏ"),
    ("l", "ˡ"),
    ("m", "ᵐ"),
    ("o", "ᵒ"),
    ("p", "ᵖ"),
    ("r", "ʳ"),
    ("s", "ˢ"),
    ("t", "ᵗ"),
    ("u", "ᵘ"),
    ("v", "ᵛ"),
    ("w", "ʷ"),
    ("x", "ˣ"),
    ("y", "ʸ"),
    ("z", "ᶻ"),
    // Uppercase superscripts — Unicode's "Modifier Letter Capital" block
    // covers most letters. Missing: C, F, Q, S, X, Y, Z (no codepoint).
    ("A", "ᴬ"),
    ("B", "ᴮ"),
    ("D", "ᴰ"),
    ("E", "ᴱ"),
    ("G", "ᴳ"),
    ("H", "ᴴ"),
    ("I", "ᴵ"),
    ("J", "ᴶ"),
    ("K", "ᴷ"),
    ("L", "ᴸ"),
    ("M", "ᴹ"),
    ("N", "ᴺ"),
    ("O", "ᴼ"),
    ("P", "ᴾ"),
    ("R", "ᴿ"),
    ("U", "ᵁ"),
    ("V", "ⱽ"),
    ("W", "ᵂ"),
];

/// Characters that have a Unicode subscript form. Broader than the
/// superscript set because more letters have dedicated subscript codepoints.
pub(super) static SUB_MAP: &[(&str, &str)] = &[
    ("0", "₀"),
    ("1", "₁"),
    ("2", "₂"),
    ("3", "₃"),
    ("4", "₄"),
    ("5", "₅"),
    ("6", "₆"),
    ("7", "₇"),
    ("8", "₈"),
    ("9", "₉"),
    ("+", "₊"),
    ("-", "₋"),
    ("=", "₌"),
    ("(", "₍"),
    (")", "₎"),
    ("n", "ₙ"),
    ("i", "ᵢ"),
    ("k", "ₖ"),
    ("j", "ⱼ"),
    ("e", "ₑ"),
    ("a", "ₐ"),
    ("o", "ₒ"),
    ("x", "ₓ"),
    ("r", "ᵣ"),
    ("u", "ᵤ"),
    ("v", "ᵥ"),
    ("s", "ₛ"),
    ("t", "ₜ"),
    ("l", "ₗ"),
    ("m", "ₘ"),
    ("p", "ₚ"),
    ("h", "ₕ"),
];

/// Look up a single character in a (char → replacement) map.
/// Returns `None` if the character has no replacement form.
#[inline]
pub(super) fn map_lookup(map: &[(&'static str, &'static str)], ch: char) -> Option<&'static str> {
    let mut buf = [0u8; 4];
    let key = ch.encode_utf8(&mut buf);
    for (k, v) in map {
        if *k == key {
            return Some(*v);
        }
    }
    None
}

/// Map a LaTeX font command + letter to a Julia symbol table name.
/// E.g. `("mathbf", "b")` → `"bfb"` → `𝐛`, `("mathbb", "R")` → `"bbR"` → `ℝ`.
pub(super) fn latex_font_to_julia(cmd: &str, letter: &str) -> Option<&'static str> {
    let prefix = match cmd {
        "mathbf" | "textbf" | "boldsymbol" => "bf",
        "mathbb" => "bb",
        "mathcal" | "cal" => "scr",
        "mathfrak" | "frak" => "frak",
        "mathit" | "textit" => "it",
        "mathsf" => "sf",
        "mathtt" => "tt",
        _ => return None,
    };
    // Build the Julia name (e.g., "bfb", "bbR") and binary-search SYMBOLS.
    let julia_name = format!("{prefix}{letter}");
    SYMBOLS
        .binary_search_by_key(&julia_name.as_str(), |&(k, _)| k)
        .ok()
        .map(|i| SYMBOLS[i].1)
}

