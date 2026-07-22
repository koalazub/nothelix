use std::fmt::Write;

const ERROR_AT: &str = "Error @ ";

struct ErrorAt<'a> {
    line: Option<&'a str>,
    column: &'a str,
}

#[derive(Clone, Copy)]
struct BracketBalance {
    paren: i32,
    bracket: i32,
    brace: i32,
}

impl BracketBalance {
    fn is_even(self) -> bool {
        self.paren == 0 && self.bracket == 0 && self.brace == 0
    }

    fn report(self, out: &mut String) {
        write_imbalance(out, self.paren, '(', ')');
        write_imbalance(out, self.bracket, '[', ']');
        write_imbalance(out, self.brace, '{', '}');
    }
}

pub(super) fn enrich(message: &str, source: &str) -> Option<String> {
    let col = scan_parse_error_col(message)?;
    if col == 0 || source.is_empty() {
        return None;
    }

    let mut out = String::new();
    let col_idx = col - 1;
    if col_idx < source.len() {
        let _ = write!(out, "   = note: error at column {col}");
        if let Some(ch) = source.chars().nth(col_idx)
            && !ch.is_whitespace()
        {
            let _ = write!(out, " (near `{ch}`)");
        }
        out.push('\n');
    }

    let balance = scan_bracket_balance(source);
    if !balance.is_even() {
        out.push_str("   = note: bracket balance on this line:\n");
        balance.report(&mut out);
        out.push_str(
            "   = help: scan the line for an extra or missing bracket before trusting the \"Expected end\" message\n",
        );
    }

    Some(out)
}

fn write_imbalance(out: &mut String, net: i32, open: char, close: char) {
    if net > 0 {
        let _ = writeln!(out, "   |   {net} more `{open}` than `{close}` — unclosed");
    } else if net < 0 {
        let _ = writeln!(
            out,
            "   |   {} more `{close}` than `{open}` — stray close",
            -net
        );
    }
}

fn scan_bracket_balance(source: &str) -> BracketBalance {
    let mut balance = BracketBalance {
        paren: 0,
        bracket: 0,
        brace: 0,
    };
    let bytes = source.as_bytes();
    let mut in_string = false;
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        if in_string {
            if b == b'\\' {
                i += 2;
                continue;
            }
            if b == b'"' {
                in_string = false;
            }
        } else {
            match b {
                b'"' => in_string = true,
                b'#' => break,
                b'(' => balance.paren += 1,
                b')' => balance.paren -= 1,
                b'[' => balance.bracket += 1,
                b']' => balance.bracket -= 1,
                b'{' => balance.brace += 1,
                b'}' => balance.brace -= 1,
                _ => {}
            }
        }
        i += 1;
    }
    balance
}

fn error_at_lines(msg: &str) -> impl Iterator<Item = ErrorAt<'_>> {
    msg.lines().filter_map(|raw| {
        let rest = raw.trim().trim_start_matches("# ").strip_prefix(ERROR_AT)?;
        let mut parts = rest.rsplitn(3, ':');
        let column = parts.next()?.trim();
        Some(ErrorAt {
            line: parts.next().map(str::trim),
            column,
        })
    })
}

fn scan_parse_error_col(msg: &str) -> Option<usize> {
    error_at_lines(msg).find_map(|at| at.column.parse::<usize>().ok())
}

pub(crate) fn scan_error_location(msg: &str) -> Option<String> {
    error_at_lines(msg).find_map(|at| {
        let line = at.line?;
        (is_digits(line) && is_digits(at.column))
            .then(|| format!("line {line}, column {}", at.column))
    })
}

fn is_digits(s: &str) -> bool {
    s.chars().all(|c| c.is_ascii_digit())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bracket_balance_reports_simple_imbalance() {
        let balance = scan_bracket_balance("x = arr[1]]");
        assert_eq!((balance.paren, balance.brace), (0, 0));
        assert_eq!(balance.bracket, -1);
    }

    #[test]
    fn bracket_balance_ignores_brackets_inside_strings() {
        let balance = scan_bracket_balance(r#"println("close: ]")"#);
        assert!(balance.is_even());
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

    #[test]
    fn scan_error_location_rejects_non_numeric_line() {
        assert_eq!(scan_error_location("Error @ none:23"), None);
    }
}
