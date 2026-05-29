//! `.ipynb ↔ .jl` round-trip conversion.
//!
//! Two directions:
//!   - [`notebook_convert_sync`]: `.ipynb` → Nothelix `.jl` cell format.
//!     Writes the marker preamble + each cell prefixed with `@cell N
//!     :lang` or `@markdown N`. Auto-formats single-line math envs so
//!     a freshly-converted notebook looks the same as one that's been
//!     saved through the editor.
//!   - [`convert_to_ipynb`]: `.jl` → `.ipynb`. Preserves the original
//!     notebook's `outputs` / `execution_count` for unedited cells,
//!     drops them on edits, lifts `# @image` markers into
//!     `display_data` outputs (code cells) or base64 `attachments`
//!     (markdown cells).
//!
//! All non-trivial helpers live alongside their consumer; the two
//! public fns are the only export surface this module needs.

// Steel's `register_fn` marshals values from the Steel VM and requires
// the registered fn's signature to take owned types (`String`), not
// borrows.
#![allow(clippy::needless_pass_by_value)]

use std::fs;

use serde_json::{json, Value};

use super::cells::{parse_jl_file, read_notebook, source_to_string, CellKind, JlCell};
use super::embed::{embed_markdown_attachments, read_sidecar_image_output};

pub fn notebook_convert_sync(path: String) -> String {
    let nb = match read_notebook(&path) {
        Err(e) => return format!("ERROR: {e}"),
        Ok(v) => v,
    };
    let cells = match nb["cells"].as_array() {
        Some(c) => c,
        None => return "ERROR: no cells array".to_string(),
    };

    let lang = nb["metadata"]["kernelspec"]["language"]
        .as_str()
        .unwrap_or("julia");

    let mut out = String::new();
    // Preamble: using NothelixMacros so the Julia LSP resolves @cell
    // and @markdown markers without false-positive "Missing reference"
    // squiggles. The package lives in the nothelix LSP bootstrap env
    // (not the user's Project.toml) so uninstall cleans it up.
    out.push_str("using NothelixMacros  # cell markers for static checking\n\n");
    out.push_str(&format!(
        "# ═══ Nothelix Notebook: {path} ═══\n# Cells: {}\n\n",
        cells.len()
    ));

    for (idx, cell) in cells.iter().enumerate() {
        let cell_type = cell["cell_type"].as_str().unwrap_or("code");
        let source = source_to_string(&cell["source"]);

        if cell_type == "markdown" {
            out.push_str(&format!("@markdown {idx}\n"));
            for line in source.lines() {
                out.push_str("# ");
                out.push_str(line);
                out.push('\n');
            }
        } else {
            out.push_str(&format!("@cell {idx} :{lang}\n"));
            out.push_str(&source);
            if !source.ends_with('\n') {
                out.push('\n');
            }
        }
        out.push('\n');
    }

    // Run math formatting across every markdown cell in one pass so newly
    // converted notebooks arrive already-normalized: single-line
    // \begin{cases}/pmatrix/aligned get expanded, and `$$`-block content
    // that was crammed onto one line gets split at \text{…}, \\, and env
    // boundaries. Idempotent — a second convert on the same source is a
    // no-op, and this is cheaper than forcing the user to `:w` the fresh
    // file just to trigger the save-hook formatter.
    crate::math_format::format_math(out)
}

