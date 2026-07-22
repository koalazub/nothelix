use std::fmt::Write;

use super::super::scan::Scanner;
use super::super::types::{ScopeVarEntry, StructuredError};

const MAX_LISTED_NAMES: usize = 5;
const OBJECTS_OF_TYPE: &str = "objects of type ";
const ARE_NOT_CALLABLE: &str = " are not callable";

pub(super) fn enrich(message: &str, source: &str, err: &StructuredError) -> Option<String> {
    let obj_type = scan_uncallable_type(message)?;
    let called = scan_call_identifiers(source);

    let mut out = String::new();
    let _ = writeln!(
        out,
        "   = note: called something of type {obj_type} (not a function)"
    );

    match err.in_scope_variable_types.get(&obj_type) {
        Some(entries) => write_typed_vars(&mut out, &obj_type, entries, &called),
        None if !called.is_empty() => write_called_names(&mut out, &called),
        None => {}
    }
    Some(out)
}

fn write_typed_vars(
    out: &mut String,
    obj_type: &str,
    entries: &[ScopeVarEntry],
    called: &[String],
) {
    let matching: Vec<&ScopeVarEntry> = entries
        .iter()
        .filter(|entry| called.iter().any(|name| name == &entry.name))
        .collect();

    if matching.is_empty() {
        let _ = writeln!(out, "   = scope: in-scope values of type {obj_type}:");
        for entry in entries {
            let _ = writeln!(out, "   |   `{}` — cell {}", entry.name, entry.cell);
        }
        return;
    }

    out.push_str("   = scope: the call site likely resolves to:\n");
    for entry in matching {
        let _ = writeln!(
            out,
            "   |   `{}` — {obj_type} assigned in cell {}",
            entry.name, entry.cell
        );
    }
    out.push_str("   = help: that name is a value, not a function. Index it with `[…]` or pick a different name for your function.\n");
}

fn write_called_names(out: &mut String, called: &[String]) {
    out.push_str("   = note: names called in the source line:\n");
    for name in called.iter().take(MAX_LISTED_NAMES) {
        let _ = writeln!(out, "   |   `{name}()`");
    }
    out.push_str("   = help: one of these resolves to a value, not a function. Execute upstream cells so the kernel knows each binding's type, then re-run this cell for a pinpointed hint.\n");
}

fn scan_uncallable_type(msg: &str) -> Option<String> {
    let start = msg.find(OBJECTS_OF_TYPE)? + OBJECTS_OF_TYPE.len();
    let after = &msg[start..];
    let name = after[..after.find(ARE_NOT_CALLABLE)?].trim();
    (!name.is_empty()).then(|| name.to_string())
}

fn scan_call_identifiers(source: &str) -> Vec<String> {
    let mut scanner = Scanner::new(source);
    let mut names = Vec::new();
    while let Some(b) = scanner.peek() {
        if b == b'#' {
            break;
        }
        if b == b'"' {
            scanner.skip_string_literal();
            continue;
        }
        if let Some(name) = scanner.scan_identifier() {
            if scanner.peek() == Some(b'(') {
                names.push(name.to_string());
            }
            continue;
        }
        scanner.advance();
    }
    names
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_type_between_markers() {
        assert_eq!(
            scan_uncallable_type("objects of type Vector{Float64} are not callable"),
            Some("Vector{Float64}".to_string())
        );
    }

    #[test]
    fn extract_returns_none_outside_pattern() {
        assert_eq!(scan_uncallable_type("totally unrelated message"), None);
    }

    #[test]
    fn scan_call_identifiers_skips_strings_and_comments() {
        let names = scan_call_identifiers(r#"foo(x) "bar(y)" # baz(z)"#);
        assert_eq!(names, vec!["foo"]);
    }
}
