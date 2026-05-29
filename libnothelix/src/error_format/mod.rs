//! Ergonomic error formatting for Julia cell execution.
//!
//! Transforms raw Julia errors into Rust-compiler-style guided messages
//! with source context, call chains, and fix examples.
//!
//! Error matching uses token-based classification (no regex). An error
//! message is scanned into `ErrorTokens` (error type, function name,
//! type arguments, keywords), then each hint declares required/excluded
//! tokens for matching. Highest-specificity hint wins.

use std::fmt::Write;
use std::sync::OnceLock;

mod hints;
mod scanners;
mod tokenize;
mod types;
mod util;

use hints::{hints, ErrorHint};
use tokenize::{tokenize_error, ErrorTokens};
pub use types::{ErrorFrame, MethodCandidate, ScopeVarEntry, StructuredError, VarContext};
use util::{build_call_chain, clean_message, truncate, wrap};
#[cfg(test)]
use util::clean_path;

// ─── Hint matching ───────────────────────────────────────────────────────────

/// Find the best-matching hint for the given tokens.
/// Scores by specificity: more matching constraints = higher score.
fn find_hint<'a>(hints: &'a [ErrorHint], tokens: &ErrorTokens) -> Option<&'a ErrorHint> {
    let full_text = format!("{}: {}", tokens.error_type, tokens.message);
    let mut best: Option<(isize, &ErrorHint)> = None;

    for hint in hints {
        if !hint.match_type.is_empty() && !tokens.error_type.contains(&hint.match_type) {
            continue;
        }
        if !hint.selector.matches(&full_text) {
            continue;
        }

        // Priority dominates (×1000) → match_type presence (+10) →
        // constraint count as specificity.
        let score = hint.priority as isize * 1000
            + if hint.match_type.is_empty() { 0 } else { 10 }
            + hint.selector.specificity() as isize;

        if best.is_none_or(|(s, _)| score > s) {
            best = Some((score, hint));
        }
    }

    best.map(|(_, h)| h)
}

/// Expand template placeholders using extracted tokens.
/// - {func} → extracted function name
/// - {type1}, {type2} → extracted type names
/// - {var} → first word of message (for `UndefVarError`)
fn expand_template(template: &str, tokens: &ErrorTokens) -> String {
    let mut result = template.to_string();

    if result.contains("{func}") {
        result = result.replace("{func}", &tokens.func_name);
    }

    for (i, t) in tokens.types.iter().enumerate() {
        let placeholder = format!("{{type{}}}", i + 1);
        result = result.replace(&placeholder, t);
    }

    // {var} → first word of the message (for UndefVarError: varname not defined)
    if result.contains("{var}") {
        let var = tokens
            .message
            .split_whitespace()
            .next()
            .unwrap_or("")
            .trim_matches('`');
        result = result.replace("{var}", var).to_string();
    }

    result
}

// ─── Source context enrichment ────────────────────────────────────────────────

/// Extract concrete variable names and dimensions from the error + source line.
/// Returns a `= note:` block using the user's actual variable names instead of
/// generic placeholders.
fn enrich_with_source_context(err: &StructuredError) -> Option<String> {
    let src = err.source_line.trim();
    if src.is_empty() {
        return None;
    }

    match err.error_type.as_str() {
        "DimensionMismatch" => enrich_dimension_mismatch(&err.message, src),
        "BoundsError" => enrich_bounds_error(&err.message, src),
        "MethodError" => enrich_method_error(&err.message, src, err),
        "ParseError" | "Meta.ParseError" => enrich_parse_error(&err.message, src),
        _ => None,
    }
}

/// For `DimensionMismatch`: extract operand names from source line and dimensions
/// from the message, render a concrete note.
///
/// Message: "a has size (8, 8), b has size (5, 5), mismatch at dim 1"
/// Source:  "result = `S_hat` * K"
/// Output:  "   = note: `S_hat` is (8, 8), K is (5, 5) — need `size(S_hat`, 2) == size(K, 1)"
fn enrich_dimension_mismatch(message: &str, source: &str) -> Option<String> {
    // Extract dimensions from message: look for "size (M, N)" or "(M, N)" patterns
    let dims = scan_dimension_pairs(message);

    // Extract operands from source line by splitting on operator
    let operands = scan_binary_operands(source);

    if dims.len() < 2 || operands.is_empty() {
        return None;
    }

    let mut out = String::new();

    // Map operands to dimensions
    let lhs = &operands[0];
    let rhs = if operands.len() > 1 {
        &operands[1]
    } else {
        return None;
    };

    let _ = write!(
        out,
        "   = note: `{lhs}` is {}, `{rhs}` is {}",
        dims[0], dims[1]
    );

    // Try to explain the mismatch
    if message.contains("mismatch at dim") {
        // Matrix multiply: inner dimensions must agree
        let _ = write!(
            out,
            " — need size({lhs}, 2) == size({rhs}, 1)"
        );
    }
    out.push('\n');

    // Concrete example with their variable names
    let _ = write!(out, "   = help: check with: size({lhs}), size({rhs})");
    out.push('\n');

    Some(out)
}

/// For `BoundsError` with source context: show what the user was indexing.
fn enrich_bounds_error(message: &str, source: &str) -> Option<String> {
    // Extract the array length from "N-element"
    let n = scan_element_count(message)?;

    // Find what's being indexed in the source (e.g., "arr[i]" → "arr")
    let indexed = scan_indexed_var(source)?;

    let mut out = String::new();
    let _ = write!(out, "   = note: `{indexed}` has {n} elements (valid indices: 1 to {n})");
    out.push('\n');
    Some(out)
}

