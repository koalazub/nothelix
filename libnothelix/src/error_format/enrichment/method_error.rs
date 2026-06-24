//! `MethodError` enricher.
//!
//! Maps Julia's "no method matching `f(::T1, ::T2)`" to the user's
//! actual argument names + emits kernel-aware suggestions:
//!
//! ```text
//! = note: argument types:
//! |   `n` is Int64
//! |   `Float64` is Type{Float64}
//! = help: check types with: typeof(n), typeof(Float64)
//! ```
//!
//! Plus, when `in_scope_variable_types` is populated by the kernel:
//!
//! ```text
//! = scope: in-scope variables by type:
//! |   Int64: `n` (cell 17), `k` (cell 17)
//! = candidates: `similar()` accepts these in-scope values:
//! |   `arr` (Vector{Float64}) — cell 12
//! ```
//!
//! Delegates to the `not_callable` enricher for the "objects of type X
//! are not callable" shape (a `MethodError` that wants a totally
//! different angle of attack).

use std::fmt::Write;

use super::super::scanners::{find_matching_paren, split_top_level_commas};
use super::super::types::StructuredError;
use super::not_callable;

pub(super) fn enrich(message: &str, source: &str, err: &StructuredError) -> Option<String> {
    if message.contains("not callable") {
        return not_callable::enrich(message, source, err);
    }

    // Julia writes two callable shapes after "matching ":
    //   "matching funcname(::T1, ::T2)"   — ordinary calls
    //   "matching (TypeName)(::T1, ::T2)" — type constructors (Matrix, Float64…)
    // The constructor form wraps the callable in parens; this used to
    // confuse find('(') and the empty slice before it became the "name".
    let func_name = {
        let idx = message.find("matching ")?;
        let after = &message[idx + 9..];
        let bytes = after.as_bytes();
        if bytes.first() == Some(&b'(') {
            let close = find_matching_paren(bytes, 0)?;
            let name = after[1..close].trim();
            if name.is_empty() {
                return None;
            }
            name.to_string()
        } else {
            let end = after.find('(')?;
            let name = after[..end].trim();
            if name.is_empty() {
                return None;
            }
            name.to_string()
        }
    };

    let arg_types = scan_types_from_call(message);
    let arg_exprs = scan_call_args(source, &func_name);
    if arg_types.is_empty() || arg_exprs.is_empty() {
        return None;
    }

    let mut out = String::new();

    let pairs: Vec<_> = arg_exprs.iter().zip(arg_types.iter()).collect();
    if !pairs.is_empty() {
        out.push_str("   = note: argument types:\n");
        for (expr, typ) in &pairs {
            let _ = writeln!(out, "   |   `{expr}` is {typ}");
        }
    }
    let _ = write!(out, "   = help: check types with: ");
    let checks: Vec<String> = arg_exprs.iter().map(|a| format!("typeof({a})")).collect();
    out.push_str(&checks.join(", "));
    out.push('\n');

    // Kernel-powered hints: for each ::T in the error, surface every
    // in-scope variable of that type with cell attribution.
    let mut any_scope_hint = false;
    for typ in &arg_types {
        if let Some(entries) = err.in_scope_variable_types.get(typ) {
            if entries.is_empty() {
                continue;
            }
            if !any_scope_hint {
                out.push_str("   = scope: in-scope variables by type:\n");
                any_scope_hint = true;
            }
            let names: Vec<String> = entries
                .iter()
                .map(|e| {
                    if e.cell >= 0 {
                        format!("`{}` (cell {})", e.name, e.cell)
                    } else {
                        format!("`{}`", e.name)
                    }
                })
                .collect();
            let _ = writeln!(out, "   |   {typ}: {}", names.join(", "));
        }
    }

    if !err.method_candidates.is_empty() {
        let _ = writeln!(
            out,
            "   = candidates: `{func_name}()` accepts these in-scope values:"
        );
        for c in &err.method_candidates {
            if c.cell >= 0 {
                let _ = writeln!(
                    out,
                    "   |   `{}` ({}) — cell {}",
                    c.name, c.type_name, c.cell
                );
            } else {
                let _ = writeln!(out, "   |   `{}` ({})", c.name, c.type_name);
            }
        }
    }

    Some(out)
}

/// Extract type names from a `MethodError` call signature.
/// "matching similar(::Int64, ::Type{Float64})" → ["Int64", "Type{Float64}"]
fn scan_types_from_call(msg: &str) -> Vec<String> {
    let Some(start) = msg.find("matching ").map(|i| i + 9) else {
        return Vec::new();
    };
    let rest = &msg[start..];
    let rest_bytes = rest.as_bytes();

    // For type-constructor errors Julia emits `(Name)(::ArgT)`; the first
    // paren group is the callable, not the argument list. Skip past it.
    let after_name = if rest_bytes.first() == Some(&b'(') {
        match find_matching_paren(rest_bytes, 0) {
            Some(close) => close + 1,
            None => return Vec::new(),
        }
    } else {
        0
    };
    let tail = &rest[after_name..];
    let tail_bytes = tail.as_bytes();
    let Some(paren_open) = tail.find('(') else {
        return Vec::new();
    };
    let Some(paren_close) = find_matching_paren(tail_bytes, paren_open) else {
        return Vec::new();
    };
    let args_str = &tail[paren_open + 1..paren_close];

    let mut types = Vec::new();
    for part in args_str.split("::") {
        let trimmed = part.trim().trim_end_matches([',', ' ']);
        if !trimmed.is_empty() {
            types.push(trimmed.to_string());
        }
    }
    types
}

/// Extract argument expressions from a function call in source code.
/// "B = similar(n, Float64)" with func "similar" → ["n", "Float64"]
fn scan_call_args(source: &str, func: &str) -> Vec<String> {
    let Some(after_func) = source.find(func).map(|i| &source[i + func.len()..]) else {
        return Vec::new();
    };
    let Some(paren_open) = after_func.find('(') else {
        return Vec::new();
    };
    let Some(paren_close) = find_matching_paren(after_func.as_bytes(), paren_open) else {
        return Vec::new();
    };
    split_top_level_commas(&after_func[paren_open + 1..paren_close])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scan_types_from_call_handles_parenthesized_name() {
        let types = scan_types_from_call("matching (Matrix)(::Vector{Float64})");
        assert_eq!(types, vec!["Vector{Float64}"]);
    }

    #[test]
    fn scan_types_from_call_handles_plain_function_name() {
        let types = scan_types_from_call("matching similar(::Int64, ::Type{Float64})");
        assert_eq!(types, vec!["Int64", "Type{Float64}"]);
    }

    #[test]
    fn scan_call_args_extracts_expressions() {
        let args = scan_call_args("B = similar(n, Float64)", "similar");
        assert_eq!(args, vec!["n", "Float64"]);
    }
}
