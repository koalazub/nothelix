//! `.ipynb ↔ .jl` round-trip conversion.
//!
//! Two directions:
//!   - [`notebook_convert_sync`]: `.ipynb` → Nothelix `.jl` cell format.
//!     Writes the marker preamble + each cell prefixed with `@cell N
//!     :lang`, `@markdown N` or `@raw N`. Markdown attachments are
//!     extracted to `.nothelix/images/` sidecar files referenced via
//!     `# @image` markers. Auto-formats single-line math envs so
//!     a freshly-converted notebook looks the same as one that's been
//!     saved through the editor.
//!   - [`convert_to_ipynb`]: `.jl` → `.ipynb`. Preserves the original
//!     notebook's `outputs` / `execution_count` / `id` for unedited
//!     cells (matching by position first, then by content so reorders
//!     keep their outputs), drops them on edits, lifts `# @image`
//!     markers into `display_data` outputs (code cells) or base64
//!     `attachments` (markdown cells), and stamps deterministic
//!     nbformat-4.5 ids on fresh cells.
//!
//! Round-trip guarantees and their deliberate narrowings:
//!
//!   - **Trailing blank lines are normalized away.** [`body_lines`]
//!     drops a cell body's trailing blank lines on emit, and the `.jl`
//!     parser applies the same trim on read. A cell whose source ends
//!     in blank lines round-trips to the identical cell minus those
//!     lines; all other content is preserved byte-for-byte. This
//!     narrowing is what makes convert → parse → convert byte-stable.
//!   - **Attachment refs are transport artifacts.** Both the empty-alt
//!     `![](attachment:NAME)` form this module emits and Jupyter's
//!     alt-texted `![NAME](attachment:NAME)` form are recognized; alt
//!     text is not preserved (re-embedding always writes the empty-alt
//!     form, appended at the end of the body).
//!   - **Undecodable attachments survive.** An attachment whose
//!     payload can't be decoded (or whose key sanitizes to nothing)
//!     is not extracted: its ref line stays in the markdown body, and
//!     on the way back the original cell's entry is carried through
//!     for every name still referenced in the body. Exact guarantee:
//!     an attachment entry survives `.ipynb → .jl → .ipynb` iff its
//!     `![…](attachment:…)` ref line is still present in the cell
//!     body when converting back.
//!
//! All non-trivial helpers live alongside their consumer; the two
//! public fns are the only export surface this module needs.

// Steel's `register_fn` marshals values from the Steel VM and requires
// the registered fn's signature to take owned types (`String`), not
// borrows.
#![allow(clippy::needless_pass_by_value)]

use std::fs;

use serde_json::{Value, json};

use super::cells::{
    CellKind, JlCell, jl_sibling_path, parse_jl_file, read_notebook, source_to_string,
};
use super::embed::{
    attachment_ref_name, embed_markdown_attachments, extract_markdown_attachments,
    is_attachment_ref_line, read_sidecar_image_output,
};

/// A cell body's lines with trailing blank lines dropped — the same
/// trim [`parse_jl_file`] applies when it reads the body back. Emitting
/// through the identical normalization is what makes a convert → parse
/// → convert cycle byte-stable (the fixpoint property).
fn body_lines(source: &str) -> Vec<&str> {
    let mut lines: Vec<&str> = source.lines().collect();
    while lines.last().is_some_and(|l| l.trim().is_empty()) {
        lines.pop();
    }
    lines
}

/// A markdown body reduced to its prose: `![…](attachment:…)` ref
/// lines (any alt text) dropped, surrounding whitespace trimmed. Both
/// sides of the original-cell match go through this so attachment
/// transport (which strips and re-appends those refs) can't break the
/// comparison.
fn strip_attachment_refs(body: &str) -> String {
    body.lines()
        .filter(|l| !is_attachment_ref_line(l))
        .collect::<Vec<_>>()
        .join("\n")
        .trim()
        .to_string()
}

