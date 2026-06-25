//! Final rendering of a parsed + enriched error into the user-facing
//! Rust-compiler-style block (`error[E…]: …`, `  --> cell N, line L`,
//! source-line frame, hint help/example, enriched notes, optional call
//! chain). Two entry points: `format_structured` for kernel-supplied
//! JSON payloads, `format_raw` for stderr-only inputs.

use std::fmt::Write;

use super::enrichment::{enrich_with_source_context, scan_error_location};
use super::hints::ErrorHint;
use super::matching::{expand_template, find_hint};
use super::tokenize::tokenize_error;
use super::types::{StructuredError, VarContext};
use super::util::{build_call_chain, clean_message, truncate, wrap};
// ─── Structured formatting ───────────────────────────────────────────────────

pub(super) fn format_structured(err: &StructuredError, hints: &[ErrorHint]) -> String {
    let clean_msg = clean_message(&err.message).to_string();
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
        let _ = writeln!(
            out,
            "   = note: this cell depends on {cells} which haven't been executed"
        );
    }

    // ── Call chain ──
    let chain = build_call_chain(&err.frames);
    if !chain.is_empty() {
        out.push_str("   |\n");
        out.push_str("   = note: call chain:\n");
        for (i, entry) in chain.iter().enumerate() {
            let pfx = if i == 0 {
                "   |   → "
            } else {
                "   |     → "
            };
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
        VarContext::StaticSource {
            defined_in_cell,
            line_text,
            ..
        } => {
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
        VarContext::Executed {
            defined_in_cell,
            status,
        } => {
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

pub(super) fn format_raw(raw: &str, hints: &[ErrorHint]) -> String {
    let cleaned = clean_message(raw).to_string();
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