/// For `MethodError`: extract the function call from the source line, map
/// the types from the error message to the actual arguments.
///
/// Message: "no method matching `similar(::Int64`, `::Type{Float64`})"
/// Source:  "B = similar(n, Float64)"
/// Output:  "   = note: `n` is Int64 — `similar()` expects an array as the first argument"
///          "   = help: try: `similar(your_array`, Float64, `size(your_array)`)"
fn enrich_method_error(message: &str, source: &str, err: &StructuredError) -> Option<String> {
    // "objects of type T are not callable" is a distinct shape from
    // "no method matching f(::T)". Route it separately so we can give
    // the user a concrete answer — "you called a value, here's where
    // that value was assigned" — instead of silently dropping through
    // to the generic hint about missing operators.
    if message.contains("not callable") {
        return enrich_not_callable(message, source, err);
    }

    // Extract the function name from the message. Julia writes two forms:
    //   "matching funcname(::T1, ::T2)"   — ordinary calls
    //   "matching (TypeName)(::T1, ::T2)" — type constructors (Matrix, Float64…)
    // The constructor form wraps the callable in parens, which used to
    // confuse this parser: `find('(')` returned 0, the empty slice before
    // it became the "name", and the whole enricher bailed, leaving the
    // user with "no method `` for these argument types".
    let func_name = {
        let idx = message.find("matching ")?;
        let after = &message[idx + 9..];
        let bytes = after.as_bytes();
        if bytes.first() == Some(&b'(') {
            let close = find_matching_close(bytes, 0)?;
            let name = after[1..close].trim();
            if name.is_empty() { return None; }
            name.to_string()
        } else {
            let end = after.find('(')?;
            let name = after[..end].trim();
            if name.is_empty() { return None; }
            name.to_string()
        }
    };

    // Extract argument types from the message: "matching func(::Type1, ::Type2)"
    let arg_types = scan_types_from_call(message);

    // Extract argument expressions from the source line: "func(arg1, arg2)"
    let arg_exprs = scan_call_args(source, &func_name);

    if arg_types.is_empty() || arg_exprs.is_empty() {
        return None;
    }

    let mut out = String::new();

    // Map each argument to its type
    let pairs: Vec<_> = arg_exprs.iter().zip(arg_types.iter()).collect();
    if !pairs.is_empty() {
        out.push_str("   = note: argument types:\n");
        for (expr, typ) in &pairs {
            let _ = writeln!(out, "   |   `{expr}` is {typ}");
        }
    }

    // Provide concrete fix using their variable names
    let _ = write!(out, "   = help: check types with: ");
    let checks: Vec<String> = arg_exprs.iter().map(|a| format!("typeof({a})")).collect();
    out.push_str(&checks.join(", "));
    out.push('\n');

    // Kernel-powered type hints. When the kernel is running and the
    // user has executed at least one cell, each ::T in the error
    // signature can point at the in-scope variable that actually has
    // that type — so instead of "`C_inv = inv(Matrix(C))` failed" the
    // user sees "`C` (a Vector{ComplexF64} from cell 17, line 5) was
    // passed, but `C1` (a Circulant from cell 17) would work".
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

/// Render a "not callable" `MethodError` — the Julia parser's way of
/// saying "you called a name, but that name is bound to a value (array,
/// number, …), not a function".
///
/// The generic E043 hint suggests `2(x+1)` → `2*(x+1)`, which is one
/// valid cause but misleading when the real issue is "X is a Vector from
/// cell 17; stop trying to call it like a function". When the kernel
/// has populated `in_scope_variable_types`, we can pinpoint: the failing
/// value's type is in the message, and any in-scope binding of that
/// type is a plausible culprit with cell-anchored attribution.
fn enrich_not_callable(message: &str, source: &str, err: &StructuredError) -> Option<String> {
    let obj_type = extract_not_callable_type(message)?;

    let mut out = String::new();
    let _ = writeln!(out, "   = note: called something of type {obj_type} (not a function)");

    // Name the call site from the source line when we can. Julia's
    // message only gives us the type, not the identifier — we look at
    // the source for `ident(...)` patterns and intersect with the
    // scope map to point at the right one.
    let call_names = scan_call_identifiers(source);
    let typed_vars = err.in_scope_variable_types.get(&obj_type);

    if let Some(entries) = typed_vars {
        let matching: Vec<&ScopeVarEntry> = entries
            .iter()
            .filter(|e| call_names.iter().any(|c| c == &e.name))
            .collect();
        if !matching.is_empty() {
            out.push_str("   = scope: the call site likely resolves to:\n");
            for e in matching {
                let _ = writeln!(
                    out,
                    "   |   `{}` — {obj_type} assigned in cell {}",
                    e.name, e.cell
                );
            }
            out.push_str("   = help: that name is a value, not a function. Index it with `[…]` or pick a different name for your function.\n");
        } else {
            // Fall back to listing all in-scope values of that type;
            // user can eyeball which one matches.
            let _ = writeln!(out, "   = scope: in-scope values of type {obj_type}:");
            for e in entries {
                let _ = writeln!(out, "   |   `{}` — cell {}", e.name, e.cell);
            }
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
/// Returns the type string (e.g. `Vector{ComplexF64}`) or None if the
/// message shape doesn't match.
fn extract_not_callable_type(msg: &str) -> Option<String> {
    let start = msg.find("objects of type ")?;
    let after = &msg[start + "objects of type ".len()..];
    let end = after.find(" are not callable")?;
    let t = after[..end].trim();
    if t.is_empty() { None } else { Some(t.to_string()) }
}

/// Find every `identifier(` call in a source line. Skips string content
/// and stops at `#` comments. Used to narrow "not callable" enrichment
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

/// Extract type names from a `MethodError` call signature.
/// "matching `similar(::Int64`, `::Type{Float64`})" → ["Int64", "Type{Float64}"]
fn scan_types_from_call(msg: &str) -> Vec<String> {
    let start = match msg.find("matching ") {
        Some(i) => i + 9,
        None => return Vec::new(),
    };
    let rest = &msg[start..];
    let rest_bytes = rest.as_bytes();

    // For type-constructor errors Julia emits `(Name)(::ArgT)`; the first
    // paren group is the callable, not the argument list. Skip past it so
    // we read the right parens for the arg types.
    let after_name = if rest_bytes.first() == Some(&b'(') {
        match find_matching_close(rest_bytes, 0) {
            Some(close) => close + 1,
            None => return Vec::new(),
        }
    } else {
        0
    };
    let tail = &rest[after_name..];
    let tail_bytes = tail.as_bytes();
    let paren_open = match tail.find('(') {
        Some(i) => i,
        None => return Vec::new(),
    };
    let paren_close = match find_matching_close(tail_bytes, paren_open) {
        Some(c) => c,
        None => return Vec::new(),
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

/// Thin alias for the parenthesis-specialised matcher in `scanners`,
/// kept so older callsites read naturally.
fn find_matching_close(bytes: &[u8], open_idx: usize) -> Option<usize> {
    scanners::find_matching_paren(bytes, open_idx)
}

/// Extract argument expressions from a function call in source code.
/// Source: "B = similar(n, Float64)" with func "similar" → ["n", "Float64"]
fn scan_call_args(source: &str, func: &str) -> Vec<String> {
    let idx = source.find(func).map(|i| i + func.len());
    let Some(after_func) = idx.map(|i| &source[i..]) else {
        return Vec::new();
    };
    let Some(paren_open) = after_func.find('(') else {
        return Vec::new();
    };
    let Some(paren_close) = scanners::find_matching_paren(after_func.as_bytes(), paren_open)
    else {
        return Vec::new();
    };
    scanners::split_top_level_commas(&after_func[paren_open + 1..paren_close])
}


/// Scan for "(M, N)" dimension pairs in an error message.
/// Returns strings like "(8, 8)", "(5, 5)".
fn scan_dimension_pairs(msg: &str) -> Vec<String> {
    let mut pairs = Vec::new();
    let bytes = msg.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'(' {
            // Scan to closing paren
            let start = i;
            i += 1;
            while i < bytes.len() && bytes[i] != b')' {
                i += 1;
            }
            if i < bytes.len() {
                let content = &msg[start + 1..i];
                // Check it looks like dimensions (comma-separated numbers)
                if content.split(',').all(|p| p.trim().chars().all(|c| c.is_ascii_digit())) {
                    pairs.push(msg[start..=i].to_string());
                }
                i += 1;
            }
        } else {
            i += 1;
        }
    }
    pairs
}

/// Extract the two operands of a binary operator from a source line.
/// "result = `S_hat` * K" → ["`S_hat`", "K"]
/// "A \ b" → ["A", "b"]
/// "x .+ y" → ["x", "y"]
fn scan_binary_operands(source: &str) -> Vec<String> {
    // Strip assignment: "result = S_hat * K" → "S_hat * K"
    let expr = if let Some(eq) = source.find('=') {
        // Make sure it's not ==, !=, <=, >=
        let before_eq = if eq > 0 { source.as_bytes()[eq - 1] } else { 0 };
        let after_eq = source.as_bytes().get(eq + 1).copied().unwrap_or(0);
        if before_eq != b'!' && before_eq != b'<' && before_eq != b'>' && after_eq != b'=' {
            source[eq + 1..].trim()
        } else {
            source.trim()
        }
    } else {
        source.trim()
    };

    // Split on common binary operators (trying them in order of length)
    for op in &[" .* ", " ./ ", " .\\ ", " * ", " / ", " \\ ", " .+ ", " .- "] {
        if let Some(idx) = expr.find(op) {
            let lhs = expr[..idx].trim();
            let rhs = expr[idx + op.len()..].trim();
            // Take only the "word" part (handle things like "func(x)" → skip)
            let lhs_name = extract_var_name(lhs);
            let rhs_name = extract_var_name(rhs);
            if !lhs_name.is_empty() && !rhs_name.is_empty() {
                return vec![lhs_name, rhs_name];
            }
        }
    }

    Vec::new()
}

/// Extract a variable name from an expression fragment.
/// "`S_hat`" → "`S_hat`", "func(x)" → "", "A'" → "A"
fn extract_var_name(s: &str) -> String {
    let s = s.trim().trim_end_matches('\''); // strip transpose
    let bytes = s.as_bytes();
    if bytes.is_empty() || (!bytes[0].is_ascii_alphabetic() && bytes[0] != b'_') {
        return String::new();
    }
    let mut i = 0;
    while i < bytes.len()
        && (bytes[i].is_ascii_alphanumeric()
            || bytes[i] == b'_'
            || bytes[i] == b'.'  // struct field access
            || bytes[i] == 0xCC // unicode combining (for things like x̂)
            || bytes[i] == 0xC3 // extended latin
            || bytes[i] > 127)  // any non-ASCII (Julia unicode vars)
    {
        i += 1;
    }
    // Only return if we consumed the whole thing (it's a clean variable name)
    if i == bytes.len() {
        s.to_string()
    } else {
        // Might have trailing stuff like "[1]" or "()" — take what we have
        let name = &s[..i];
        if name.is_empty() { String::new() } else { name.to_string() }
    }
}

/// Extract element count from "N-element" in a `BoundsError` message.
fn scan_element_count(msg: &str) -> Option<String> {
    let idx = msg.find("-element")?;
    // Walk backwards to find the number
    let before = &msg[..idx];
    let start = before.rfind(|c: char| !c.is_ascii_digit()).map_or(0, |i| i + 1);
    let num = &before[start..];
    if num.is_empty() {
        None
    } else {
        Some(num.to_string())
    }
}

/// For `ParseError`: point at the exact column where the parser got confused,
/// and surface bracket-balance issues on the offending line. Julia's
/// parser often reports `Expected end` for problems that are really
/// "you have one more `]` than `[`" — the caret column lands far from
/// the actual stray bracket and the user can't tell from the message
/// alone. A concrete "1 extra `]` on this line" note cuts through the
/// misdirection.
fn enrich_parse_error(message: &str, source: &str) -> Option<String> {
    let col = scan_parse_error_col(message)?;
    if col == 0 || source.is_empty() {
        return None;
    }

    let mut out = String::new();
    let col_idx = col.saturating_sub(1);
    if col_idx < source.len() {
        let _ = write!(out, "   = note: error at column {col}");
        if let Some(ch) = source.chars().nth(col_idx) {
            if !ch.is_whitespace() {
                let _ = write!(out, " (near `{ch}`)");
            }
        }
        out.push('\n');
    }

    let (paren, bracket, brace) = count_bracket_balance(source);
    if paren != 0 || bracket != 0 || brace != 0 {
        out.push_str("   = note: bracket balance on this line:\n");
        let report = |out: &mut String, net: i32, open: char, close: char| {
            if net > 0 {
                let _ = writeln!(out, "   |   {net} more `{open}` than `{close}` — unclosed");
            } else if net < 0 {
                let _ = writeln!(
                    out,
                    "   |   {} more `{close}` than `{open}` — stray close",
                    -net
                );
            }
        };
        report(&mut out, paren, '(', ')');
        report(&mut out, bracket, '[', ']');
        report(&mut out, brace, '{', '}');
        out.push_str(
            "   = help: scan the line for an extra or missing bracket before trusting the \"Expected end\" message\n",
        );
    }

    Some(out)
}

/// Count net bracket imbalance on a source line, ignoring anything inside
/// `"..."` strings. Returns `(parens, brackets, braces)` where each value
/// is `opens - closes` — positive means unclosed, negative means stray
/// close. Strings are skipped with a simple backslash-aware scanner so
/// `"]"` inside a literal doesn't throw off the count.
fn count_bracket_balance(source: &str) -> (i32, i32, i32) {
    let bytes = source.as_bytes();
    let mut paren = 0i32;
    let mut bracket = 0i32;
    let mut brace = 0i32;
    let mut in_str = false;
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        if in_str {
            if b == b'\\' {
                i += 2;
                continue;
            }
            if b == b'"' {
                in_str = false;
            }
        } else {
            match b {
                b'"' => in_str = true,
                b'#' => break, // comment — rest of line irrelevant
                b'(' => paren += 1,
                b')' => paren -= 1,
                b'[' => bracket += 1,
                b']' => bracket -= 1,
                b'{' => brace += 1,
                b'}' => brace -= 1,
                _ => {}
            }
        }
        i += 1;
    }
    (paren, bracket, brace)
}

/// Extract column number from `ParseError` message "Error @ <file:line:col>".
fn scan_parse_error_col(msg: &str) -> Option<usize> {
    for line in msg.lines() {
        let trimmed = line.trim().trim_start_matches("# ");
        if let Some(rest) = trimmed.strip_prefix("Error @ ") {
            // Split from right on ':' to get file:line:col
            let parts: Vec<&str> = rest.rsplitn(3, ':').collect();
            if !parts.is_empty() {
                if let Ok(col) = parts[0].trim().parse::<usize>() {
                    return Some(col);
                }
            }
        }
    }
    None
}

/// Extract location from Julia error patterns like "# Error @ none:10:23"
/// or "Error @ /path/to/file.jl:42:5". Returns "line 10, column 23" style.
fn scan_error_location(msg: &str) -> Option<String> {
    for line in msg.lines() {
        let trimmed = line.trim().trim_start_matches("# ");
        if let Some(rest) = trimmed.strip_prefix("Error @ ") {
            // Format: "file:line:col" or "none:line:col"
            let parts: Vec<&str> = rest.rsplitn(3, ':').collect();
            if parts.len() >= 2 {
                let col = parts[0].trim();
                let line_num = parts[1].trim();
                if line_num.chars().all(|c| c.is_ascii_digit())
                    && col.chars().all(|c| c.is_ascii_digit())
                {
                    return Some(format!("line {line_num}, column {col}"));
                }
            }
        }
    }
    None
}

/// Find the variable being indexed in source like "arr[i]" → "arr".
fn scan_indexed_var(source: &str) -> Option<String> {
    let bracket = source.find('[')?;
    let before = source[..bracket].trim();
    let name = extract_var_name(before);
    if name.is_empty() { None } else { Some(name) }
}

// ─── Public entry point ──────────────────────────────────────────────────────

/// Format a Julia error into a guided message.
/// Inputs for a single error-formatting pass. All optional context that
/// enrichers might consume lives here — adding new enrichers means
/// threading a new field, not forking a new entry point.
pub struct FormatContext<'a> {
    pub error_json: &'a str,
    pub raw_error: &'a str,
    pub notebook_path: Option<&'a str>,
}

/// An enricher inspects the parsed error + the surrounding context and
/// may mutate the error in place (e.g. adding `cell_context` entries
/// discovered via notebook static scan, annotating frames, …).
trait Enricher: Send + Sync {
    fn enrich(&self, err: &mut StructuredError, ctx: &FormatContext<'_>);
}

fn enrichers() -> &'static [Box<dyn Enricher + Send + Sync>] {
    static ENRICHERS: OnceLock<Vec<Box<dyn Enricher + Send + Sync>>> = OnceLock::new();
    ENRICHERS
        .get_or_init(|| -> Vec<Box<dyn Enricher + Send + Sync>> {
            vec![Box::new(StaticCellScanEnricher)]
        })
        .as_slice()
}

/// Unified formatter: deserialize the structured payload if present,
/// run every registered enricher against it, then format; otherwise
/// fall back to the raw-error path. Replaces the old pair of
/// `format_error` / `format_error_with_notebook` fork.
pub fn format_error(ctx: &FormatContext<'_>) -> String {
    let hints = hints();

    if let Ok(mut err) = serde_json::from_str::<StructuredError>(ctx.error_json) {
        if !err.error_type.is_empty() {
            for enricher in enrichers() {
                enricher.enrich(&mut err, ctx);
            }
            return format_structured(&err, hints);
        }
    }

    if !ctx.raw_error.is_empty() {
        return format_raw(ctx.raw_error, hints);
    }

    "error: unknown\n".to_string()
}

/// Populates `cell_context` for `UndefVarError` by scanning the notebook
/// `.jl` source for an assignment to the missing variable. Only fires
/// when the kernel supplied no context of its own.
struct StaticCellScanEnricher;

impl Enricher for StaticCellScanEnricher {
    fn enrich(&self, err: &mut StructuredError, ctx: &FormatContext<'_>) {
        if err.error_type != "UndefVarError" || !err.cell_context.is_empty() {
            return;
        }
        let Some(path) = ctx.notebook_path else { return };
        if path.is_empty() {
            return;
        }
        let var = extract_undef_var(&err.message);
        if var.is_empty() {
            return;
        }

        let json = crate::notebook::scan_variable_definition(path.to_string(), var.clone());
        if json == "null" {
            return;
        }

        #[derive(serde::Deserialize)]
        struct Scanned {
            cell_index: i64,
            #[serde(default)]
            line_in_cell: i64,
            #[serde(default)]
            line_text: String,
        }
        if let Ok(hit) = serde_json::from_str::<Scanned>(&json) {
            if hit.cell_index == err.cell_index {
                return;
            }
            err.cell_context.insert(
                var,
                VarContext::StaticSource {
                    defined_in_cell: hit.cell_index,
                    line_in_cell: hit.line_in_cell,
                    line_text: hit.line_text,
                },
            );
        }
    }
}

fn extract_undef_var(msg: &str) -> String {
    // Prefer a backticked identifier.
    if let Some(start) = msg.find('`') {
        if let Some(end) = msg[start + 1..].find('`') {
            let cand = &msg[start + 1..start + 1 + end];
            if scanners::is_identifier(cand) {
                return cand.to_string();
            }
        }
    }
    // Fallback: first whitespace-delimited word that looks like an ident.
    for word in msg.split_whitespace() {
        if scanners::is_identifier(word) {
            return word.to_string();
        }
    }
    String::new()
}

// ─── Structured formatting ───────────────────────────────────────────────────

fn format_structured(err: &StructuredError, hints: &[ErrorHint]) -> String {
    let clean_msg = clean_message(&err.message);
    let tokens = tokenize_error(&err.error_type, &clean_msg);
    let matched = find_hint(hints, &tokens);
    let mut out = String::new();

    // ── Header ──
    if let Some(h) = &matched {
        let title = expand_template(&h.title, &tokens);
        let _ = writeln!(out, "error[{}]: {}", h.id, title);
    } else {
        let short = err.message.lines().next().unwrap_or(&err.message);
        let _ = writeln!(out, "error[{}]: {}", err.error_type, truncate(short, 80));
    }

    // ── Location ──
    if err.cell_index >= 0 && err.cell_line > 0 {
        let _ = writeln!(out, "  --> cell {}, line {}", err.cell_index, err.cell_line);
    } else if err.cell_index >= 0 {
        let _ = writeln!(out, "  --> cell {}", err.cell_index);
    }

    // ── Source context ──
    out.push_str("   |\n");
    if !err.source_line.is_empty() && err.cell_line > 0 {
        let src = err.source_line.trim_end();
        let _ = writeln!(out, "{:>3} | {}", err.cell_line, src);
        let leading = err.source_line.len() - err.source_line.trim_start().len();
        let width = src.trim().len();
        if width > 0 {
            let _ = writeln!(out, "   | {}{}", " ".repeat(leading), "^".repeat(width));
        }
    }
    let short_msg = truncate(err.message.lines().next().unwrap_or(&err.message), 120);
    let _ = writeln!(out, "   | {short_msg}");
    out.push_str("   |\n");

    // ── Help + example ──
    if let Some(h) = &matched {
        if !h.help.is_empty() {
            let help = expand_template(&h.help, &tokens);
            for line in wrap(&help, 72) {
                let _ = writeln!(out, "   = help: {line}");
            }
        }
        if !h.example.is_empty() {
            let ex = expand_template(&h.example, &tokens);
            out.push_str("   = example:\n");
            for line in ex.lines() {
                let _ = writeln!(out, "   |   {line}");
            }
        }
    }

    // ── Contextual enrichment (use actual variable names from source) ──
    if let Some(enriched) = enrich_with_source_context(err) {
        out.push_str("   |\n");
        for line in enriched.lines() {
            out.push_str(line);
            out.push('\n');
        }
    }

    // ── Cross-cell context ──
    if !err.cell_context.is_empty() {
        out.push_str("   |\n");
        for (var, ctx) in &err.cell_context {
            format_var_context(&mut out, var, ctx, err.cell_index);
        }
    }

    if !err.unexecuted_deps.is_empty() {
        out.push_str("   |\n");
        let cells: String = err
            .unexecuted_deps
            .iter()
            .map(|c| format!("@cell {c}"))
            .collect::<Vec<_>>()
            .join(", ");
        let _ = writeln!(out, "   = note: this cell depends on {cells} which haven't been executed");
    }

    // ── Call chain ──
    let chain = build_call_chain(&err.frames);
    if !chain.is_empty() {
        out.push_str("   |\n");
        out.push_str("   = note: call chain:\n");
        for (i, entry) in chain.iter().enumerate() {
            let pfx = if i == 0 { "   |   → " } else { "   |     → " };
            let _ = writeln!(out, "{pfx}{entry}");
        }
    }

    out
}

/// Render one `VarContext` entry as user-facing `   = note` / `   = help`
/// lines. Pulled out so the main structured-format body stays readable
/// and so future variant additions stay isolated.
fn format_var_context(out: &mut String, var: &str, ctx: &VarContext, error_cell: i64) {
    match ctx {
        VarContext::StaticSource { defined_in_cell, line_text, .. } => {
            let relation = match defined_in_cell.cmp(&error_cell) {
                std::cmp::Ordering::Greater => "later in the notebook",
                std::cmp::Ordering::Less => "earlier in the notebook",
                std::cmp::Ordering::Equal => "in this cell",
            };
            let _ = writeln!(
                out,
                "   = note: `{var}` is defined in @cell {defined_in_cell} ({relation}) — that cell hasn't been executed yet"
            );
            if !line_text.is_empty() {
                let _ = writeln!(out, "   = note: look for:  {}", line_text.trim());
            }
            if *defined_in_cell > error_cell {
                let _ = writeln!(
                    out,
                    "   = help: move the `{var} = …` line above @cell {error_cell}, or run @cell {defined_in_cell} first"
                );
            } else {
                let _ = writeln!(
                    out,
                    "   = help: run @cell {defined_in_cell} first, or use :execute-cells-above"
                );
            }
        }
        VarContext::Executed { defined_in_cell, status } => {
            let _ = writeln!(
                out,
                "   = note: `{var}` is defined in @cell {defined_in_cell} (status: {status})"
            );
            out.push_str(
                "   = note: the cell ran but the variable may have been overwritten or errored\n",
            );
        }
        VarContext::PendingRegistered { defined_in_cell } => {
            let _ = writeln!(
                out,
                "   = note: `{var}` is defined in @cell {defined_in_cell} — not yet executed"
            );
            let _ = writeln!(
                out,
                "   = help: run @cell {defined_in_cell} first, or use :execute-cells-above"
            );
        }
    }
}


// ─── Raw formatting ──────────────────────────────────────────────────────────

fn format_raw(raw: &str, hints: &[ErrorHint]) -> String {
    let cleaned = clean_message(raw);
    let tokens = tokenize_error("", &cleaned);
    let matched = find_hint(hints, &tokens);
    let mut out = String::new();

    // Extract location from "# Error @ file:line:col" or "Error @ none:10:23"
    let location = scan_error_location(&cleaned);

    if let Some(h) = &matched {
        let title = expand_template(&h.title, &tokens);
        let _ = writeln!(out, "error[{}]: {}", h.id, title);

        // Show location if extracted
        if let Some(ref loc) = location {
            let _ = writeln!(out, "  --> {loc}");
        }

        out.push_str("   |\n");
        // Show meaningful content lines (skip the error type echo and location lines)
        let content_lines: Vec<&str> = cleaned
            .lines()
            .filter(|l| {
                let t = l.trim().trim_start_matches("# ");
                !t.is_empty()
                    && !t.starts_with("Error @")
                    && !t.starts_with("ParseError")
                    && !t.starts_with("LoadError")
            })
            .take(3)
            .collect();
        if content_lines.is_empty() {
            // No meaningful content — just show the raw first line
            let first = truncate(cleaned.lines().next().unwrap_or(&cleaned), 120);
            let _ = writeln!(out, "   | {first}");
        } else {
            for line in &content_lines {
                let _ = writeln!(out, "   | {line}");
            }
        }
        out.push_str("   |\n");

        if !h.help.is_empty() {
            let help = expand_template(&h.help, &tokens);
            for line in wrap(&help, 72) {
                let _ = writeln!(out, "   = help: {line}");
            }
        }
        if !h.example.is_empty() {
            let ex = expand_template(&h.example, &tokens);
            out.push_str("   = example:\n");
            for line in ex.lines() {
                let _ = writeln!(out, "   |   {line}");
            }
        }
    } else {
        let first = cleaned.lines().next().unwrap_or(&cleaned);
        if !tokens.error_type.is_empty() && !tokens.message.is_empty() {
            let _ = writeln!(
                out,
                "error[{}]: {}",
                tokens.error_type,
                truncate(&tokens.message, 100)
            );
        } else if !tokens.error_type.is_empty() {
            // Error type but no message — still show it cleanly
            let _ = writeln!(out, "error[{}]", tokens.error_type);
        } else {
            let _ = writeln!(out, "error: {}", truncate(first, 100));
        }

        if let Some(ref loc) = location {
            let _ = writeln!(out, "  --> {loc}");
        }

        out.push_str("   |\n");
        // Show content lines, filtering noise
        let content_lines: Vec<&str> = cleaned
            .lines()
            .filter(|l| {
                let t = l.trim().trim_start_matches("# ");
                !t.is_empty() && !t.starts_with("Error @")
            })
            .skip(1) // skip the "ErrorType: ..." line already in header
            .take(3)
            .collect();
        if content_lines.is_empty() && !tokens.message.is_empty() {
            let _ = writeln!(out, "   | {}", tokens.message);
        } else {
            for line in &content_lines {
                let _ = writeln!(out, "   | {line}");
            }
        }
        out.push_str("   |\n");
    }

    out
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── Tokenizer ──

    #[test]
    fn tokenize_method_error_with_types() {
        let tokens = tokenize_error(
            "MethodError",
            "no method matching +(::Vector{Float64}, ::Int64)",
        );
        assert_eq!(tokens.error_type, "MethodError");
        assert_eq!(tokens.func_name, "+");
        assert_eq!(tokens.types, vec!["Vector", "Int64"]);
    }

    #[test]
    fn tokenize_undef_var() {
        let tokens = tokenize_error("UndefVarError", "myvar not defined");
        assert_eq!(tokens.error_type, "UndefVarError");
        assert_eq!(tokens.message, "myvar not defined");
    }

    #[test]
    fn tokenize_qualified_type() {
        let tokens = tokenize_error("MethodError", "no method matching norm(::Base.Missing)");
        assert_eq!(tokens.func_name, "norm");
        assert!(tokens.types.contains(&"Missing".to_string()));
    }

    #[test]
    fn tokenize_typeof() {
        let tokens = tokenize_error(
            "MethodError",
            "no method matching /(::Int64, ::typeof(sqrt))",
        );
        assert_eq!(tokens.func_name, "/");
        assert!(tokens.types.iter().any(|t| t.contains("typeof(sqrt)")));
    }

    #[test]
    fn tokenize_raw_string() {
        let tokens = tokenize_error("", "BoundsError: attempt to access 3-element Vector{Int64} at index [0]");
        assert_eq!(tokens.error_type, "BoundsError");
        assert!(tokens.message.contains("index [0]"));
    }

    // ── Hint matching ──

    #[test]
    fn hints_load() {
        let hints = hints();
        assert!(hints.len() >= 50, "got {}", hints.len());
    }

    #[test]
    fn hints_cached_identity() {
        let a = hints();
        let b = hints();
        assert!(std::ptr::eq(a, b));
    }

    #[test]
    fn bounds_error_zero() {
        let raw = "BoundsError: attempt to access 3-element Vector{Int64} at index [0]";
        let hints = hints();
        let out = format_raw(raw, hints);
        assert!(out.contains("E001"), "got:\n{}", out);
        assert!(out.contains("1-indexed"));
    }

    #[test]
    fn undef_var() {
        let raw = "UndefVarError: myvar not defined";
        let hints = hints();
        let out = format_raw(raw, hints);
        assert!(out.contains("E004"), "got:\n{}", out);
        assert!(out.contains("myvar"));
    }

    #[test]
    fn method_error_function_as_arg() {
        let raw = "MethodError: no method matching /(::Int64, ::typeof(sqrt))";
        let hints = hints();
        let out = format_raw(raw, hints);
        assert!(out.contains("E005"), "got:\n{}", out);
        assert!(out.contains("sqrt"));
    }

    #[test]
    fn closest_candidates_stripped() {
        let raw = "MethodError: no method matching /(::Int64, ::typeof(sqrt))\nClosest candidates are:\n  lots of noise";
        let hints = hints();
        let out = format_raw(raw, hints);
        assert!(!out.contains("Closest candidates"));
    }

    #[test]
    fn missing_value_error() {
        let raw = "MethodError: no method matching norm(::Base.Missing)";
        let hints = hints();
        let out = format_raw(raw, hints);
        assert!(out.contains("E021"), "got:\n{}", out);
        assert!(out.contains("missing"));
    }

    #[test]
    fn nothing_iterate_error() {
        let raw = "MethodError: no method matching iterate(::Nothing)";
        let hints = hints();
        let out = format_raw(raw, hints);
        assert!(out.contains("E024"), "got:\n{}", out);
    }

    #[test]
    fn vector_plus_scalar() {
        let raw = "MethodError: no method matching +(::Vector{Float64}, ::Int64)";
        let hints = hints();
        let out = format_raw(raw, hints);
        assert!(out.contains("E060"), "got:\n{}", out);
        assert!(out.contains(".+"));
    }

    #[test]
    fn vector_times_vector() {
        let raw = "MethodError: no method matching *(::Vector{Float64}, ::Vector{Float64})";
        let hints = hints();
        let out = format_raw(raw, hints);
        assert!(out.contains("E064"), "got:\n{}", out);
        assert!(out.contains("dot"));
    }

    #[test]
    fn empty_collection() {
        let raw = "ArgumentError: reducing over an empty collection is not allowed";
        let hints = hints();
        let out = format_raw(raw, hints);
        assert!(out.contains("E071"), "got:\n{}", out);
    }

    #[test]
    fn matrix_not_square() {
        let raw = "ArgumentError: matrix is not square: dimensions are (2, 3)";
        let hints = hints();
        let out = format_raw(raw, hints);
        assert!(out.contains("E072"), "got:\n{}", out);
    }

    #[test]
    fn objects_not_callable() {
        let raw = "MethodError: objects of type Float64 are not callable";
        let hints = hints();
        let out = format_raw(raw, hints);
        assert!(out.contains("E043"), "got:\n{}", out);
    }

    #[test]
    fn structured_with_call_chain() {
        let json = r#"{
            "error_type": "MethodError",
            "message": "no method matching norm(::Base.Missing)",
            "frames": [
                {"file": "<cell>", "line": 5, "func": "compute_result", "is_user_code": true},
                {"file": "/nix/store/8h9qwxffgyisf9hiscw5ms6l56w6mni5-julia-bin-1.12.5/share/julia/stdlib/v1.12/LinearAlgebra/src/generic.jl", "line": 760, "func": "norm", "is_user_code": false}
            ],
            "source_line": "result = norm(data)",
            "cell_index": 3,
            "cell_line": 5
        }"#;
        let out = format_error(&FormatContext { error_json: json, raw_error: "", notebook_path: None });
        assert!(out.contains("E021"), "got:\n{}", out);
        assert!(out.contains("^^^"));
        assert!(out.contains("stdlib:LinearAlgebra"));
        assert!(!out.contains("/nix/store/"));
    }

    #[test]
    fn unmatched_extracts_type() {
        let raw = "CustomError: something new";
        let hints = hints();
        let out = format_raw(raw, hints);
        assert!(out.contains("error[CustomError]"));
    }

    #[test]
    fn clean_nix_stdlib_path() {
        let p = "/nix/store/8h9qwxffgyisf9hiscw5ms6l56w6mni5-julia-bin-1.12.5/share/julia/stdlib/v1.12/LinearAlgebra/src/generic.jl";
        assert_eq!(clean_path(p), "stdlib:LinearAlgebra/src/generic.jl");
    }

    #[test]
    fn clean_julia_packages_path() {
        let p = "/home/user/.julia/packages/DataFrames/AbCdE/src/dataframe.jl";
        assert_eq!(clean_path(p), "DataFrames/src/dataframe.jl");
    }

    #[test]
    fn cross_cell_context_unexecuted() {
        let json = r#"{
            "error_type": "UndefVarError",
            "message": "`data` not defined",
            "frames": [],
            "source_line": "result = norm(data)",
            "cell_index": 5,
            "cell_line": 3,
            "cell_context": {
                "data": {
                    "source": "pending_registered",
                    "defined_in_cell": 2
                }
            }
        }"#;
        let out = format_error(&FormatContext { error_json: json, raw_error: "", notebook_path: None });
        assert!(out.contains("@cell 2"), "should reference defining cell, got:\n{out}");
        assert!(out.contains("not yet executed"), "should say not executed, got:\n{out}");
        assert!(out.contains("run @cell 2 first"), "should suggest running it, got:\n{out}");
    }

    #[test]
    fn cross_cell_context_executed() {
        let json = r#"{
            "error_type": "UndefVarError",
            "message": "`x` not defined",
            "frames": [],
            "source_line": "println(x)",
            "cell_index": 3,
            "cell_line": 1,
            "cell_context": {
                "x": {
                    "source": "executed",
                    "defined_in_cell": 1,
                    "status": "error"
                }
            }
        }"#;
        let out = format_error(&FormatContext { error_json: json, raw_error: "", notebook_path: None });
        assert!(out.contains("@cell 1"), "got:\n{out}");
        assert!(out.contains("status: error"), "got:\n{out}");
    }

    #[test]
    fn unexecuted_deps_rendered() {
        let json = r#"{
            "error_type": "MethodError",
            "message": "no method matching foo(::Int64)",
            "frames": [],
            "source_line": "foo(1)",
            "cell_index": 4,
            "cell_line": 1,
            "unexecuted_deps": [1, 2]
        }"#;
        let out = format_error(&FormatContext { error_json: json, raw_error: "", notebook_path: None });
        assert!(out.contains("@cell 1"), "got:\n{out}");
        assert!(out.contains("@cell 2"), "got:\n{out}");
        assert!(out.contains("haven't been executed"), "got:\n{out}");
    }

    #[test]
    fn no_context_no_extra_output() {
        let json = r#"{
            "error_type": "BoundsError",
            "message": "attempt to access 3-element Vector{Int64} at index [0]",
            "frames": [],
            "source_line": "v[0]",
            "cell_index": 1,
            "cell_line": 1
        }"#;
        let out = format_error(&FormatContext { error_json: json, raw_error: "", notebook_path: None });
        assert!(!out.contains("@cell"), "should have no cell context, got:\n{out}");
        assert!(!out.contains("haven't been executed"), "got:\n{out}");
    }

    // ── Source context enrichment ──

    #[test]
    fn dimension_mismatch_uses_real_variable_names() {
        let json = r#"{
            "error_type": "DimensionMismatch",
            "message": "a has size (8, 8), b has size (5, 5), mismatch at dim 1",
            "frames": [],
            "source_line": "result = S_hat * K",
            "cell_index": 3,
            "cell_line": 5
        }"#;
        let out = format_error(&FormatContext { error_json: json, raw_error: "", notebook_path: None });
        assert!(out.contains("S_hat"), "should use actual var name, got:\n{out}");
        assert!(out.contains("(8, 8)"), "should show dimensions, got:\n{out}");
        assert!(out.contains("K"), "should use actual var name, got:\n{out}");
        assert!(out.contains("(5, 5)"), "should show dimensions, got:\n{out}");
        assert!(out.contains("size(S_hat, 2) == size(K, 1)"), "should show concrete check, got:\n{out}");
    }

    #[test]
    fn dimension_mismatch_backslash_solve() {
        let json = r#"{
            "error_type": "DimensionMismatch",
            "message": "a has size (3, 3), b has size (5,), mismatch at dim 1",
            "frames": [],
            "source_line": "x = A \\ b",
            "cell_index": 2,
            "cell_line": 1
        }"#;
        let out = format_error(&FormatContext { error_json: json, raw_error: "", notebook_path: None });
        assert!(out.contains("`A`"), "got:\n{out}");
        assert!(out.contains("`b`"), "got:\n{out}");
    }

    #[test]
    fn bounds_error_shows_actual_array_name() {
        let json = r#"{
            "error_type": "BoundsError",
            "message": "attempt to access 5-element Vector{Float64} at index [10]",
            "frames": [],
            "source_line": "prices[10]",
            "cell_index": 1,
            "cell_line": 3
        }"#;
        let out = format_error(&FormatContext { error_json: json, raw_error: "", notebook_path: None });
        assert!(out.contains("prices"), "should name the array, got:\n{out}");
        assert!(out.contains("5 elements"), "should show count, got:\n{out}");
    }

    #[test]
    fn scan_dimension_pairs_extracts_sizes() {
        let pairs = scan_dimension_pairs("a has size (8, 8), b has size (5, 5)");
        assert_eq!(pairs, vec!["(8, 8)", "(5, 5)"]);
    }

    #[test]
    fn scan_binary_operands_multiplication() {
        let ops = scan_binary_operands("result = S_hat * K");
        assert_eq!(ops, vec!["S_hat", "K"]);
    }

    #[test]
    fn scan_binary_operands_backslash() {
        let ops = scan_binary_operands("x = A \\ b");
        assert_eq!(ops, vec!["A", "b"]);
    }

    #[test]
    fn method_error_shows_actual_arg_types() {
        let json = r#"{
            "error_type": "MethodError",
            "message": "no method matching similar(::Int64, ::Type{Float64})",
            "frames": [],
            "source_line": "B = similar(n, Float64)",
            "cell_index": 3,
            "cell_line": 5
        }"#;
        let out = format_error(&FormatContext { error_json: json, raw_error: "", notebook_path: None });
        assert!(out.contains("`n`"), "should show actual arg name, got:\n{out}");
        assert!(out.contains("Int64"), "should show type, got:\n{out}");
        assert!(out.contains("typeof(n)"), "should suggest typeof check, got:\n{out}");
    }

    #[test]
    fn method_error_type_constructor_parenthesized() {
        // Julia writes type-constructor MethodErrors as `matching (Name)(...)`.
        // Previously enrich_method_error read the empty slice before the
        // first `(` as the name and bailed, producing `no method `` for these
        // argument types` — useless.
        let json = r#"{
            "error_type": "MethodError",
            "message": "no method matching (Matrix)(::Vector{ComplexF64})",
            "frames": [],
            "source_line": "C_inv = inv(Matrix(eigenvalues))",
            "cell_index": 19,
            "cell_line": 3
        }"#;
        let out = format_error(&FormatContext { error_json: json, raw_error: "", notebook_path: None });
        assert!(out.contains("`eigenvalues`"), "should show actual arg name, got:\n{out}");
        assert!(out.contains("Vector{ComplexF64}"), "should show Vector type, got:\n{out}");
        assert!(out.contains("typeof(eigenvalues)"), "should suggest typeof check, got:\n{out}");
    }

    #[test]
    fn scan_types_from_call_handles_parenthesized_name() {
        let types = scan_types_from_call("no method matching (Matrix)(::Vector{ComplexF64})");
        assert_eq!(types, vec!["Vector{ComplexF64}".to_string()]);
    }

    #[test]
    fn parse_error_reports_stray_close_bracket() {
        // Real user case: `V = [... for w in ω]` had an extra `]`
        // mid-expression. Julia reports "Expected `end`" because its
        // parser got confused after the first stray close — not
        // useful. The enricher should call out the bracket imbalance
        // directly.
        let json = r##"{
            "error_type": "ParseError",
            "message": "# Error @ none:16:39\nExpected `end`",
            "frames": [],
            "source_line": "V = [sum(X(w - k*ωs) for k in -10:10)] for w in ω]",
            "cell_index": 25,
            "cell_line": 16
        }"##;
        let out = format_error(&FormatContext { error_json: json, raw_error: "", notebook_path: None });
        assert!(
            out.contains("bracket balance on this line"),
            "should name the balance issue, got:\n{out}"
        );
        assert!(
            out.contains("stray close") && out.contains("`]`"),
            "should identify the extra `]`, got:\n{out}"
        );
    }

    #[test]
    fn parse_error_reports_unclosed_bracket() {
        let json = r##"{
            "error_type": "ParseError",
            "message": "# Error @ none:3:12\nExpected `end`",
            "frames": [],
            "source_line": "x = [1, 2, 3",
            "cell_index": 1,
            "cell_line": 3
        }"##;
        let out = format_error(&FormatContext { error_json: json, raw_error: "", notebook_path: None });
        assert!(
            out.contains("unclosed") && out.contains("`[`"),
            "should identify the unclosed `[`, got:\n{out}"
        );
    }

    #[test]
    fn parse_error_ignores_brackets_inside_strings() {
        // `println("]")` is balanced — the `]` is inside a string literal.
        let json = r##"{
            "error_type": "ParseError",
            "message": "# Error @ none:1:12\nsomething",
            "frames": [],
            "source_line": "println(\"]\")",
            "cell_index": 0,
            "cell_line": 1
        }"##;
        let out = format_error(&FormatContext { error_json: json, raw_error: "", notebook_path: None });
        assert!(
            !out.contains("bracket balance"),
            "bracket inside string must not trigger imbalance note, got:\n{out}"
        );
    }

    #[test]
    fn not_callable_pinpoints_by_scope_type() {
        // User's case: `V = [sum(X(w - k*ωₛ) for k in -10:10) for w in ω]`
        // where `X` was previously assigned a Vector{ComplexF64}. Julia
        // says "objects of type Vector{ComplexF64} are not callable".
        // With kernel's scope map we match the type to the called name
        // and tell the user exactly which cell defined it.
        let json = r##"{
            "error_type": "MethodError",
            "message": "MethodError: objects of type Vector{ComplexF64} are not callable\nUse square brackets [] for indexing an Array.",
            "frames": [],
            "source_line": "V = [sum(X(w - k*ωs) for k in -10:10) for w in ω]",
            "cell_index": 25,
            "cell_line": 8,
            "in_scope_variable_types": {
                "Vector{ComplexF64}": [{"name": "X", "cell": 17}]
            }
        }"##;
        let out = format_error(&FormatContext { error_json: json, raw_error: "", notebook_path: None });
        assert!(out.contains("called something of type Vector{ComplexF64}"), "got:\n{out}");
        assert!(
            out.contains("`X` — Vector{ComplexF64} assigned in cell 17"),
            "should name X and cell 17, got:\n{out}"
        );
    }

    #[test]
    fn not_callable_without_scope_map_lists_call_names() {
        // When the kernel hasn't been running, fall back to naming the
        // identifiers called on the offending line so the user knows
        // where to look.
        let json = r##"{
            "error_type": "MethodError",
            "message": "MethodError: objects of type Vector{Int64} are not callable",
            "frames": [],
            "source_line": "result = foo(x) + bar(y)",
            "cell_index": 3,
            "cell_line": 2
        }"##;
        let out = format_error(&FormatContext { error_json: json, raw_error: "", notebook_path: None });
        assert!(out.contains("names called in the source line"), "got:\n{out}");
        assert!(out.contains("`foo()`") && out.contains("`bar()`"), "got:\n{out}");
    }

    #[test]
    fn method_error_with_kernel_scope_hints() {
        // Kernel attached in_scope_variable_types + method_candidates:
        // Vector{ComplexF64} is held by `eigenvalues` (cell 17); Circulant
        // is held by `C1` (cell 17) and `Matrix(::Circulant)` has a method.
        // The enricher should render BOTH the "variables by type" block
        // and the "candidates" block using those values.
        let json = r#"{
            "error_type": "MethodError",
            "message": "no method matching (Matrix)(::Vector{ComplexF64})",
            "frames": [],
            "source_line": "C_inv = inv(Matrix(eigenvalues))",
            "cell_index": 19,
            "cell_line": 3,
            "in_scope_variable_types": {
                "Vector{ComplexF64}": [{"name": "eigenvalues", "cell": 17}],
                "Circulant{Int64, Vector{Int64}}": [{"name": "C1", "cell": 17}]
            },
            "method_candidates": [
                {"name": "C1", "type": "Circulant{Int64, Vector{Int64}}", "cell": 17}
            ]
        }"#;
        let out = format_error(&FormatContext { error_json: json, raw_error: "", notebook_path: None });
        assert!(
            out.contains("in-scope variables by type"),
            "should include scope block, got:\n{out}"
        );
        assert!(
            out.contains("Vector{ComplexF64}: `eigenvalues` (cell 17)"),
            "should map type → var, got:\n{out}"
        );
        assert!(
            out.contains("`Matrix()` accepts these in-scope values"),
            "should include candidates block, got:\n{out}"
        );
        assert!(
            out.contains("`C1`"),
            "should name C1 as candidate, got:\n{out}"
        );
    }

    #[test]
    fn method_error_without_kernel_hints_still_works() {
        // No in_scope_variable_types / method_candidates → parenthesized
        // name parse + arg-type note should still render. Guarantees the
        // enricher degrades gracefully when the kernel isn't running.
        let json = r#"{
            "error_type": "MethodError",
            "message": "no method matching (Matrix)(::Vector{ComplexF64})",
            "frames": [],
            "source_line": "C_inv = inv(Matrix(eigenvalues))",
            "cell_index": 19,
            "cell_line": 3
        }"#;
        let out = format_error(&FormatContext { error_json: json, raw_error: "", notebook_path: None });
        assert!(out.contains("`eigenvalues`"), "got:\n{out}");
        assert!(!out.contains("in-scope variables by type"), "should not render empty scope block, got:\n{out}");
    }

    #[test]
    fn method_error_multi_arg_enrichment() {
        let json = r#"{
            "error_type": "MethodError",
            "message": "no method matching foo(::Vector{Float64}, ::String)",
            "frames": [],
            "source_line": "result = foo(data, label)",
            "cell_index": 1,
            "cell_line": 2
        }"#;
        let out = format_error(&FormatContext { error_json: json, raw_error: "", notebook_path: None });
        assert!(out.contains("`data`"), "got:\n{out}");
        assert!(out.contains("`label`"), "got:\n{out}");
        assert!(out.contains("typeof(data)"), "got:\n{out}");
        assert!(out.contains("typeof(label)"), "got:\n{out}");
    }
}
