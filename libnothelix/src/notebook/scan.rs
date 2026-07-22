use serde_json::json;

use super::cells::parse_jl_file;
use super::marker::CellKind;
use crate::error::Result;

const NOT_FOUND: &str = "null";

pub struct ScanCell {
    pub index: i64,
    pub code: String,
    pub label: String,
}

pub fn scan_code_cells(jl_path: &str) -> Vec<ScanCell> {
    code_cells(jl_path).unwrap_or_default()
}

fn code_cells(jl_path: &str) -> Result<Vec<ScanCell>> {
    let (cells, _) = parse_jl_file(jl_path)?;
    Ok(cells
        .into_iter()
        .filter(|cell| cell.kind == CellKind::Code)
        .map(|cell| ScanCell {
            index: cell.index as i64,
            code: cell.code,
            label: cell.marker_comment,
        })
        .collect())
}

pub fn scan_variable_definition(jl_path: String, var_name: String) -> String {
    definition_site(&jl_path, &var_name).unwrap_or_else(|| NOT_FOUND.to_string())
}

fn definition_site(jl_path: &str, var_name: &str) -> Option<String> {
    code_cells(jl_path).ok()?.iter().find_map(|cell| {
        let (line_in_cell, line_text) = find_assignment_line(&cell.code, var_name)?;
        Some(
            json!({
                "cell_index": cell.index,
                "line_in_cell": line_in_cell,
                "line_text": line_text.trim(),
            })
            .to_string(),
        )
    })
}

fn find_assignment_line(code: &str, var_name: &str) -> Option<(usize, String)> {
    code.lines().enumerate().find_map(|(at, raw_line)| {
        let uncommented = match raw_line.split_once('#') {
            Some((before_comment, _)) => before_comment,
            None => raw_line,
        };
        line_assigns_to(uncommented, var_name).then(|| (at, raw_line.to_string()))
    })
}

fn line_assigns_to(line: &str, var_name: &str) -> bool {
    let bytes = line.as_bytes();
    let name = var_name.as_bytes();
    let mut at = 0;
    while at + name.len() <= bytes.len() {
        let on_boundary = at == 0 || !is_ident_byte(bytes[at - 1]);
        if on_boundary && &bytes[at..at + name.len()] == name {
            let after = at + name.len();
            if !bytes.get(after).is_some_and(|&b| is_ident_byte(b))
                && starts_assignment(bytes, skip_blanks(bytes, after))
            {
                return true;
            }
        }
        at += 1;
    }
    false
}

fn skip_blanks(bytes: &[u8], from: usize) -> usize {
    let mut at = from;
    while matches!(bytes.get(at), Some(b' ' | b'\t')) {
        at += 1;
    }
    at
}

fn starts_assignment(bytes: &[u8], at: usize) -> bool {
    let Some(&op) = bytes.get(at) else {
        return false;
    };
    let followed_by_equals = bytes.get(at + 1) == Some(&b'=');
    match op {
        b'=' => !followed_by_equals,
        b'+' | b'-' | b'*' | b'/' | b'^' | b'%' => followed_by_equals,
        _ => false,
    }
}

#[inline]
fn is_ident_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}

#[cfg(test)]
mod tests {
    use super::line_assigns_to;

    #[test]
    fn assignment_forms_are_recognised() {
        for line in ["x = 1", "x=1", "x += 1", "x *= 2", "  x  =  1"] {
            assert!(line_assigns_to(line, "x"), "{line}");
        }
    }

    #[test]
    fn comparisons_and_other_names_are_rejected() {
        for line in ["x == 1", "xy = 1", "ax = 1", "y = x", "x, y = f()"] {
            assert!(!line_assigns_to(line, "x"), "{line}");
        }
    }
}
