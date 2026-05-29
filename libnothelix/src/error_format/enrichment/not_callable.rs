//! "Objects of type X are not callable" enricher.
//!
//! Julia raises this whenever the parser sees `name(arg)` where `name`
//! is currently bound to a value rather than a function. The generic
//! E043 hint suggests "missing `*`", which is one valid cause but
//! misleading when the real issue is "you reassigned `X` two cells ago
//! and now you're calling it like a function".
//!
//! When the kernel populates `in_scope_variable_types`, we can pinpoint:
//! the failing object's type comes from the message, every identifier
//! called in the source line is a candidate, and intersecting the two
//! sets gives us the specific name with cell-anchored attribution.

use std::fmt::Write;

use super::super::scanners;
use super::super::types::{ScopeVarEntry, StructuredError};

pub(super) fn enrich(message: &str, source: &str, err: &StructuredError) -> Option<String> {
    let obj_type = extract_not_callable_type(message)?;

    let mut out = String::new();
    let _ = writeln!(
        out,
        "   = note: called something of type {obj_type} (not a function)"
    );

    let call_names = scan_call_identifiers(source);
    let typed_vars = err.in_scope_variable_types.get(&obj_type);

    if let Some(entries) = typed_vars {
        let matching: Vec<&ScopeVarEntry> = entries
            .iter()
            .filter(|e| call_names.iter().any(|c| c == &e.name))
            .collect();
        if matching.is_empty() {
            // Couldn't intersect — list all in-scope values of that type so the user can eyeball.
            let _ = writeln!(out, "   = scope: in-scope values of type {obj_type}:");
            for e in entries {
                let _ = writeln!(out, "   |   `{}` — cell {}", e.name, e.cell);
            }
        } else {
            out.push_str("   = scope: the call site likely resolves to:\n");
            for e in matching {
                let _ = writeln!(
                    out,
                    "   |   `{}` — {obj_type} assigned in cell {}",
                    e.name, e.cell
                );
            }
            out.push_str("   = help: that name is a value, not a function. Index it with `[…]` or pick a different name for your function.\n");
        }
    } else if !call_names.is_empty() {
        out.push_str("   = note: names called in the source line:\n");
        for name in call_names.iter().take(5) {
            let _ = writeln!(out, "   |   `{name}()`");
        }
        out.push_str("   = help: one of these resolves to a value, not a function. Execute upstream cells so the kernel knows each binding's type, then re-run this cell for a pinpointed hint.\n");
    }

    Some(out)
}

/// Pull the "of type X" portion out of `objects of type X are not callable`.
pub(super) fn extract_not_callable_type(msg: &str) -> Option<String> {
    let start = msg.find("objects of type ")?;
    let after = &msg[start + "objects of type ".len()..];
    let end = after.find(" are not callable")?;
    let t = after[..end].trim();
    if t.is_empty() { None } else { Some(t.to_string()) }
}

/// Find every `identifier(` call in a source line. Skips string content
/// and stops at `#` comments. Used to narrow the not-callable enrichment
/// when Julia's error omits the offending name.
fn scan_call_identifiers(source: &str) -> Vec<String> {
    let mut s = scanners::Scanner::new(source);
    let mut names = Vec::new();
    while let Some(b) = s.peek() {
        if b == b'#' {
            break;
        }
        if b == b'"' {
            s.skip_string_literal();
            continue;
        }
        if let Some(name) = s.scan_identifier() {
            if s.peek() == Some(b'(') {
                names.push(name.to_string());
            }
            continue;
        }
        s.advance();
    }
    names
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_type_between_markers() {
        assert_eq!(
            extract_not_callable_type("objects of type Vector{Float64} are not callable"),
            Some("Vector{Float64}".to_string())
        );
    }

    #[test]
    fn extract_returns_none_outside_pattern() {
        assert_eq!(extract_not_callable_type("totally unrelated message"), None);
    }

    #[test]
    fn scan_call_identifiers_skips_strings_and_comments() {
        let names = scan_call_identifiers(r#"foo(x) "bar(y)" # baz(z)"#);
        assert_eq!(names, vec!["foo"]);
    }
}
