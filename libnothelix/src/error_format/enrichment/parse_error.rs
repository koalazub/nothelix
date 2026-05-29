//! `ParseError` (and `Meta.ParseError`) enricher.
//!
//! Julia's parser often reports `Expected end` for problems that are
//! really "you have one more `]` than `[`" — the caret column lands far
//! from the actual stray bracket and the user can't tell from the
//! message alone. We pin the column to a specific source char + name
//! any net bracket imbalance ("1 extra `]` on this line — stray close")
//! so the user has somewhere concrete to look.

use std::fmt::Write;

pub(super) fn enrich(message: &str, source: &str) -> Option<String> {
    let col = scan_parse_error_col(message)?;
    if col == 0 || source.is_empty() {
        return None;
    }

    let mut out = String::new();
    let col_idx = col.saturating_sub(1);
    if col_idx < source.len() {
        let _ = write!(out, "   = note: error at column {col}");
        if let Some(ch) = source.chars().nth(col_idx) {
            if !ch.is_whitespace() {
                let _ = write!(out, " (near `{ch}`)");
            }
        }
        out.push('\n');
    }

    let (paren, bracket, brace) = count_bracket_balance(source);
    if paren != 0 || bracket != 0 || brace != 0 {
        out.push_str("   = note: bracket balance on this line:\n");
        let report = |out: &mut String, net: i32, open: char, close: char| {
            if net > 0 {
                let _ = writeln!(out, "   |   {net} more `{open}` than `{close}` — unclosed");
            } else if net < 0 {
                let _ = writeln!(
                    out,
                    "   |   {} more `{close}` than `{open}` — stray close",
                    -net
                );
            }
        };
        report(&mut out, paren, '(', ')');
        report(&mut out, bracket, '[', ']');
        report(&mut out, brace, '{', '}');
        out.push_str(
            "   = help: scan the line for an extra or missing bracket before trusting the \"Expected end\" message\n",
        );
    }

    Some(out)
}

/// Net bracket imbalance on a source line, ignoring anything inside
/// `"..."` strings. Returns `(parens, brackets, braces)` where each
/// value is `opens - closes` — positive means unclosed, negative means
/// stray close. Strings are skipped with a simple backslash-aware
/// scanner so `"]"` inside a literal doesn't throw off the count.
pub(super) fn count_bracket_balance(source: &str) -> (i32, i32, i32) {
    let bytes = source.as_bytes();
    let mut paren = 0i32;
    let mut bracket = 0i32;
    let mut brace = 0i32;
    let mut in_str = false;
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        if in_str {
            if b == b'\\' {
                i += 2;
                continue;
            }
            if b == b'"' {
                in_str = false;
            }
        } else {
            match b {
                b'"' => in_str = true,
                b'#' => break, // rest of line is a comment
                b'(' => paren += 1,
                b')' => paren -= 1,
                b'[' => bracket += 1,
                b']' => bracket -= 1,
                b'{' => brace += 1,
                b'}' => brace -= 1,
                _ => {}
            }
        }
        i += 1;
    }
    (paren, bracket, brace)
}

/// Extract column number from `ParseError` message "Error @ <file:line:col>".
fn scan_parse_error_col(msg: &str) -> Option<usize> {
    for line in msg.lines() {
        let trimmed = line.trim().trim_start_matches("# ");
        if let Some(rest) = trimmed.strip_prefix("Error @ ") {
            let parts: Vec<&str> = rest.rsplitn(3, ':').collect();
            if !parts.is_empty() {
                if let Ok(col) = parts[0].trim().parse::<usize>() {
                    return Some(col);
                }
            }
        }
    }
    None
}

/// Extract location from Julia error patterns like "# Error @ none:10:23"
/// or "Error @ /path/to/file.jl:42:5". Returns "line 10, column 23".
/// Public to the rest of error_format because the raw-error renderer
/// uses it too.
pub(crate) fn scan_error_location(msg: &str) -> Option<String> {
    for line in msg.lines() {
        let trimmed = line.trim().trim_start_matches("# ");
        if let Some(rest) = trimmed.strip_prefix("Error @ ") {
            let parts: Vec<&str> = rest.rsplitn(3, ':').collect();
            if parts.len() >= 2 {
                let col = parts[0].trim();
                let line_num = parts[1].trim();
                if line_num.chars().all(|c| c.is_ascii_digit())
                    && col.chars().all(|c| c.is_ascii_digit())
                {
                    return Some(format!("line {line_num}, column {col}"));
                }
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn count_bracket_balance_simple_imbalance() {
        // 1 extra `]` should report (-1) on bracket count.
        let (p, br, bc) = count_bracket_balance("x = arr[1]]");
        assert_eq!((p, bc), (0, 0));
        assert_eq!(br, -1);
    }

    #[test]
    fn count_bracket_balance_ignores_brackets_inside_strings() {
        // The `]` inside the string should NOT count.
        let (p, br, bc) = count_bracket_balance(r#"println("close: ]")"#);
        assert_eq!((p, br, bc), (0, 0, 0));
    }

    #[test]
    fn scan_parse_error_col_reads_column_from_error_at() {
        let col = scan_parse_error_col("# Error @ none:10:23\nParseError: Expected end");
        assert_eq!(col, Some(23));
    }

    #[test]
    fn scan_error_location_emits_human_string() {
        let loc = scan_error_location("Error @ /path/to/file.jl:42:5");
        assert_eq!(loc, Some("line 42, column 5".to_string()));
    }
}
