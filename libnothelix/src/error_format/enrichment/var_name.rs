const ASCII_MAX: u8 = 127;

pub(super) fn extract_var_name(expr: &str) -> String {
    let expr = expr.trim().trim_end_matches('\'');
    let bytes = expr.as_bytes();
    if !bytes.first().copied().is_some_and(is_name_start) {
        return String::new();
    }
    let end = bytes.iter().take_while(|b| is_name_byte(**b)).count();
    expr[..end].to_string()
}

fn is_name_start(b: u8) -> bool {
    b.is_ascii_alphabetic() || b == b'_'
}

fn is_name_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_' || b == b'.' || b > ASCII_MAX
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_name_is_kept_whole() {
        assert_eq!(extract_var_name("S_hat"), "S_hat");
    }

    #[test]
    fn transpose_suffix_is_stripped() {
        assert_eq!(extract_var_name("A'"), "A");
    }

    #[test]
    fn leading_name_is_taken_from_a_call() {
        assert_eq!(extract_var_name("func(x)"), "func");
    }

    #[test]
    fn non_name_starts_yield_nothing() {
        assert_eq!(extract_var_name("3 * x"), "");
        assert_eq!(extract_var_name(""), "");
    }

    #[test]
    fn combining_marks_stay_attached() {
        assert_eq!(extract_var_name("x\u{0302} * y"), "x\u{0302}");
    }
}
