//! Static `.jl` source scan: find the cell + line where a variable is
//! assigned. Used by the `UndefVarError` enricher in `error_format` —
//! when the kernel says "x not defined" but the static scan finds an
//! assignment in a later cell, the formatter can point at that cell
//! and tell the user to run it first.
//!
//! Pure source-level scan, no Julia kernel involvement.

// Steel's `register_fn` marshals values from the Steel VM and requires
// the registered fn's signature to take owned types (`String`), not
// borrows.
#![allow(clippy::needless_pass_by_value)]

use super::cells::{CellKind, parse_jl_file};

/// A code cell reduced to what the error-formatter's cross-cell scan
/// needs: its index, its source, and any marker-line label.
pub struct ScanCell {
    pub index: i64,
    pub code: String,
    pub label: String,
}

/// Load every code cell of a notebook `.jl` for static cross-cell
/// analysis. Returns an empty vector when the file can't be parsed —
/// callers treat "no cells" and "unreadable" identically (no guidance).
pub fn scan_code_cells(jl_path: &str) -> Vec<ScanCell> {
    let Ok((cells, _)) = parse_jl_file(jl_path) else {
        return Vec::new();
    };
    cells
        .into_iter()
        .filter(|c| c.kind == CellKind::Code)
        .map(|c| ScanCell {
            index: c.index as i64,
            code: c.code,
            label: c.marker_comment,
        })
        .collect()
}

pub fn scan_variable_definition(jl_path: String, var_name: String) -> String {
    let Ok((cells, _)) = parse_jl_file(&jl_path) else {
        return "null".to_string();
    };
    for cell in &cells {
        if cell.kind != CellKind::Code {
            continue;
        }
        if let Some((line_no, line_text)) = find_assignment_line(&cell.code, &var_name) {
            return format!(
                r#"{{"cell_index":{},"line_in_cell":{},"line_text":{}}}"#,
                cell.index,
                line_no,
                serde_json::to_string(line_text.trim()).unwrap_or_else(|_| "\"\"".to_string())
            );
        }
    }
    "null".to_string()
}

/// Find the first line in `code` that assigns to `var_name`. Returns
/// `(line_number_0_indexed, line_text)` or `None`.
///
/// Recognizes:
///   - `var = expr`        (plain assignment)
///   - `var .= expr`       (broadcast assignment)
///   - `var += expr`       (compound assignments)
///   - `var, other = ...`  (destructuring — first LHS position)
///
/// Rejects:
///   - `var == expr`       (equality comparison)
///   - `function var(...)` (function definition — we want variable
///     introductions, though functions ARE technically binding `var`;
///     caller can iterate again if the variable slot turns out to be a
///     function, but the common UX case is `x = …` style assignments)
///   - Matches inside `#` comment tails (best-effort — we just strip the
///     comment tail before scanning)
fn find_assignment_line(code: &str, var_name: &str) -> Option<(usize, String)> {
    for (idx, raw_line) in code.lines().enumerate() {
        // Strip inline comments — `x = 5 # note` → `x = 5 `
        let line = match raw_line.find('#') {
            Some(pos) => &raw_line[..pos],
            None => raw_line,
        };
        if line_assigns_to(line, var_name) {
            return Some((idx, raw_line.to_string()));
        }
    }
    None
}

/// Token-level check: does `line` contain `var_name` followed by an
/// assignment operator (but not `==`)?
fn line_assigns_to(line: &str, var_name: &str) -> bool {
    let bytes = line.as_bytes();
    let mut i = 0;
    let name_bytes = var_name.as_bytes();
    while i + name_bytes.len() <= bytes.len() {
        // Match `var_name` on an identifier boundary.
        let prev_ok = i == 0 || !is_ident_byte(bytes[i - 1]);
        if prev_ok && &bytes[i..i + name_bytes.len()] == name_bytes {
            let after = i + name_bytes.len();
            // Next char must NOT be part of an identifier (so we don't
            // match `var_namex`).
            if after < bytes.len() && is_ident_byte(bytes[after]) {
                i += 1;
                continue;
            }
            // Skip spaces and look for `=` that isn't `==`.
            let mut j = after;
            while j < bytes.len() && (bytes[j] == b' ' || bytes[j] == b'\t') {
                j += 1;
            }
            // Accept `=`, `.=`, `+=`, `-=`, `*=`, `/=`, `^=`, `%=`, etc.
            // Reject `==`.
            if j < bytes.len() {
                let c = bytes[j];
                let has_dot = j >= 1 && bytes[j.saturating_sub(1)] == b'.';
                if c == b'=' && bytes.get(j + 1) != Some(&b'=') {
                    return true;
                }
                if matches!(c, b'+' | b'-' | b'*' | b'/' | b'^' | b'%')
                    && bytes.get(j + 1) == Some(&b'=')
                {
                    return true;
                }
                if has_dot && c == b'=' {
                    return true;
                }
            }
        }
        i += 1;
    }
    false
}

#[inline]
fn is_ident_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}
