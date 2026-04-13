//! Ergonomic error formatting for Julia cell execution.
//!
//! Transforms raw Julia errors into Rust-style guided messages with
//! examples showing how to fix the problem.

use regex::Regex;
use serde::Deserialize;

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
    #[serde(default)]
    example: String,
}

pub struct ErrorHint {
    pub id: String,
    pub pattern: Regex,
    pub title: String,
    pub help: String,
    pub example: String,
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
                example: h.example,
            })
        })
        .collect()
}

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

/// Format a Julia error into a guided message.
pub fn format_error(error_json: &str, raw_error: &str) -> String {
    let hints = load_hints();

    if let Ok(err) = serde_json::from_str::<StructuredError>(error_json) {
        if !err.error_type.is_empty() {
            return format_structured(&err, &hints);
        }
    }

    if !raw_error.is_empty() {
        return format_raw(raw_error, &hints);
    }

    "error: unknown\n".to_string()
}

fn format_structured(err: &StructuredError, hints: &[ErrorHint]) -> String {
    let full_msg = clean_message(&format!("{}: {}", err.error_type, err.message));
    let matched = find_hint(hints, &full_msg);
    let mut out = String::new();

    // Header
    match &matched {
        Some(h) => {
            let title = expand(&h.title, &h.pattern, &full_msg);
            out.push_str(&format!("error[{}]: {}\n", h.id, title));
        }
        None => out.push_str(&format!("error: {}\n", err.error_type)),
    }

    // Location
    if err.cell_index >= 0 && err.cell_line > 0 {
        out.push_str(&format!("  --> cell {}, line {}\n", err.cell_index, err.cell_line));
    } else if err.cell_index >= 0 {
        out.push_str(&format!("  --> cell {}\n", err.cell_index));
    }

    // Source line + message
    out.push_str("   |\n");
    if !err.source_line.is_empty() && err.cell_line > 0 {
        out.push_str(&format!("{:>3} | {}\n", err.cell_line, err.source_line.trim_end()));
    }
    // Short message (first line only, no "Closest candidates")
    let short_msg = err.message.lines().next().unwrap_or(&err.message);
    out.push_str(&format!("   | {}\n", short_msg));
    out.push_str("   |\n");

    // Help + example
    if let Some(h) = &matched {
        if !h.help.is_empty() {
            out.push_str(&format!("   = help: {}\n", expand(&h.help, &h.pattern, &full_msg)));
        }
        if !h.example.is_empty() {
            let ex = expand(&h.example, &h.pattern, &full_msg);
            out.push_str("   = example:\n");
            for line in ex.lines() {
                out.push_str(&format!("   |   {}\n", line));
            }
        }
    }

    out
}

fn format_raw(raw: &str, hints: &[ErrorHint]) -> String {
    let cleaned = clean_message(raw);
    let matched = find_hint(hints, &cleaned);
    let mut out = String::new();

    match &matched {
        Some(h) => {
            let title = expand(&h.title, &h.pattern, &cleaned);
            out.push_str(&format!("error[{}]: {}\n", h.id, title));
            out.push_str("   |\n");
            // First meaningful line only
            let first = cleaned.lines().next().unwrap_or(&cleaned);
            out.push_str(&format!("   | {}\n", first));
            out.push_str("   |\n");
            if !h.help.is_empty() {
                out.push_str(&format!("   = help: {}\n", expand(&h.help, &h.pattern, &cleaned)));
            }
            if !h.example.is_empty() {
                let ex = expand(&h.example, &h.pattern, &cleaned);
                out.push_str("   = example:\n");
                for line in ex.lines() {
                    out.push_str(&format!("   |   {}\n", line));
                }
            }
        }
        None => {
            out.push_str("error: execution failed\n");
            out.push_str("   |\n");
            // Show only the first meaningful line, not the candidates noise
            let first = cleaned.lines().next().unwrap_or(&cleaned);
            out.push_str(&format!("   | {}\n", first));
            out.push_str("   |\n");
        }
    }

    out
}

/// Strip "Closest candidates" and everything after — pure noise for researchers.
fn clean_message(msg: &str) -> String {
    if let Some(idx) = msg.find("\nClosest candidates") {
        msg[..idx].trim_end().to_string()
    } else if let Some(idx) = msg.find("\n\nClosest candidates") {
        msg[..idx].trim_end().to_string()
    } else if let Some(idx) = msg.find("Closest candidates are:") {
        msg[..idx].trim_end().to_string()
    } else {
        // Also strip "Stacktrace:" sections
        if let Some(idx) = msg.find("\nStacktrace:") {
            msg[..idx].trim_end().to_string()
        } else {
            msg.to_string()
        }
    }
}

fn find_hint<'a>(hints: &'a [ErrorHint], text: &str) -> Option<&'a ErrorHint> {
    hints.iter().find(|h| h.pattern.is_match(text))
}

fn expand(template: &str, pattern: &Regex, text: &str) -> String {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hints_load() {
        let hints = load_hints();
        assert!(hints.len() >= 15);
    }

    #[test]
    fn bounds_error_zero() {
        let raw = "BoundsError: attempt to access 3-element Vector{Int64} at index [0]";
        let out = format_raw(raw, &load_hints());
        assert!(out.contains("E001"));
        assert!(out.contains("1-indexed"));
        assert!(out.contains("v[1]"));
    }

    #[test]
    fn undef_var() {
        let raw = "UndefVarError: myvar not defined";
        let out = format_raw(raw, &load_hints());
        assert!(out.contains("myvar"));
        assert!(out.contains("E004"));
    }

    #[test]
    fn method_error_function_as_arg() {
        let raw = "MethodError: no method matching /(::Int64, ::typeof(sqrt))";
        let out = format_raw(raw, &load_hints());
        assert!(out.contains("E005"));
        assert!(out.contains("sqrt"));
        assert!(out.contains("function"));
        assert!(out.contains("parentheses"));
    }

    #[test]
    fn closest_candidates_stripped() {
        let raw = "MethodError: no method matching /(::Int64, ::typeof(sqrt))\nClosest candidates are:\n  /(::R, !Matched::S)\n  lots of noise";
        let out = format_raw(raw, &load_hints());
        assert!(!out.contains("Closest candidates"));
        assert!(!out.contains("!Matched"));
    }

    #[test]
    fn unknown_falls_back_cleanly() {
        let raw = "SomeNewError: never seen this";
        let out = format_raw(raw, &load_hints());
        assert!(out.contains("error: execution failed"));
        assert!(out.contains("SomeNewError"));
    }

    #[test]
    fn structured_with_example() {
        let json = r#"{
            "error_type": "BoundsError",
            "message": "attempt to access 3-element Vector{Int64} at index [0]",
            "frames": [],
            "source_line": "v[0]",
            "cell_index": 2,
            "cell_line": 3
        }"#;
        let out = format_error(json, "");
        assert!(out.contains("error[E001]"));
        assert!(out.contains("cell 2, line 3"));
        assert!(out.contains("v[0]"));
        assert!(out.contains("example"));
        assert!(out.contains("v[1]"));
    }
}
