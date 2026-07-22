mod julia_repl;
mod unicode_math_aliases;

use serde_json::json;

const MAX_COMPLETIONS: usize = 50;

pub(super) fn julia_repl_name(name: &str) -> Option<&'static str> {
    julia_repl::lookup(name)
}

pub(super) fn symbol(name: &str) -> Option<&'static str> {
    julia_repl::lookup(name).or_else(|| unicode_math_aliases::lookup(name))
}

pub fn unicode_lookup(name: String) -> String {
    symbol(&name).unwrap_or_default().to_string()
}

pub fn unicode_completions_for_prefix(prefix: String) -> String {
    let matches: Vec<_> = julia_repl::SYMBOLS
        .iter()
        .filter(|(name, _)| name.starts_with(prefix.as_str()))
        .take(MAX_COMPLETIONS)
        .map(|(name, glyph)| json!({"name": name, "char": glyph}))
        .collect();
    json!(matches).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lookup_alpha() {
        assert_eq!(unicode_lookup("alpha".into()), "α");
    }

    #[test]
    fn lookup_in() {
        assert_eq!(unicode_lookup("in".into()), "∈");
    }

    #[test]
    fn lookup_pi() {
        assert_eq!(unicode_lookup("pi".into()), "π");
    }

    #[test]
    fn lookup_missing() {
        assert_eq!(unicode_lookup("notareal symbol".into()), "");
    }

    #[test]
    fn lookup_alias_reals() {
        assert_eq!(unicode_lookup("Reals".into()), "ℝ");
    }

    #[test]
    fn lookup_alias_bbb_r() {
        assert_eq!(unicode_lookup("BbbR".into()), "ℝ");
    }

    #[test]
    fn lookup_alias_bbb_c() {
        assert_eq!(unicode_lookup("BbbC".into()), "ℂ");
    }

    #[test]
    fn lookup_alias_bbb_a() {
        assert_eq!(unicode_lookup("BbbA".into()), "𝔸");
    }

    #[test]
    fn lookup_alias_bbb_zero() {
        assert_eq!(unicode_lookup("Bbbzero".into()), "𝟘");
    }

    #[test]
    fn completions_prefix() {
        let result = unicode_completions_for_prefix("alp".into());
        let v: serde_json::Value = serde_json::from_str(&result).unwrap();
        let arr = v.as_array().unwrap();
        assert!(!arr.is_empty());
        assert!(arr.iter().any(|e| e["name"] == "alpha"));
    }

    #[test]
    fn completions_empty_prefix_capped() {
        let result = unicode_completions_for_prefix("".into());
        let v: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert!(v.as_array().unwrap().len() <= MAX_COMPLETIONS);
    }
}
