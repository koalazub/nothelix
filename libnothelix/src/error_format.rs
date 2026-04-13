//! Ergonomic error formatting for Julia cell execution errors.
//!
//! Transforms raw Julia exceptions into Rust-style guided messages:
//!
//! ```text
//! error[E001]: index out of bounds (0-indexed)
//!   --> cell 2, line 3
//!    |
//!  3 | v[0]
//!    |   ^ attempt to access 3-element Vector{Int64} at index [0]
//!    |
//!    = help: Julia arrays are 1-indexed. Use v[1] for the first element.
//! ```
//!
//! The hint registry is loaded from `error_hints.toml` (embedded at compile
//! time) and matched via regex. Contributors add hints by editing the TOML —
//! no Rust knowledge required.

use regex::Regex;
use serde::Deserialize;

// ─── Hint registry ──────────────────────────────────────────────────────────

static HINTS_TOML: &str = include_str!("../error_hints.toml");

#[derive(Deserialize)]
struct HintsFile {
    hint: Vec<RawHint>,
}

#[derive(Deserialize)]
struct RawHint {
    id: String,
    pattern: String,
    title: String,
    help: String,
}

pub struct ErrorHint {
    pub id: String,
    pub pattern: Regex,
    pub title: String,
    pub help: String,
}

pub fn load_hints() -> Vec<ErrorHint> {
    let file: HintsFile = toml::from_str(HINTS_TOML).unwrap_or(HintsFile { hint: vec![] });
    file.hint
        .into_iter()
        .filter_map(|h| {
            let pattern = Regex::new(&h.pattern).ok()?;
            Some(ErrorHint {
                id: h.id,
                pattern,
                title: h.title,
                help: h.help,
            })
        })
        .collect()
}

// ─── Structured error from Julia kernel ─────────────────────────────────────

#[derive(Deserialize, Default)]
pub struct StructuredError {
    #[serde(default)]
    pub error_type: String,
    #[serde(default)]
    pub message: String,
    #[serde(default)]
    pub frames: Vec<ErrorFrame>,
    #[serde(default)]
    pub source_line: String,
    #[serde(default)]
    pub cell_index: i64,
    #[serde(default)]
    pub cell_line: i64,
}

#[derive(Deserialize, Default)]
pub struct ErrorFrame {
    #[serde(default)]
    pub file: String,
    #[serde(default)]
    pub line: i64,
    #[serde(default)]
    pub func: String,
    #[serde(default)]
    pub is_user_code: bool,
}

// ─── Formatter ──────────────────────────────────────────────────────────────

/// Format a structured Julia error into a Rust-style guided message.
/// Falls back to a clean presentation if no hint matches.
pub fn format_error(error_json: &str, raw_error: &str) -> String {
    let hints = load_hints();

    // Try structured first
    if let Ok(err) = serde_json::from_str::<StructuredError>(error_json) {
        if !err.error_type.is_empty() {
            return format_structured(&err, &hints);
        }
    }

    // Fall back to raw error string with hint matching
    format_raw(raw_error, &hints)
}

fn format_structured(err: &StructuredError, hints: &[ErrorHint]) -> String {
    let full_msg = format!("{}: {}", err.error_type, err.message);
    let matched = find_hint(hints, &full_msg);

    let mut out = String::new();

    // Header
    match &matched {
        Some(h) => {
            let title = expand_captures(&h.title, &h.pattern, &full_msg);
            out.push_str(&format!("error[{}]: {}\n", h.id, title));
        }
        None => {
            out.push_str(&format!("error: {}\n", err.error_type));
        }
    }

    // Location
    if err.cell_index >= 0 {
        if err.cell_line > 0 {
            out.push_str(&format!("  --> cell {}, line {}\n", err.cell_index, err.cell_line));
        } else {
            out.push_str(&format!("  --> cell {}\n", err.cell_index));
        }
    }

    // Source line
    if !err.source_line.is_empty() {
        out.push_str("   |\n");
        if err.cell_line > 0 {
            out.push_str(&format!("{:>3} | {}\n", err.cell_line, err.source_line.trim_end()));
        } else {
            out.push_str(&format!("    | {}\n", err.source_line.trim_end()));
        }
        out.push_str(&format!("   | {}\n", err.message));
    } else {
        out.push_str("   |\n");
        out.push_str(&format!("   | {}\n", err.message));
    }
    out.push_str("   |\n");

    // Help
    if let Some(h) = &matched {
        if !h.help.is_empty() {
            let help = expand_captures(&h.help, &h.pattern, &full_msg);
            out.push_str(&format!("   = help: {}\n", help));
        }
    }

    // User frames only
    let user_frames: Vec<&ErrorFrame> = err.frames.iter().filter(|f| f.is_user_code).collect();
    let internal_count = err.frames.len() - user_frames.len();

    if !user_frames.is_empty() {
        out.push('\n');
        for f in &user_frames {
            out.push_str(&format!("   {} at {}:{}\n", f.func, f.file, f.line));
        }
    }
    if internal_count > 0 {
        out.push_str(&format!("   ... {} internal frames hidden\n", internal_count));
    }

    out
}

