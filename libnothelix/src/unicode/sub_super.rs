//! Sub/superscript maps + the font-command (`\mathbf` / `\mathbb` / ...) helper.
//!
//! These tables used to be inlined inside the scanner four times (once per
//! braced/inline × sub/super case). Hoisted here so every case shares a
//! single source of truth.

use super::symbol_table::SYMBOLS;

/// Characters that have a Unicode superscript form. Handles digits,
/// arithmetic operators, parentheses, and the two letters n/i which are
/// the only letters with dedicated superscript codepoints in common use.
static SUPER_MAP: &[(&str, &str)] = &[
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
static SUB_MAP: &[(&str, &str)] = &[
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

/// An ASCII-indexed lookup table: position `b` holds the replacement for the
/// character whose byte value is `b`, or `None`. Every key in `SUPER_MAP` and
/// `SUB_MAP` is a single ASCII byte, so a 128-entry array gives O(1) lookup
/// without scanning the table per content char.
type AsciiLut = [Option<&'static str>; 128];

/// Build an ASCII LUT from a `(char_str → replacement)` table at compile time.
/// Keeps `SUPER_MAP`/`SUB_MAP` as the single source of truth — the LUT is
/// derived from them rather than duplicating the data. Each key is asserted to
/// be exactly one ASCII byte.
const fn build_lut(map: &[(&'static str, &'static str)]) -> AsciiLut {
    let mut lut: AsciiLut = [None; 128];
    let mut i = 0;
    while i < map.len() {
        let key = map[i].0.as_bytes();
        assert!(key.len() == 1, "sub/super map key must be one ASCII byte");
        lut[key[0] as usize] = Some(map[i].1);
        i += 1;
    }
    lut
}

static SUPER_LUT: AsciiLut = build_lut(SUPER_MAP);
static SUB_LUT: AsciiLut = build_lut(SUB_MAP);

/// Look up a single character in an ASCII LUT. Returns `None` if the character
/// has no replacement form. Non-ASCII chars never have a replacement, so they
/// short-circuit to `None`.
#[inline]
fn lut_lookup(lut: &AsciiLut, ch: char) -> Option<&'static str> {
    let b = ch as u32;
    if b < 128 { lut[b as usize] } else { None }
}

/// Look up a character's Unicode superscript form, or `None`.
#[inline]
pub(super) fn super_lookup(ch: char) -> Option<&'static str> {
    lut_lookup(&SUPER_LUT, ch)
}

/// Look up a character's Unicode subscript form, or `None`.
#[inline]
pub(super) fn sub_lookup(ch: char) -> Option<&'static str> {
    lut_lookup(&SUB_LUT, ch)
}

/// LaTeX commands that appear as super/subscript *content*, mapped to the
/// glyph to render in the raised slot. The transpose family (`\top`,
/// `\intercal`, `\transpose`) maps to superscript T because `A^\top` is
/// transpose notation, not the ⊤ lattice symbol — outside a superscript,
/// `\top` still concealment-maps to ⊤ via the symbol table. Marks that already
/// read as raised (° ′ † ‡ ∗ ⋆ ⊥) are placed as-is.
static SUPER_COMMAND_MAP: &[(&str, &str)] = &[
    ("ast", "∗"),
    ("bullet", "∙"),
    ("circ", "°"),
    ("dag", "†"),
    ("dagger", "†"),
    ("ddag", "‡"),
    ("ddagger", "‡"),
    ("intercal", "ᵀ"),
    ("perp", "⊥"),
    ("prime", "′"),
    ("star", "⋆"),
    ("top", "ᵀ"),
    ("transpose", "ᵀ"),
];

/// LaTeX commands that appear as subscript content, mapped to the glyph to
/// render in the lowered slot. Physics uses `v_\perp` / `v_\parallel` for
/// perpendicular/parallel components.
static SUB_COMMAND_MAP: &[(&str, &str)] = &[
    ("ast", "∗"),
    ("bullet", "∙"),
    ("circ", "∘"),
    ("dagger", "†"),
    ("intercal", "ᵀ"),
    ("parallel", "∥"),
    ("perp", "⊥"),
    ("prime", "′"),
    ("star", "⋆"),
    ("top", "ᵀ"),
];

fn command_map_lookup(map: &[(&str, &'static str)], name: &str) -> Option<&'static str> {
    map.iter().find(|(k, _)| *k == name).map(|&(_, v)| v)
}

/// Look up a LaTeX command's superscript rendering (`\top` → ᵀ), or `None`.
#[inline]
pub(super) fn super_command_lookup(name: &str) -> Option<&'static str> {
    command_map_lookup(SUPER_COMMAND_MAP, name)
}

/// Look up a LaTeX command's subscript rendering (`\perp` → ⊥), or `None`.
#[inline]
pub(super) fn sub_command_lookup(name: &str) -> Option<&'static str> {
    command_map_lookup(SUB_COMMAND_MAP, name)
}

/// Resolve braced super/subscript content that is a single command
/// (`{\top}`) or a font-wrapped single letter (`{\mathsf{T}}`) to its
/// raised/lowered glyph. `cmd_lookup` handles the bare-command case;
/// `char_lookup` handles the font-wrapped letter, ignoring the font.
fn braced_command(
    content: &str,
    cmd_lookup: fn(&str) -> Option<&'static str>,
    char_lookup: fn(char) -> Option<&'static str>,
) -> Option<&'static str> {
    let rest = content.trim().strip_prefix('\\')?;
    let name_len = rest.chars().take_while(char::is_ascii_alphabetic).count();
    let (name, after) = rest.split_at(name_len);
    if after.is_empty() {
        return cmd_lookup(name);
    }
    if matches!(
        name,
        "mathsf"
            | "mathrm"
            | "text"
            | "mathtt"
            | "mathbf"
            | "mathit"
            | "mathbb"
            | "mathcal"
            | "boldsymbol"
    ) && after.starts_with('{')
        && after.ends_with('}')
    {
        let inner = &after[1..after.len() - 1];
        let mut it = inner.chars();
        if let (Some(ch), None) = (it.next(), it.next()) {
            return char_lookup(ch);
        }
    }
    None
}

/// Braced superscript command resolution (`{\top}`/`{\mathsf{T}}` → ᵀ).
#[inline]
pub(super) fn braced_super_command(content: &str) -> Option<&'static str> {
    braced_command(content, super_command_lookup, super_lookup)
}

/// Braced subscript command resolution (`{\perp}` → ⊥).
#[inline]
pub(super) fn braced_sub_command(content: &str) -> Option<&'static str> {
    braced_command(content, sub_command_lookup, sub_lookup)
}

/// Spell out a single ASCII digit; pass anything else through unchanged.
fn digit_to_word(s: &str) -> &str {
    match s {
        "0" => "zero",
        "1" => "one",
        "2" => "two",
        "3" => "three",
        "4" => "four",
        "5" => "five",
        "6" => "six",
        "7" => "seven",
        "8" => "eight",
        "9" => "nine",
        _ => s,
    }
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
    // Bold/double-struck/mono digit glyphs are keyed by word ("bfzero", not
    // "bf0"), so a bare digit must be spelled out before the lookup.
    let letter = digit_to_word(letter);
    // Build the Julia name (e.g., "bfb", "bbR", "bfzero") and binary-search.
    let julia_name = format!("{prefix}{letter}");
    SYMBOLS
        .binary_search_by_key(&julia_name.as_str(), |&(k, _)| k)
        .ok()
        .map(|i| SYMBOLS[i].1)
}
