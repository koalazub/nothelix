use std::fmt::Write;

use super::super::scan::{find_matching_paren, split_top_level_commas};
use super::super::types::StructuredError;
use super::not_callable;

const MATCHING: &str = "matching ";

pub(super) fn enrich(message: &str, source: &str, err: &StructuredError) -> Option<String> {
    if message.contains("not callable") {
        return not_callable::enrich(message, source, err);
    }

    let func_name = scan_called_name(message)?;
    let arg_types = scan_types_from_call(message);
    if arg_types.is_empty() {
        return None;
    }
    let arg_exprs = scan_call_args(source, &func_name);

    let mut out = String::new();
    if count_call_sites(source, &func_name) == 1 && arg_exprs.len() == arg_types.len() {
        write_paired_args(&mut out, &arg_exprs, &arg_types);
    } else {
        write_type_only(&mut out, &func_name, &arg_types);
    }
    write_scope_hints(&mut out, &arg_types, err);
    write_candidates(&mut out, &func_name, err);
    Some(out)
}

fn write_paired_args(out: &mut String, arg_exprs: &[String], arg_types: &[String]) {
    out.push_str("   = note: argument types:\n");
    for (expr, arg_type) in arg_exprs.iter().zip(arg_types) {
        let _ = writeln!(out, "   |   `{expr}` is {arg_type}");
    }
    let checks: Vec<String> = arg_exprs
        .iter()
        .map(|expr| format!("typeof({expr})"))
        .collect();
    let _ = writeln!(out, "   = help: check types with: {}", checks.join(", "));
}

fn write_type_only(out: &mut String, func_name: &str, arg_types: &[String]) {
    let types = arg_types.join(", ");
    let _ = writeln!(out, "   = note: `{func_name}` got argument types ({types})");
    let _ = writeln!(
        out,
        "   = help: no `{func_name}` method accepts ({types}); check each argument's type and value"
    );
}

fn write_scope_hints(out: &mut String, arg_types: &[String], err: &StructuredError) {
    let mut opened = false;
    for arg_type in arg_types {
        let Some(entries) = err
            .in_scope_variable_types
            .get(arg_type)
            .filter(|entries| !entries.is_empty())
        else {
            continue;
        };
        if !opened {
            out.push_str("   = scope: in-scope variables by type:\n");
            opened = true;
        }
        let names: Vec<String> = entries
            .iter()
            .map(|entry| {
                if entry.cell >= 0 {
                    format!("`{}` (cell {})", entry.name, entry.cell)
                } else {
                    format!("`{}`", entry.name)
                }
            })
            .collect();
        let _ = writeln!(out, "   |   {arg_type}: {}", names.join(", "));
    }
}

fn write_candidates(out: &mut String, func_name: &str, err: &StructuredError) {
    if err.method_candidates.is_empty() {
        return;
    }
    let _ = writeln!(
        out,
        "   = candidates: `{func_name}()` accepts these in-scope values:"
    );
    for candidate in &err.method_candidates {
        if candidate.cell >= 0 {
            let _ = writeln!(
                out,
                "   |   `{}` ({}) — cell {}",
                candidate.name, candidate.type_name, candidate.cell
            );
        } else {
            let _ = writeln!(out, "   |   `{}` ({})", candidate.name, candidate.type_name);
        }
    }
}

fn scan_called_name(message: &str) -> Option<String> {
    let after = after_matching(message)?;
    let name = if after.as_bytes().first() == Some(&b'(') {
        &after[1..find_matching_paren(after.as_bytes(), 0)?]
    } else {
        &after[..after.find('(')?]
    };
    let name = name.trim();
    (!name.is_empty()).then(|| name.to_string())
}