fn format_raw(raw: &str, hints: &[ErrorHint]) -> String {
    let matched = find_hint(hints, raw);
    let mut out = String::new();

    match &matched {
        Some(h) => {
            let title = expand_captures(&h.title, &h.pattern, raw);
            out.push_str(&format!("error[{}]: {}\n", h.id, title));
            out.push_str("   |\n");
            // Show first meaningful line of the raw error
            let first_line = raw.lines().next().unwrap_or(raw);
            out.push_str(&format!("   | {}\n", first_line));
            out.push_str("   |\n");
            if !h.help.is_empty() {
                let help = expand_captures(&h.help, &h.pattern, raw);
                out.push_str(&format!("   = help: {}\n", help));
            }
        }
        None => {
            out.push_str("error: execution failed\n");
            out.push_str("   |\n");
            for line in raw.lines().take(5) {
                out.push_str(&format!("   | {}\n", line));
            }
            if raw.lines().count() > 5 {
                out.push_str(&format!("   | ... ({} more lines)\n", raw.lines().count() - 5));
            }
            out.push_str("   |\n");
        }
    }

    out
}

fn find_hint<'a>(hints: &'a [ErrorHint], text: &str) -> Option<&'a ErrorHint> {
    hints.iter().find(|h| h.pattern.is_match(text))
}

/// Replace `{1}`, `{2}` etc. with regex capture groups from the match.
fn expand_captures(template: &str, pattern: &Regex, text: &str) -> String {
    let caps = match pattern.captures(text) {
        Some(c) => c,
        None => return template.to_string(),
    };
    let mut result = template.to_string();
    for i in 1..caps.len() {
        if let Some(m) = caps.get(i) {
            result = result.replace(&format!("{{{i}}}"), m.as_str());
        }
    }
    result
}

// ─── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hints_load() {
        let hints = load_hints();
        assert!(hints.len() >= 15, "expected at least 15 hints, got {}", hints.len());
        assert_eq!(hints[0].id, "E001");
    }

    #[test]
    fn bounds_error_zero_index() {
        let raw = "BoundsError: attempt to access 3-element Vector{Int64} at index [0]";
        let out = format_raw(raw, &load_hints());
        assert!(out.contains("E001"), "should match E001");
        assert!(out.contains("1-indexed"), "should mention 1-indexed");
    }

    #[test]
    fn undef_var_captures_name() {
        let raw = "UndefVarError: myvar not defined";
        let out = format_raw(raw, &load_hints());
        assert!(out.contains("E003"), "should match E003");
        assert!(out.contains("myvar"), "should capture variable name");
    }

    #[test]
    fn unknown_error_fallback() {
        let raw = "SomeWeirdError: never seen this before";
        let out = format_raw(raw, &load_hints());
        assert!(out.contains("error: execution failed"));
        assert!(out.contains("SomeWeirdError"));
    }

    #[test]
    fn structured_error_formatting() {
        let json = r#"{
            "error_type": "BoundsError",
            "message": "attempt to access 3-element Vector{Int64} at index [0]",
            "frames": [
                {"file": "<cell>", "line": 3, "func": "top-level scope", "is_user_code": true},
                {"file": "array.jl", "line": 861, "func": "getindex", "is_user_code": false}
            ],
            "source_line": "v[0]",
            "cell_index": 2,
            "cell_line": 3
        }"#;
        let out = format_error(json, "");
        assert!(out.contains("error[E001]"));
        assert!(out.contains("cell 2, line 3"));
        assert!(out.contains("v[0]"));
        assert!(out.contains("1-indexed"));
        assert!(out.contains("1 internal frames hidden"));
    }
}
