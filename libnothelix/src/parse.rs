/// Find the byte index of the matching closing brace.
///
/// `start` must point to the byte *after* an opening `{`.
/// Returns `Some(pos)` where `pos` is the byte index after the matching `}`,
/// or `None` if no matching brace is found.
#[inline]
fn find_matching_brace(bytes: &[u8], mut start: usize) -> Option<usize> {
    let mut depth = 1i32;
    while start < bytes.len() && depth > 0 {
        match bytes[start] {
            b'{' => depth += 1,
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(start + 1);
                }
            }
            _ => {}
        }
        start += 1;
    }
    None
}

/// Find the matching `}` for content starting after the opening `{`.
/// Returns the byte index of the matching `}` within `s`, or `None` if not found.
pub(crate) fn matching_brace(s: &str) -> Option<usize> {
    find_matching_brace(s.as_bytes(), 0).map(|pos| pos - 1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_matching_brace_simple() {
        assert_eq!(matching_brace("hello}"), Some(5));
        assert_eq!(matching_brace("}"), Some(0));
        assert_eq!(matching_brace(""), None);
    }

    #[test]
    fn test_matching_brace_nested() {
        assert_eq!(matching_brace("nested}"), Some(6));
        assert_eq!(matching_brace("a{b{c}d}e}"), Some(9));
        assert_eq!(matching_brace("{unmatched"), None);
    }

    #[test]
    fn test_find_matching_brace_bytes() {
        let bytes = b"hello{world}";
        assert_eq!(find_matching_brace(bytes, 6), Some(12));
        let bytes = b"{nested}";
        assert_eq!(find_matching_brace(bytes, 1), Some(8));
    }
}