fn scan_types_from_call(msg: &str) -> Vec<String> {
    let Some(after) = after_matching(msg) else {
        return Vec::new();
    };
    let bytes = after.as_bytes();
    let args_start = if bytes.first() == Some(&b'(') {
        match find_matching_paren(bytes, 0) {
            Some(close) => close + 1,
            None => return Vec::new(),
        }
    } else {
        0
    };

    let tail = &after[args_start..];
    let Some(open) = tail.find('(') else {
        return Vec::new();
    };
    let Some(close) = find_matching_paren(tail.as_bytes(), open) else {
        return Vec::new();
    };

    tail[open + 1..close]
        .split("::")
        .filter_map(|part| {
            let trimmed = part.trim().trim_end_matches([',', ' ']);
            (!trimmed.is_empty()).then(|| trimmed.to_string())
        })
        .collect()
}

fn after_matching(msg: &str) -> Option<&str> {
    msg.find(MATCHING).map(|idx| &msg[idx + MATCHING.len()..])
}

fn scan_call_args(source: &str, func: &str) -> Vec<String> {
    let Some(after_func) = source.find(func).map(|i| &source[i + func.len()..]) else {
        return Vec::new();
    };
    let Some(open) = after_func.find('(') else {
        return Vec::new();
    };
    let Some(close) = find_matching_paren(after_func.as_bytes(), open) else {
        return Vec::new();
    };
    split_top_level_commas(&after_func[open + 1..close])
}

fn count_call_sites(source: &str, func: &str) -> usize {
    if func.is_empty() {
        return 0;
    }
    let bytes = source.as_bytes();
    let mut count = 0;
    let mut start = 0;
    while let Some(rel) = source[start..].find(func) {
        let at = start + rel;
        let left_ok = at == 0 || {
            let previous = bytes[at - 1];
            !(previous.is_ascii_alphanumeric() || previous == b'_')
        };
        let right_ok = source[at + func.len()..].trim_start().starts_with('(');
        if left_ok && right_ok {
            count += 1;
        }
        start = at + func.len();
    }
    count
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
    fn scan_called_name_unwraps_type_constructors() {
        assert_eq!(
            scan_called_name("matching (Matrix)(::Vector{Float64})").as_deref(),
            Some("Matrix")
        );
        assert_eq!(
            scan_called_name("matching similar(::Int64)").as_deref(),
            Some("similar")
        );
    }

    #[test]
    fn scan_call_args_extracts_expressions() {
        let args = scan_call_args("B = similar(n, Float64)", "similar");
        assert_eq!(args, vec!["n", "Float64"]);
    }

    #[test]
    fn enrich_emits_type_note_when_source_missing() {
        let err = StructuredError::default();
        let out = enrich("MethodError: no method matching add(::Module)", "", &err)
            .expect("should enrich from the type signature alone");
        assert!(
            out.contains("`add` got argument types (Module)"),
            "got: {out}"
        );
        assert!(
            out.contains("no `add` method accepts (Module)"),
            "got: {out}"
        );
    }

    #[test]
    fn enrich_pairs_args_with_types_when_source_present() {
        let err = StructuredError::default();
        let out = enrich(
            "MethodError: no method matching similar(::Int64, ::Type{Float64})",
            "B = similar(n, Float64)",
            &err,
        )
        .expect("should enrich with paired note");
        assert!(out.contains("`n` is Int64"), "got: {out}");
        assert!(out.contains("typeof(n), typeof(Float64)"), "got: {out}");
    }

    #[test]
    fn enrich_uses_type_note_for_ambiguous_multi_call_line() {
        let err = StructuredError::default();
        let source = "Pkg.add(\"FFTW\"); Pkg.add(DSP); Pkg.add(\"Plots\")";
        let out = enrich(
            "MethodError: no method matching add(::Module)",
            source,
            &err,
        )
        .expect("ambiguous line still enriches via type-only note");
        assert!(
            out.contains("`add` got argument types (Module)"),
            "got: {out}"
        );
        assert!(
            !out.contains("= note: argument types:"),
            "must not emit a misleading paired note for an ambiguous line: {out}"
        );
    }
}
