//! Alias names → Unicode character lookup, consulted only when the exact
//! [`super::symbol_table::SYMBOLS`] lookup misses, so Julia REPL names
//! always win.
//!
//! Covers the unicode-math canonical double-struck names (`\BbbA`…`\BbbZ`,
//! `\Bbba`…`\Bbbz`, `\Bbbzero`…`\Bbbnine`) and friendly set names
//! (`\Reals`, `\Integers`, …). Every `Bbb*` target is the glyph the
//! corresponding `bb*` SYMBOLS entry produces; the test
//! `bbb_aliases_derive_from_symbols` enforces that they never diverge.
//!
//! The table is sorted by name so `binary_search_by_key` works. The test
//! `alias_table_is_sorted` enforces this invariant.

pub(crate) static ALIASES: &[(&str, &str)] = &[
    ("BbbA", "𝔸"),
    ("BbbB", "𝔹"),
    ("BbbC", "ℂ"),
    ("BbbD", "𝔻"),
    ("BbbE", "𝔼"),
    ("BbbF", "𝔽"),
    ("BbbG", "𝔾"),
    ("BbbH", "ℍ"),
    ("BbbI", "𝕀"),
    ("BbbJ", "𝕁"),
    ("BbbK", "𝕂"),
    ("BbbL", "𝕃"),
    ("BbbM", "𝕄"),
    ("BbbN", "ℕ"),
    ("BbbO", "𝕆"),
    ("BbbP", "ℙ"),
    ("BbbQ", "ℚ"),
    ("BbbR", "ℝ"),
    ("BbbS", "𝕊"),
    ("BbbT", "𝕋"),
    ("BbbU", "𝕌"),
    ("BbbV", "𝕍"),
    ("BbbW", "𝕎"),
    ("BbbX", "𝕏"),
    ("BbbY", "𝕐"),
    ("BbbZ", "ℤ"),
    ("Bbba", "𝕒"),
    ("Bbbb", "𝕓"),
    ("Bbbc", "𝕔"),
    ("Bbbd", "𝕕"),
    ("Bbbe", "𝕖"),
    ("Bbbeight", "𝟠"),
    ("Bbbf", "𝕗"),
    ("Bbbfive", "𝟝"),
    ("Bbbfour", "𝟜"),
    ("Bbbg", "𝕘"),
    ("Bbbh", "𝕙"),
    ("Bbbi", "𝕚"),
    ("Bbbj", "𝕛"),
    ("Bbbk", "𝕜"),
    ("Bbbl", "𝕝"),
    ("Bbbm", "𝕞"),
    ("Bbbn", "𝕟"),
    ("Bbbnine", "𝟡"),
    ("Bbbo", "𝕠"),
    ("Bbbone", "𝟙"),
    ("Bbbp", "𝕡"),
    ("Bbbq", "𝕢"),
    ("Bbbr", "𝕣"),
    ("Bbbs", "𝕤"),
    ("Bbbseven", "𝟟"),
    ("Bbbsix", "𝟞"),
    ("Bbbt", "𝕥"),
    ("Bbbthree", "𝟛"),
    ("Bbbtwo", "𝟚"),
    ("Bbbu", "𝕦"),
    ("Bbbv", "𝕧"),
    ("Bbbw", "𝕨"),
    ("Bbbx", "𝕩"),
    ("Bbby", "𝕪"),
    ("Bbbz", "𝕫"),
    ("Bbbzero", "𝟘"),
    ("Complexes", "ℂ"),
    ("Integers", "ℤ"),
    ("Naturals", "ℕ"),
    ("Primes", "ℙ"),
    ("Quaternions", "ℍ"),
    ("Rationals", "ℚ"),
    ("Reals", "ℝ"),
];

pub(crate) fn alias_lookup(name: &str) -> Option<&'static str> {
    ALIASES
        .binary_search_by_key(&name, |&(k, _)| k)
        .ok()
        .map(|i| ALIASES[i].1)
}
