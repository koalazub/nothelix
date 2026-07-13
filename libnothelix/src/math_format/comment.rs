//! Julia comment-line lexing: the `# ` body, blank detection, and trailing-CR
//! preservation. A bare `#` carries an empty body so a save-time trailing-space
//! trim does not read as a non-comment line and terminate a `$$` block.

pub(super) fn comment_body(line: &str) -> Option<&str> {
    let body = line.trim_end_matches('\r');
    if body == "#" {
        Some("")
    } else {
        body.strip_prefix("# ")
    }
}

pub(super) fn is_blank_comment_line(line: &str) -> bool {
    matches!(comment_body(line), Some(b) if b.trim().is_empty())
}

pub(super) fn cr_suffix(line: &str) -> &'static str {
    if line.ends_with('\r') { "\r" } else { "" }
}
