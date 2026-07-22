pub fn find_matching_paren(bytes: &[u8], open_idx: usize) -> Option<usize> {
    if bytes.get(open_idx).copied() != Some(b'(') {
        return None;
    }
    let mut depth: i32 = 1;
    for (offset, b) in bytes.iter().enumerate().skip(open_idx + 1) {
        match b {
            b'(' => depth += 1,
            b')' => {
                depth -= 1;
                if depth == 0 {
                    return Some(offset);
                }
            }
            _ => {}
        }
    }
    None
}

pub fn split_top_level_commas(s: &str) -> Vec<String> {
    let mut args = Vec::new();
    let mut current = String::new();
    let mut depth = 0i32;
    let flush = |current: &mut String, args: &mut Vec<String>| {
        let trimmed = current.trim();
        if !trimmed.is_empty() {
            args.push(trimmed.to_string());
        }
        current.clear();
    };
    for ch in s.chars() {
        match ch {
            '(' | '[' | '{' => {
                depth += 1;
                current.push(ch);
            }
            ')' | ']' | '}' => {
                depth -= 1;
                current.push(ch);
            }
            ',' if depth == 0 => flush(&mut current, &mut args),
            _ => current.push(ch),
        }
    }
    flush(&mut current, &mut args);
    args
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn find_matching_paren_handles_nesting() {
        assert_eq!(find_matching_paren(b"f(g(x), y)", 1), Some(9));
    }

    #[test]
    fn find_matching_paren_requires_open_byte_at_idx() {
        assert_eq!(find_matching_paren(b"abc", 0), None);
    }

    #[test]
    fn find_matching_paren_returns_none_when_unclosed() {
        assert_eq!(find_matching_paren(b"f(g(x)", 1), None);
    }

    #[test]
    fn split_top_level_commas_respects_nesting() {
        let parts = split_top_level_commas("a, f(b, c), [d, e]");
        assert_eq!(parts, vec!["a", "f(b, c)", "[d, e]"]);
    }

    #[test]
    fn split_top_level_commas_drops_empty_fragments() {
        let parts = split_top_level_commas(",a,, b,");
        assert_eq!(parts, vec!["a", "b"]);
    }
}
