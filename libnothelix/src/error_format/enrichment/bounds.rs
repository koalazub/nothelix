//! `BoundsError` enricher.
//!
//! Replaces Julia's bare "BoundsError: attempt to access N-element …
//! at index [K]" with a note naming the actual indexed variable
//! (extracted from the source line) and reminding the user of the
//! valid index range. The valid-index range is Julia-style 1-based.

use std::fmt::Write;

use super::extract_var_name;

pub(super) fn enrich(message: &str, source: &str) -> Option<String> {
    let n = scan_element_count(message)?;
    let indexed = scan_indexed_var(source)?;

    let mut out = String::new();
    let _ = writeln!(
        out,
        "   = note: `{indexed}` has {n} elements (valid indices: 1 to {n})"
    );
    Some(out)
}

/// Extract element count from "N-element" in a `BoundsError` message.
fn scan_element_count(msg: &str) -> Option<String> {
    let idx = msg.find("-element")?;
    let before = &msg[..idx];
    let start = before
        .rfind(|c: char| !c.is_ascii_digit())
        .map_or(0, |i| i + 1);
    let num = &before[start..];
    if num.is_empty() {
        None
    } else {
        Some(num.to_string())
    }
}

/// Find the variable being indexed in source like "arr[i]" → "arr".
fn scan_indexed_var(source: &str) -> Option<String> {
    let bracket = source.find('[')?;
    let before = source[..bracket].trim();
    let name = extract_var_name(before);
    if name.is_empty() {
        None
    } else {
        Some(name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn enrich_names_indexed_var_and_length() {
        let out = enrich(
            "BoundsError: attempt to access 5-element Vector{Int64} at index [9]",
            "arr[9]",
        )
        .unwrap();
        assert!(out.contains("`arr` has 5 elements"), "got:\n{out}");
        assert!(out.contains("valid indices: 1 to 5"), "got:\n{out}");
    }

    #[test]
    fn enrich_returns_none_without_indexed_var() {
        assert!(enrich("5-element Vector at index [1]", "println(\"hi\")").is_none());
    }

    #[test]
    fn enrich_returns_none_without_element_count() {
        assert!(enrich("some other error", "arr[1]").is_none());
    }
}
