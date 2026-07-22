pub(super) static ALIASES: &[(&str, &str)] = &[
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

pub(super) fn lookup(name: &str) -> Option<&'static str> {
    ALIASES
        .binary_search_by_key(&name, |&(k, _)| k)
        .ok()
        .map(|i| ALIASES[i].1)
}

#[cfg(test)]
mod tests {
    use super::super::{julia_repl::SYMBOLS, unicode_lookup};
    use super::ALIASES;

    #[test]
    fn alias_table_is_sorted() {
        for i in 1..ALIASES.len() {
            assert!(
                ALIASES[i - 1].0 < ALIASES[i].0,
                "Alias table not sorted at index {i}: {:?} >= {:?}",
                ALIASES[i - 1].0,
                ALIASES[i].0
            );
        }
    }

    #[test]
    fn alias_keys_disjoint_from_symbols() {
        for &(alias, _) in ALIASES {
            assert!(
                SYMBOLS.binary_search_by_key(&alias, |&(k, _)| k).is_err(),
                "Alias {alias:?} shadows a SYMBOLS entry"
            );
        }
    }

    #[test]
    fn bbb_aliases_derive_from_symbols() {
        let letters = ('A'..='Z').chain('a'..='z').map(String::from);
        let digits = [
            "zero", "one", "two", "three", "four", "five", "six", "seven", "eight", "nine",
        ]
        .into_iter()
        .map(String::from);
        for suffix in letters.chain(digits) {
            let bb_key = format!("bb{suffix}");
            let i = SYMBOLS
                .binary_search_by_key(&bb_key.as_str(), |&(k, _)| k)
                .unwrap_or_else(|_| panic!("SYMBOLS lacks {bb_key:?}"));
            assert_eq!(
                unicode_lookup(format!("Bbb{suffix}")),
                SYMBOLS[i].1,
                "Bbb{suffix} diverges from SYMBOLS {bb_key:?}"
            );
        }
    }

    #[test]
    fn exact_julia_name_wins_over_alias() {
        assert!(ALIASES.binary_search_by_key(&"bbR", |&(k, _)| k).is_err());
        assert_eq!(unicode_lookup("bbR".into()), "ℝ");
    }
}
