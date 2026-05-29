//! Source-context enrichers, one per Julia error type.
//!
//! Each enricher takes the cleaned error message + the offending source
//! line and produces an optional `= note:` / `= help:` block that pins
//! the user's actual variable names + values to Julia's generic error.
//! The dispatcher in [`enrich_with_source_context`] routes by error
//! type. Errors without a registered enricher fall through to whatever
//! generic hint matched against `error_hints.toml`.

mod bounds;
mod dimension;
mod method_error;
mod not_callable;
mod parse_error;

use super::types::StructuredError;

// `format_raw` over in mod.rs uses the same "Error @ file:line:col"
// scanner that the parse-error enricher uses, so expose it via the
// enrichment module rather than duplicating.
pub(super) use parse_error::scan_error_location;

/// Single entry point — runs the enricher matched to the error type
/// against the cleaned source line. Returns `None` when the error type
/// has no enricher, when the source line is missing/empty, or when the
/// enricher itself decided it had nothing useful to add.
pub(super) fn enrich_with_source_context(err: &StructuredError) -> Option<String> {
    let src = err.source_line.trim();
    if src.is_empty() {
        return None;
    }

    match err.error_type.as_str() {
        "DimensionMismatch" => dimension::enrich(&err.message, src),
        "BoundsError" => bounds::enrich(&err.message, src),
        "MethodError" => method_error::enrich(&err.message, src, err),
        "ParseError" | "Meta.ParseError" => parse_error::enrich(&err.message, src),
        _ => None,
    }
}

// ─── Shared helper used by both dimension + bounds enrichers ─────────────────

/// Extract a variable name from an expression fragment.
/// `S_hat` → "`S_hat`", `func(x)` → "" (consumed past the name), `A'` → "A".
pub(super) fn extract_var_name(s: &str) -> String {
    let s = s.trim().trim_end_matches('\''); // strip transpose
    let bytes = s.as_bytes();
    if bytes.is_empty() || (!bytes[0].is_ascii_alphabetic() && bytes[0] != b'_') {
        return String::new();
    }
    let mut i = 0;
    while i < bytes.len()
        && (bytes[i].is_ascii_alphanumeric()
            || bytes[i] == b'_'
            || bytes[i] == b'.' // struct field access
            || bytes[i] == 0xCC // unicode combining (for things like x̂)
            || bytes[i] == 0xC3 // extended latin
            || bytes[i] > 127)
    // any non-ASCII (Julia unicode vars)
    {
        i += 1;
    }
    // Whole expr was a clean name — return it; otherwise take the leading slice.
    if i == bytes.len() {
        s.to_string()
    } else {
        let name = &s[..i];
        if name.is_empty() {
            String::new()
        } else {
            name.to_string()
        }
    }
}
