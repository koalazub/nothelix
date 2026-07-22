pub fn is_identifier(s: &str) -> bool {
    let mut chars = s.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    if !(first.is_ascii_alphabetic() || first == '_') {
        return false;
    }
    chars.all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '!')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn is_identifier_accepts_standard_forms() {
        assert!(is_identifier("foo"));
        assert!(is_identifier("foo_bar"));
        assert!(is_identifier("foo!"));
        assert!(is_identifier("_private"));
        assert!(is_identifier("a1b2"));
    }

    #[test]
    fn is_identifier_rejects_invalid_forms() {
        assert!(!is_identifier(""));
        assert!(!is_identifier("9foo"));
        assert!(!is_identifier("foo bar"));
        assert!(!is_identifier("foo-bar"));
    }
}
