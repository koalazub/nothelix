pub(super) fn matching_brace(bytes: &[u8], from: usize) -> usize {
    let mut depth = 1u32;
    let mut j = from;
    while j < bytes.len() {
        match bytes[j] {
            b'{' => depth += 1,
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    return j;
                }
            }
            _ => {}
        }
        j += 1;
    }
    j
}

pub(super) fn past_matching_brace(bytes: &[u8], from: usize) -> usize {
    let close = matching_brace(bytes, from);
    if close < bytes.len() {
        close + 1
    } else {
        bytes.len()
    }
}

pub(super) fn alphabetic_end(bytes: &[u8], mut from: usize) -> usize {
    while from < bytes.len() && bytes[from].is_ascii_alphabetic() {
        from += 1;
    }
    from
}

pub(super) fn past_spaces(bytes: &[u8], mut from: usize) -> usize {
    while from < bytes.len() && bytes[from] == b' ' {
        from += 1;
    }
    from
}

pub(super) fn past_spaces_and_tabs(bytes: &[u8], mut from: usize) -> usize {
    while from < bytes.len() && matches!(bytes[from], b' ' | b'\t') {
        from += 1;
    }
    from
}

pub(super) fn past_whitespace(bytes: &[u8], mut from: usize) -> usize {
    while from < bytes.len() && matches!(bytes[from], b' ' | b'\t' | b'\n' | b'\r') {
        from += 1;
    }
    from
}
