pub(super) fn clean_message(msg: &str) -> &str {
    let msg = msg
        .find("\nClosest candidates")
        .or_else(|| msg.find("\n\nClosest candidates"))
        .or_else(|| msg.find("Closest candidates are:"))
        .map_or(msg, |idx| msg[..idx].trim_end());
    msg.find("\nStacktrace:")
        .map_or(msg, |idx| msg[..idx].trim_end())
}

pub(super) fn first_line(text: &str) -> &str {
    text.lines().next().unwrap_or(text)
}

pub(super) fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        return s.to_string();
    }
    let mut end = max.min(s.len());
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}…", &s[..end])
}

pub(super) fn wrap(text: &str, width: usize) -> Vec<String> {
    if text.len() <= width {
        return vec![text.to_string()];
    }
    let mut lines = Vec::new();
    let mut cur = String::new();
    for word in text.split_whitespace() {
        if cur.is_empty() {
            cur = word.to_string();
        } else if cur.len() + 1 + word.len() > width {
            lines.push(cur);
            cur = word.to_string();
        } else {
            cur.push(' ');
            cur.push_str(word);
        }
    }
    if !cur.is_empty() {
        lines.push(cur);
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn closest_candidates_tail_is_dropped() {
        let msg = "no method matching f()\nClosest candidates are:\n  noise";
        assert_eq!(clean_message(msg), "no method matching f()");
    }

    #[test]
    fn stacktrace_tail_is_dropped() {
        assert_eq!(clean_message("boom\nStacktrace:\n [1] top"), "boom");
    }

    #[test]
    fn first_line_of_empty_text_is_the_text() {
        assert_eq!(first_line(""), "");
        assert_eq!(first_line("a\nb"), "a");
    }

    #[test]
    fn truncate_respects_char_boundaries() {
        assert_eq!(truncate("abc", 10), "abc");
        assert_eq!(truncate("αβγδ", 5), "αβ…");
    }

    #[test]
    fn wrap_breaks_on_word_boundaries() {
        assert_eq!(wrap("one two three", 7), vec!["one two", "three"]);
    }
}