/// nbformat 4.5 id for a cell with no original to inherit from. Hashing
/// content + position keeps the id deterministic across conversions
/// without a uuid dependency; 16 hex chars satisfies the spec's 1-64
/// char `[a-zA-Z0-9-_]` grammar.
fn deterministic_cell_id(cell_type: &str, source: &str, position: usize) -> String {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    cell_type.hash(&mut h);
    source.trim().hash(&mut h);
    position.hash(&mut h);
    format!("{:016x}", h.finish())
}

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

        match cell_type {
            "markdown" => {
                // Pull base64 attachments out to sidecar files and
                // reference them with `# @image` so the reverse
                // conversion (embed_markdown_attachments) re-embeds
                // the same bytes — attachments survive the round-trip.
                let (body, image_rels) =
                    extract_markdown_attachments(&source, cell.get("attachments"), &path);
                out.push_str(&format!("@markdown {idx}\n"));
                for line in body_lines(&body) {
                    out.push_str("# ");
                    out.push_str(line);
                    out.push('\n');
                }
                for rel in &image_rels {
                    out.push_str(&format!("# @image {rel}\n"));
                }
            }
            "raw" => {
                out.push_str(&format!("@raw {idx}\n"));
                for line in body_lines(&source) {
                    out.push_str("# ");
                    out.push_str(line);
                    out.push('\n');
                }
            }
            _ => {
                out.push_str(&format!("@cell {idx} :{lang}\n"));
                for line in body_lines(&source) {
                    out.push_str(line);
                    out.push('\n');
                }
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

    // Markdown/raw cells carry "# " on every line in the .jl form;
    // strip it to recover the raw source for the .ipynb.
    let jl_to_ipynb_source = |cell: &JlCell| -> String {
        if matches!(
            cell.kind,
            CellKind::Markdown | CellKind::Typst | CellKind::Raw
        ) {
            cell.code
                .lines()
                .map(|l| l.strip_prefix("# ").unwrap_or(l))
                .collect::<Vec<_>>()
                .join("\n")
        } else {
            cell.code.clone()
        }
    };

    let expected_type = |cell: &JlCell| -> &'static str {
        match cell.kind {
            CellKind::Markdown | CellKind::Typst => "markdown",
            CellKind::Raw => "raw",
            CellKind::Code => "code",
        }
    };

    // Round-trip integrity guard. Keep the orig cell's metadata + id +
    // outputs + execution_count ONLY when the orig plausibly describes
    // the SAME cell the user has in the .jl:
    //   1. same cell_type (code ↔ code, markdown ↔ markdown, raw ↔ raw),
    //   2. same trimmed source (insensitive to trailing whitespace).
    //
    // Markdown comparison additionally ignores `![…](attachment:…)`
    // ref lines (any alt text): extraction strips them on ipynb→jl and
    // embedding re-appends them on the way back, so an attachment-
    // bearing cell would never match its own original otherwise. Both
    // sides are stripped of ALL ref lines — not just extracted ones —
    // so a partially-extracted cell (some attachments undecodable,
    // their refs left in the body) still matches its original and the
    // surviving entries can be carried through below.
    //
    // If the check fails the cell is treated as fresh, so we don't
    // attach stale outputs from the orig .ipynb to a cell whose code
    // has since been edited.
    let orig_is_valid_for = |orig: &Value, cell: &JlCell| -> bool {
        let orig_type = orig.get("cell_type").and_then(|v| v.as_str()).unwrap_or("");
        if orig_type != expected_type(cell) {
            return false;
        }
        let orig_source = source_to_string(&orig["source"]);
        let new_source = jl_to_ipynb_source(cell);
        if orig_type == "markdown" {
            strip_attachment_refs(&orig_source) == strip_attachment_refs(&new_source)
        } else {
            orig_source.trim() == new_source.trim()
        }
    };

    // Each orig backs at most one .jl cell, so two same-source cells
    // can't share one original's outputs/id.
    let mut orig_used = vec![false; orig_cells.len()];
    let mut new_cells: Vec<Value> = Vec::with_capacity(cells.len());
    for (position, cell) in cells.iter().enumerate() {
        let source_text = jl_to_ipynb_source(cell);

        // Positional fast path: the orig at the cell's own index. When
        // the user reorders cells in the .jl that index points at a
        // different orig and the guard rejects it — fall back to a
        // content search over the not-yet-claimed origs so the moved
        // cell keeps its outputs.
        let matched = usize::try_from(cell.index)
            .ok()
            .filter(|&i| {
                i < orig_cells.len() && !orig_used[i] && orig_is_valid_for(&orig_cells[i], cell)
            })
            .or_else(|| {
                orig_cells
                    .iter()
                    .enumerate()
                    .position(|(i, o)| !orig_used[i] && orig_is_valid_for(o, cell))
            });
        let orig = matched.map(|i| {
            orig_used[i] = true;
            orig_cells[i].clone()
        });

        let mut c = if cell.kind == CellKind::Markdown || cell.kind == CellKind::Typst {
            let mut c = orig
                .unwrap_or_else(|| json!({"cell_type": "markdown", "metadata": {}, "source": []}));
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

            // The cell's attachments are the union of (a) entries the
            // matched original carried that a `![…](attachment:…)`
            // line in the body still references — this is how
            // undecodable attachments survive: extraction left their
            // ref line in place, so the entry rides through here —
            // and (b) the freshly embedded `# @image` entries, which
            // win on key collision because their bytes are the
            // current sidecar content. Entries whose ref line is gone
            // were deleted by the user and are dropped.
            let referenced: std::collections::HashSet<&str> = markdown_body
                .lines()
                .filter_map(attachment_ref_name)
                .collect();
            let mut merged = c
                .get("attachments")
                .and_then(Value::as_object)
                .cloned()
                .unwrap_or_default();
            merged.retain(|name, _| referenced.contains(name.as_str()));
            if let Some(embedded) = attachments.as_object() {
                for (name, entry) in embedded {
                    merged.insert(name.clone(), entry.clone());
                }
            }
            if merged.is_empty() {
                if let Some(obj) = c.as_object_mut() {
                    obj.remove("attachments");
                }
            } else {
                c["attachments"] = Value::Object(merged);
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
        } else if cell.kind == CellKind::Raw {
            let mut c =
                orig.unwrap_or_else(|| json!({"cell_type": "raw", "metadata": {}, "source": []}));
            c["cell_type"] = json!("raw");
            c["source"] = make_source_lines(&source_text);

            // Like markdown, raw cells carry neither execution_count
            // nor outputs in the Jupyter spec.
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
        };

        // nbformat 4.5 requires a cell id; cells with no original to
        // inherit one from get a deterministic stamp so reconversion
        // reproduces the same notebook (the fixpoint property).
        if let Some(obj) = c.as_object_mut()
            && !obj.get("id").is_some_and(Value::is_string)
        {
            obj.insert(
                "id".to_string(),
                json!(deterministic_cell_id(
                    expected_type(cell),
                    &source_text,
                    position
                )),
            );
        }

        new_cells.push(c);
    }

    original["cells"] = Value::Array(new_cells);

    // Cell ids are only legal from nbformat 4.5 on, and we always emit
    // them — floor the minor version accordingly.
    if original["nbformat_minor"].as_i64().unwrap_or(0) < 5 {
        original["nbformat_minor"] = json!(5);
    }

    let out_path = if source_path.ends_with(".ipynb") {
        source_path.clone()
    } else {
        jl_sibling_path(&jl_path, ".ipynb")
    };

    match fs::write(
        &out_path,
        serde_json::to_string_pretty(&original).unwrap_or_default(),
    ) {
        Ok(_) => format!("Synced to {out_path}"),
        Err(e) => format!("ERROR: Cannot write {out_path}: {e}"),
    }
}