pub fn convert_to_ipynb(jl_path: String) -> String {
    let (cells, source_path) = match parse_jl_file(&jl_path) {
        Err(e) => return format!("ERROR: {e}"),
        Ok(v) => v,
    };

    let mut original = read_notebook(&source_path).unwrap_or_else(|_| {
        json!({
            "nbformat": 4,
            "nbformat_minor": 5,
            "metadata": {},
            "cells": []
        })
    });

    let orig_cells = original["cells"].as_array().cloned().unwrap_or_default();

    let make_source_lines = |text: &str| -> Value {
        let line_count = text.lines().count();
        let lines: Vec<Value> = text
            .lines()
            .enumerate()
            .map(|(i, l)| {
                let mut s = l.to_string();
                if i < line_count.saturating_sub(1) {
                    s.push('\n');
                }
                Value::String(s)
            })
            .collect();
        Value::Array(lines)
    };

    // Markdown cells carry "# " on every line in the .jl form; strip it
    // to recover the raw markdown source for the .ipynb.
    let jl_to_ipynb_source = |cell: &JlCell| -> String {
        if cell.kind == CellKind::Markdown || cell.kind == CellKind::Typst {
            cell.code
                .lines()
                .map(|l| l.strip_prefix("# ").unwrap_or(l))
                .collect::<Vec<_>>()
                .join("\n")
        } else {
            cell.code.clone()
        }
    };

    // Round-trip integrity guard. Keep the orig cell's metadata +
    // outputs + execution_count ONLY when the orig at `cell.index`
    // plausibly describes the SAME cell the user has in the .jl:
    //   1. same cell_type (code ↔ code, markdown ↔ markdown),
    //   2. same trimmed source (insensitive to trailing whitespace).
    //
    // If either check fails the cell is treated as fresh, so we don't
    // attach stale outputs from the orig .ipynb to a cell whose code
    // has since been edited (or to a cell that was reordered and whose
    // index now lands on a different orig). Index is a positional
    // fallback — after `renumber-cells!` or a reorder, matching by
    // content is what actually tells us if the orig is still valid.
    let orig_is_valid_for = |orig: &Value, cell: &JlCell| -> bool {
        let orig_type = orig.get("cell_type").and_then(|v| v.as_str()).unwrap_or("");
        let expected_type = if matches!(cell.kind, CellKind::Markdown | CellKind::Typst) {
            "markdown"
        } else {
            "code"
        };
        if orig_type != expected_type {
            return false;
        }
        let orig_source = source_to_string(&orig["source"]);
        let new_source = jl_to_ipynb_source(cell);
        orig_source.trim() == new_source.trim()
    };

    let new_cells: Vec<Value> = cells
        .iter()
        .map(|cell| {
            let source_text = jl_to_ipynb_source(cell);
            let orig = orig_cells
                .get(cell.index as usize)
                .cloned()
                .filter(|o| orig_is_valid_for(o, cell));

            if cell.kind == CellKind::Markdown || cell.kind == CellKind::Typst {
                let mut c = orig.unwrap_or_else(
                    || json!({"cell_type": "markdown", "metadata": {}, "source": []}),
                );
                c["cell_type"] = json!("markdown");

                // Embed every `# @image <path>` marker the cell held as
                // a base64 `attachments` entry, and rewrite the source
                // to reference them via `attachment:filename` — Jupyter's
                // native convention for in-cell image embedding. The
                // .ipynb becomes self-contained: shareable as a gist,
                // openable in vanilla Jupyter, readable on GitHub.
                let (markdown_body, attachments) =
                    embed_markdown_attachments(&source_text, &cell.images, &jl_path);
                c["source"] = make_source_lines(&markdown_body);
                let has_attachments = attachments
                    .as_object()
                    .map(|m| !m.is_empty())
                    .unwrap_or(false);
                if has_attachments {
                    c["attachments"] = attachments;
                }

                // Markdown cells don't carry execution_count or outputs
                // in the Jupyter spec; if the orig had them (e.g. the
                // cell was a code cell before), drop them so the .ipynb
                // stays conformant.
                if let Some(obj) = c.as_object_mut() {
                    obj.remove("execution_count");
                    obj.remove("outputs");
                }
                c
            } else {
                let mut c = orig.unwrap_or_else(|| {
                    json!({
                        "cell_type": "code",
                        "execution_count": null,
                        "metadata": {},
                        "outputs": [],
                        "source": []
                    })
                });
                c["cell_type"] = json!("code");
                c["source"] = make_source_lines(&source_text);
                if let Some(obj) = c.as_object_mut() {
                    obj.entry("execution_count").or_insert(json!(null));
                    obj.entry("outputs").or_insert(json!([]));
                }

                // Embed any plot the user generated in nothelix for this
                // cell (`.nothelix/images/cell-N.png` next to the .jl)
                // as a `display_data` output with base64 PNG. Keeps the
                // .ipynb self-contained — opened in vanilla Jupyter (or
                // pushed to GitHub) the plot shows without needing the
                // sidecar `.nothelix/images/` directory. Replaces prior
                // outputs because nothelix's saved image is the most
                // recent result for this cell.
                if let Some(image_output) = read_sidecar_image_output(&jl_path, cell.index) {
                    c["outputs"] = json!([image_output]);
                }

                c
            }
        })
        .collect();

    original["cells"] = Value::Array(new_cells);

    let out_path = if source_path.ends_with(".ipynb") {
        source_path.clone()
    } else {
        jl_path.replace(".jl", ".ipynb")
    };

    match fs::write(
        &out_path,
        serde_json::to_string_pretty(&original).unwrap_or_default(),
    ) {
        Ok(_) => format!("Synced to {out_path}"),
        Err(e) => format!("ERROR: Cannot write {out_path}: {e}"),
    }
}

