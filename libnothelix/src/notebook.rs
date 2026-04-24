//! Notebook parsing and conversion (.ipynb ↔ .jl).
//!
//! Implements the Nothelix `.jl` cell format:
//!
//! ```text
//! # ═══ Nothelix Notebook: /full/path/to/notebook.ipynb ═══
//! # Cells: N
//!
//! @cell 0 :julia
//! <code>
//!
//! @markdown 1
//! # <markdown line as Julia comment>
//!
//! @cell 2 julia
//! <code>
//! # ─── Output ───
//! <output>
//! # ─────────────
//! ```

use std::fs;
use std::path::Path;

use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use serde_json::{json, Value};

/// Look for a plot that nothelix cached from an earlier execution of
/// `cell_index` (`.nothelix/images/cell-N.png` next to the notebook) and
/// build a Jupyter `display_data` output carrying its base64 PNG. Used
/// on `.jl → .ipynb` sync so plots the user ran in nothelix survive in
/// the portable .ipynb output. Returns `None` when no sidecar file exists.
/// Turn a markdown cell's `# @image <path>` markers into an
/// `attachments` map keyed by filename and rewrite the cell source to
/// reference them via `![](attachment:filename)`. Each successfully-
/// read image contributes one `{image/png: "<base64>"}` entry. Files
/// that can't be read are skipped silently — the user's source stays
/// unchanged for those, so the `.jl` can still render them via the
/// sidecar marker even if the `.ipynb` form can't embed them.
fn embed_markdown_attachments(
    markdown_body: &str,
    image_paths: &[String],
    jl_path: &str,
) -> (String, Value) {
    if image_paths.is_empty() {
        return (markdown_body.to_string(), json!({})); // sentinel empty — caller discards
    }

    let parent = Path::new(jl_path).parent();
    let mut attachments = serde_json::Map::new();
    let mut image_lines = Vec::new();

    for raw_path in image_paths {
        let path_obj = parent
            .map(|p| p.join(raw_path))
            .unwrap_or_else(|| Path::new(raw_path).to_path_buf());
        let bytes = match fs::read(&path_obj) {
            Ok(b) if !b.is_empty() => b,
            _ => continue,
        };
        let filename = Path::new(raw_path)
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| raw_path.clone());
        let mime = mime_for_extension(&filename);
        let b64 = BASE64.encode(&bytes);
        let mut mime_map = serde_json::Map::new();
        mime_map.insert(mime.to_string(), Value::String(b64));
        attachments.insert(filename.clone(), Value::Object(mime_map));
        image_lines.push(format!("![]({})", format!("attachment:{filename}")));
    }

    if attachments.is_empty() {
        return (markdown_body.to_string(), json!({}));
    }

    // Inject the image refs at the end of the markdown body, separated
    // by a blank line so they render on their own paragraph.
    let mut body = markdown_body.trim_end().to_string();
    body.push_str("\n\n");
    body.push_str(&image_lines.join("\n"));
    (body, Value::Object(attachments))
}

fn mime_for_extension(filename: &str) -> &'static str {
    let lower = filename.to_lowercase();
    if lower.ends_with(".png") {
        "image/png"
    } else if lower.ends_with(".jpg") || lower.ends_with(".jpeg") {
        "image/jpeg"
    } else if lower.ends_with(".gif") {
        "image/gif"
    } else if lower.ends_with(".webp") {
        "image/webp"
    } else if lower.ends_with(".svg") {
        "image/svg+xml"
    } else {
        "application/octet-stream"
    }
}

fn read_sidecar_image_output(jl_path: &str, cell_index: isize) -> Option<Value> {
    let parent = Path::new(jl_path).parent()?;
    let img_path = parent
        .join(".nothelix")
        .join("images")
        .join(format!("cell-{cell_index}.png"));
    let bytes = fs::read(&img_path).ok()?;
    if bytes.is_empty() {
        return None;
    }
    let b64 = BASE64.encode(&bytes);
    Some(json!({
        "output_type": "display_data",
        "data": {"image/png": b64},
        "metadata": {}
    }))
}

// ─── Cell types ───────────────────────────────────────────────────────────────

#[derive(Debug, PartialEq)]
pub enum CellKind {
    Code,
    Markdown,
    Typst,
}

pub struct JlCell {
    pub index: isize,
    pub kind: CellKind,
    pub code: String,
    pub start_line: usize,
    /// Trailing comment from the marker line, e.g. "# Q1" from "@markdown 3 # Q1".
    /// Prepended to cell code during export so it appears in the ipynb.
    pub marker_comment: String,
    /// Paths from `# @image <path>` markers inside this cell's body.
    /// Stripped from `code` (the kernel doesn't need them as literal
    /// comments, and in markdown they'd render as `@image foo.png`
    /// prose), but preserved here so `convert_to_ipynb` can lift them
    /// into portable forms — `display_data` outputs on code cells or
    /// base64 `attachments` on markdown cells.
    pub images: Vec<String>,
}

// ─── Helpers ──────────────────────────────────────────────────────────────────

/// Extract the trailing comment from a marker line's "rest" portion.
/// Input: "3 # Q1" or "5 :julia # answer" or "0" → "# Q1", "# answer", ""
fn extract_marker_comment(rest: &str) -> String {
    if let Some(hash_pos) = rest.find(" #") {
        // Everything from " #" onward, trimmed
        let comment = rest[hash_pos + 1..].trim();
        if comment.starts_with('#') {
            return comment.to_string();
        }
    }
    String::new()
}

/// Read and parse an `.ipynb` file.
pub fn read_notebook(path: &str) -> Result<Value, String> {
    let content = fs::read_to_string(path).map_err(|e| format!("Cannot read {path}: {e}"))?;
    serde_json::from_str(&content).map_err(|e| format!("Invalid JSON in {path}: {e}"))
}

/// Join notebook cell `source` lines into a single `String`.
pub fn source_to_string(source: &Value) -> String {
    match source {
        Value::Array(lines) => lines
            .iter()
            .map(|l| l.as_str().unwrap_or(""))
            .collect::<Vec<_>>()
            .join(""),
        Value::String(s) => s.clone(),
        _ => String::new(),
    }
}

/// Parse a `.jl` file into a list of cells and the originating `.ipynb` path.
pub fn parse_jl_file(jl_path: &str) -> Result<(Vec<JlCell>, String), String> {
    let content = fs::read_to_string(jl_path).map_err(|e| format!("Cannot read {jl_path}: {e}"))?;
    let lines: Vec<&str> = content.lines().collect();

    // Extract source .ipynb path from header.
    let mut source_path = String::new();
    for line in &lines {
        if let Some(rest) = line.strip_prefix("# ═══ Nothelix Notebook: ") {
            source_path = rest.trim_end_matches(" ═══").trim().to_string();
            break;
        }
    }
    if source_path.is_empty() {
        source_path = jl_path.replace(".jl", ".ipynb");
    }

    // Locate cell markers.
    //
    // Match three shapes on each line:
    //   `@cell N :lang`      (full marker, stamp N, record lang)
    //   `@cell :lang`        (nothelix autofill produced it without
    //                         an index yet — accept, assign index 0
    //                         as a placeholder, the renumber pass
    //                         later fixes it)
    //   `@cell`              (bare — user typed it and the autofill
    //                         hasn't fired yet; still a boundary)
    //
    // …and equivalent shapes for `@markdown`. Before we only matched
    // `@cell ` (with trailing space), so bare `@cell` lines slipped
    // through and got shipped into the Julia kernel where they
    // detonated with `MethodError: no method matching var"@cell"`.
    let mut cells: Vec<JlCell> = Vec::new();
    for (i, line) in lines.iter().enumerate() {
        if line.trim_end() == "@cell" {
            cells.push(JlCell {
                index: 0,
                kind: CellKind::Code,
                code: String::new(),
                start_line: i,
                marker_comment: String::new(),
                images: Vec::new(),
            });
        } else if let Some(rest) = line.strip_prefix("@cell ") {
            let rest = rest.trim();
            let first = rest.split_whitespace().next().unwrap_or("");
            let idx: isize = first.parse().unwrap_or(0);
            cells.push(JlCell {
                index: idx,
                kind: CellKind::Code,
                code: String::new(),
                start_line: i,
                marker_comment: extract_marker_comment(rest),
                images: Vec::new(),
            });
        } else if line.trim_end() == "@markdown" {
            cells.push(JlCell {
                index: 0,
                kind: CellKind::Markdown,
                code: String::new(),
                start_line: i,
                marker_comment: String::new(),
                images: Vec::new(),
            });
        } else if let Some(rest) = line.strip_prefix("@markdown ") {
            let first = rest.split_whitespace().next().unwrap_or("");
            let idx: isize = first.parse().unwrap_or(0);
            cells.push(JlCell {
                index: idx,
                kind: CellKind::Markdown,
                code: String::new(),
                start_line: i,
                marker_comment: extract_marker_comment(rest),
                images: Vec::new(),
            });
        } else if line.trim_end() == "@typst" {
            cells.push(JlCell {
                index: 0,
                kind: CellKind::Typst,
                code: String::new(),
                start_line: i,
                marker_comment: String::new(),
                images: Vec::new(),
            });
        } else if let Some(rest) = line.strip_prefix("@typst ") {
            let first = rest.split_whitespace().next().unwrap_or("");
            let idx: isize = first.parse().unwrap_or(0);
            cells.push(JlCell {
                index: idx,
                kind: CellKind::Typst,
                code: String::new(),
                start_line: i,
                marker_comment: extract_marker_comment(rest),
                images: Vec::new(),
            });
        }
    }

    // If there's non-empty code before the first cell marker, insert
    // an implicit preamble cell at index -1. This handles `using X`
    // lines at the top of the file that need to execute before any cell.
    //
    // `using NothelixMacros` is special-cased out: the converter injects
    // it so Julia's LanguageServer resolves @cell/@markdown macros
    // without false "Missing reference" squiggles, but it's not user
    // code. Letting it become a preamble cell pollutes .ipynb round-
    // trips — the package only exists in nothelix's bootstrap env, so
    // running that cell in stock Julia fails with "Package
    // NothelixMacros not found in current path".
    let first_marker_line = cells.first().map(|c| c.start_line).unwrap_or(lines.len());
    if first_marker_line > 0 {
        let preamble: String = lines[..first_marker_line]
            .iter()
            .filter(|l| {
                let t = l.trim();
                if t.is_empty() || t.starts_with('#') {
                    return false;
                }
                if t == "using NothelixMacros" || t.starts_with("using NothelixMacros ") {
                    return false;
                }
                true
            })
            .cloned()
            .collect::<Vec<_>>()
            .join("\n");
        if !preamble.trim().is_empty() {
            cells.insert(
                0,
                JlCell {
                    index: -1,
                    kind: CellKind::Code,
                    code: preamble,
                    start_line: 0,
                    marker_comment: String::new(),
                    images: Vec::new(),
                },
            );
        }
    }

    // Collect code for each cell (strip output sections *and* any
    // stray marker-shaped lines that slipped into the cell body).
    // The stray-marker strip is a defense against users typing a new
    // `@cell` inside an existing cell without triggering the autofill
    // expansion — without it those lines would be forwarded to the
    // Julia kernel, which would then choke on `@cell` as a malformed
    // macro invocation.
    // Collect boundaries first so we can mutate cells below.
    let boundaries: Vec<(usize, usize)> = cells
        .iter()
        .enumerate()
        .map(|(ci, cell)| {
            let code_start = cell.start_line + 1;
            let code_end = cells.get(ci + 1).map_or(lines.len(), |c| c.start_line);
            (code_start, code_end)
        })
        .collect();

    for (ci, (code_start, code_end)) in boundaries.into_iter().enumerate() {

        let is_marker_line = |line: &str| -> bool {
            let t = line.trim_end();
            t == "@cell"
                || t == "@markdown"
                || t == "@typst"
                || line.starts_with("@cell ")
                || line.starts_with("@markdown ")
                || line.starts_with("@typst ")
        };

        let mut filtered: Vec<&str> = Vec::new();
        let mut images: Vec<String> = Vec::new();
        let mut in_output = false;
        for line in &lines[code_start..code_end] {
            if line.contains("# ─── Output") {
                in_output = true;
                continue;
            }
            if in_output {
                if line.contains("# ─────────────") {
                    in_output = false;
                }
                continue;
            }
            if is_marker_line(line) {
                continue;
            }
            if let Some(rest) = line.strip_prefix("# @image ") {
                let path = rest.trim_end_matches('\r').trim();
                if !path.is_empty() {
                    images.push(path.to_string());
                }
                continue;
            }
            filtered.push(line);
        }
        cells[ci].images = images;

        // Trim trailing blank lines.
        while filtered
            .last()
            .map(|l: &&str| l.trim().is_empty())
            .unwrap_or(false)
        {
            filtered.pop();
        }

        let mut code = filtered.join("\n");

        // Prepend marker-line comment as the first line of cell content.
        // For "@markdown 3 # Q1", this makes "# Q1" appear as "# # Q1"
        // in the cell body (a markdown heading when the # prefix is stripped).
        if !cells[ci].marker_comment.is_empty() {
            let comment = &cells[ci].marker_comment;
            let prefix_line = if cells[ci].kind == CellKind::Markdown || cells[ci].kind == CellKind::Typst {
                format!("# {comment}")
            } else {
                comment.to_string()
            };
            if code.is_empty() {
                code = prefix_line;
            } else {
                code = format!("{prefix_line}\n{code}");
            }
        }

        cells[ci].code = code;
    }

    Ok((cells, source_path))
}

// ─── FFI-facing functions ─────────────────────────────────────────────────────

pub fn notebook_validate(path: String) -> String {
    match read_notebook(&path) {
        Err(e) => e,
        Ok(nb) => {
            if nb.get("cells").is_none() {
                return "Missing 'cells' field".to_string();
            }
            if nb.get("nbformat").is_none() {
                return "Missing 'nbformat' field".to_string();
            }
            String::new() // empty = valid
        }
    }
}

pub fn notebook_cell_count(path: String) -> isize {
    read_notebook(&path)
        .ok()
        .and_then(|nb| nb["cells"].as_array().map(|a| a.len() as isize))
        .unwrap_or(0)
}

pub fn notebook_get_cell_code(path: String, cell_index: isize) -> String {
    read_notebook(&path)
        .ok()
        .and_then(|nb| {
            nb["cells"]
                .as_array()
                .cloned()
                .and_then(|cells| cells.into_iter().nth(cell_index as usize))
                .map(|cell| source_to_string(&cell["source"]))
        })
        .unwrap_or_default()
}

/// Convert `.ipynb` → Nothelix `.jl` cell format (returns the text content).
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

pub fn get_cell_at_line(path: String, line: isize) -> String {
    let line = line as usize;
    match parse_jl_file(&path) {
        Err(e) => json!({"cell_index": "", "source_path": "", "error": e}).to_string(),
        Ok((cells, source_path)) => {
            let mut found: isize = 0;
            for (ci, cell) in cells.iter().enumerate() {
                let next_start = cells
                    .get(ci + 1)
                    .map(|c| c.start_line)
                    .unwrap_or(usize::MAX);
                if line >= cell.start_line && line < next_start {
                    found = cell.index;
                    break;
                }
            }
            json!({
                "cell_index": found.to_string(),
                "source_path": source_path,
                "error": ""
            })
            .to_string()
        }
    }
}

pub fn get_cell_code_from_jl(jl_path: String, cell_index: isize) -> String {
    match parse_jl_file(&jl_path) {
        Err(e) => json!({"code": "", "error": e}).to_string(),
        Ok((cells, _)) => match cells.iter().find(|c| c.index == cell_index) {
            None => {
                json!({"code": "", "error": format!("Cell {cell_index} not found")}).to_string()
            }
            Some(c) => json!({"code": c.code, "error": ""}).to_string(),
        },
    }
}

pub fn list_jl_code_cells(jl_path: String, limit: isize) -> String {
    match parse_jl_file(&jl_path) {
        Err(e) => json!({"indices": "", "error": e}).to_string(),
        Ok((cells, _)) => {
            let cap = if limit <= 0 {
                usize::MAX
            } else {
                limit as usize
            };
            let indices: Vec<String> = cells
                .iter()
                .filter(|c| c.kind == CellKind::Code)
                .take(cap)
                .map(|c| c.index.to_string())
                .collect();
            json!({"indices": indices.join(","), "error": ""}).to_string()
        }
    }
}

/// Sync a `.jl` file back to its originating `.ipynb`.
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

/// Export a `.jl` notebook to Markdown (`.md`).
///
/// Markdown cells have their `# ` prefix stripped and are emitted as-is.
/// Code cells are wrapped in ```julia fenced code blocks.
/// Output sections are stripped.
/// Scan a `.jl` notebook for where `var_name` is first *assigned*.
///
/// Finds the earliest cell whose code contains an assignment to `var_name`
/// (plain `=`, compound `.=`, `+=` etc.) — ignores equality comparisons,
/// type annotations without `=`, and occurrences inside strings/comments
/// as best we can with line-level heuristics.
///
/// Used by the error-format pipeline to point the user at "your variable
/// IS defined, just in cell N which is later than where you used it — move
/// it up or run that cell first" rather than the generic "spelling / add
/// `using PackageName`" message.
///
/// Returns JSON of the form
/// `{"cell_index": N, "line_in_cell": L, "line_text": "..."}`
/// or the string `"null"` when not found.
pub fn scan_variable_definition(jl_path: String, var_name: String) -> String {
    let cells = match parse_jl_file(&jl_path) {
        Err(_) => return "null".to_string(),
        Ok((c, _)) => c,
    };
    for cell in &cells {
        if cell.kind != CellKind::Code {
            continue;
        }
        if let Some((line_no, line_text)) = find_assignment_line(&cell.code, &var_name) {
            return format!(
                r#"{{"cell_index":{},"line_in_cell":{},"line_text":{}}}"#,
                cell.index,
                line_no,
                serde_json::to_string(line_text.trim()).unwrap_or_else(|_| "\"\"".to_string())
            );
        }
    }
    "null".to_string()
}

/// Find the first line in `code` that assigns to `var_name`. Returns
/// `(line_number_0_indexed, line_text)` or `None`.
///
/// Recognizes:
///   - `var = expr`        (plain assignment)
///   - `var .= expr`       (broadcast assignment)
///   - `var += expr`       (compound assignments)
///   - `var, other = ...`  (destructuring — first LHS position)
///
/// Rejects:
///   - `var == expr`       (equality comparison)
///   - `function var(...)` (function definition — we want variable
///     introductions, though functions ARE technically binding `var`;
///     caller can iterate again if the variable slot turns out to be a
///     function, but the common UX case is `x = …` style assignments)
///   - Matches inside `#` comment tails (best-effort — we just strip the
///     comment tail before scanning)
fn find_assignment_line(code: &str, var_name: &str) -> Option<(usize, String)> {
    for (idx, raw_line) in code.lines().enumerate() {
        // Strip inline comments — `x = 5 # note` → `x = 5 `
        let line = match raw_line.find('#') {
            Some(pos) => &raw_line[..pos],
            None => raw_line,
        };
        if line_assigns_to(line, var_name) {
            return Some((idx, raw_line.to_string()));
        }
    }
    None
}

/// Token-level check: does `line` contain `var_name` followed by an
/// assignment operator (but not `==`)?
fn line_assigns_to(line: &str, var_name: &str) -> bool {
    let bytes = line.as_bytes();
    let mut i = 0;
    let name_bytes = var_name.as_bytes();
    while i + name_bytes.len() <= bytes.len() {
        // Match `var_name` on an identifier boundary.
        let prev_ok = i == 0 || !is_ident_byte(bytes[i - 1]);
        if prev_ok && &bytes[i..i + name_bytes.len()] == name_bytes {
            let after = i + name_bytes.len();
            // Next char must NOT be part of an identifier (so we don't
            // match `var_namex`).
            if after < bytes.len() && is_ident_byte(bytes[after]) {
                i += 1;
                continue;
            }
            // Skip spaces and look for `=` that isn't `==`.
            let mut j = after;
            while j < bytes.len() && (bytes[j] == b' ' || bytes[j] == b'\t') {
                j += 1;
            }
            // Accept `=`, `.=`, `+=`, `-=`, `*=`, `/=`, `^=`, `%=`, etc.
            // Reject `==`.
            if j < bytes.len() {
                let c = bytes[j];
                let has_dot = j >= 1 && bytes[j.saturating_sub(1)] == b'.';
                if c == b'=' && bytes.get(j + 1) != Some(&b'=') {
                    return true;
                }
                if matches!(c, b'+' | b'-' | b'*' | b'/' | b'^' | b'%')
                    && bytes.get(j + 1) == Some(&b'=')
                {
                    return true;
                }
                if has_dot && c == b'=' {
                    return true;
                }
            }
        }
        i += 1;
    }
    false
}

#[inline]
fn is_ident_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

pub fn export_to_markdown(jl_path: String) -> String {
    let (cells, _) = match parse_jl_file(&jl_path) {
        Err(e) => return format!("ERROR: {e}"),
        Ok(v) => v,
    };

    let mut out = String::new();

    for cell in &cells {
        match cell.kind {
            CellKind::Markdown | CellKind::Typst => {
                for line in cell.code.lines() {
                    out.push_str(line.strip_prefix("# ").unwrap_or(line));
                    out.push('\n');
                }
                out.push('\n');
            }
            CellKind::Code => {
                if cell.code.trim().is_empty() {
                    continue;
                }
                out.push_str("```julia\n");
                out.push_str(&cell.code);
                if !cell.code.ends_with('\n') {
                    out.push('\n');
                }
                out.push_str("```\n\n");
            }
        }
    }

    let out_path = jl_path.replace(".jl", ".md");
    match fs::write(&out_path, &out) {
        Ok(_) => format!("Exported to {out_path}"),
        Err(e) => format!("ERROR: Cannot write {out_path}: {e}"),
    }
}

/// Export a `.jl` notebook to Typst (`.typ`).
pub fn export_to_typst(jl_path: String) -> String {
    let (cells, _) = match parse_jl_file(&jl_path) {
        Err(e) => return format!("ERROR: {e}"),
        Ok(v) => v,
    };

    let mut out = String::new();

    for cell in &cells {
        match cell.kind {
            CellKind::Markdown | CellKind::Typst => {
                let stripped: String = cell
                    .code
                    .lines()
                    .map(|l| l.strip_prefix("# ").unwrap_or(l))
                    .collect::<Vec<_>>()
                    .join("\n");
                out.push_str(&crate::typst_export::md_to_typst(&stripped));
                out.push('\n');
            }
            CellKind::Code => {
                if cell.code.trim().is_empty() {
                    continue;
                }
                out.push_str("```julia\n");
                out.push_str(&cell.code);
                if !cell.code.ends_with('\n') {
                    out.push('\n');
                }
                out.push_str("```\n\n");
            }
        }
    }

    let out_path = jl_path.replace(".jl", ".typ");
    match fs::write(&out_path, &out) {
        Ok(_) => format!("Exported to {out_path}"),
        Err(e) => format!("ERROR: Cannot write {out_path}: {e}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture_path(name: &str) -> String {
        let manifest = env!("CARGO_MANIFEST_DIR");
        format!("{manifest}/tests/fixtures/{name}")
    }

    #[test]
    fn validate_valid_notebook() {
        let result = notebook_validate(fixture_path("simple.ipynb"));
        assert_eq!(
            result, "",
            "Expected empty string for valid notebook, got: {result}"
        );
    }

    #[test]
    fn validate_nonexistent_file() {
        let result = notebook_validate("/nonexistent/file.ipynb".into());
        assert!(
            result.contains("Cannot read"),
            "Expected read error, got: {result}"
        );
    }

    #[test]
    fn cell_count() {
        assert_eq!(notebook_cell_count(fixture_path("simple.ipynb")), 4);
    }

    #[test]
    fn cell_count_nonexistent() {
        assert_eq!(notebook_cell_count("/nonexistent.ipynb".into()), 0);
    }

    #[test]
    fn get_cell_code_first_cell() {
        let code = notebook_get_cell_code(fixture_path("simple.ipynb"), 0);
        assert_eq!(code, "using Plots");
    }

    #[test]
    fn get_cell_code_multiline() {
        let code = notebook_get_cell_code(fixture_path("simple.ipynb"), 1);
        assert_eq!(code, "x = 1:10\ny = x.^2");
    }

    #[test]
    fn get_cell_code_out_of_range() {
        let code = notebook_get_cell_code(fixture_path("simple.ipynb"), 99);
        assert_eq!(code, "");
    }

    #[test]
    fn convert_sync_produces_cell_markers() {
        let result = notebook_convert_sync(fixture_path("simple.ipynb"));
        assert!(!result.starts_with("ERROR"), "Conversion failed: {result}");
        assert!(result.contains("@cell 0 :julia"));
        assert!(result.contains("@cell 1 :julia"));
        assert!(result.contains("@markdown 2"));
        assert!(result.contains("@cell 3 :julia"));
        assert!(result.contains("using Plots"));
        assert!(result.contains("plot(x, y)"));
    }

    #[test]
    fn convert_sync_header() {
        let result = notebook_convert_sync(fixture_path("simple.ipynb"));
        assert!(result.starts_with("using NothelixMacros"));
        assert!(result.contains("# ═══ Nothelix Notebook:"));
        assert!(result.contains("# Cells: 4"));
    }

    #[test]
    fn convert_sync_markdown_commented() {
        let result = notebook_convert_sync(fixture_path("simple.ipynb"));
        assert!(result.contains("# # Results"));
        assert!(result.contains("# This shows the quadratic function."));
    }

    #[test]
    fn parse_jl_file_roundtrip() {
        let (cells, source_path) = parse_jl_file(&fixture_path("simple.jl")).unwrap();
        assert_eq!(cells.len(), 4);
        assert!(source_path.ends_with("simple.ipynb"));

        // Cell 0: code
        assert_eq!(cells[0].index, 0);
        assert_eq!(cells[0].kind, CellKind::Code);
        assert_eq!(cells[0].code, "using Plots");

        // Cell 1: code, multiline
        assert_eq!(cells[1].index, 1);
        assert_eq!(cells[1].code, "x = 1:10\ny = x.^2");

        // Cell 2: markdown
        assert_eq!(cells[2].index, 2);
        assert_eq!(cells[2].kind, CellKind::Markdown);

        // Cell 3: code
        assert_eq!(cells[3].index, 3);
        assert_eq!(cells[3].code, "plot(x, y)");
    }

    #[test]
    fn get_cell_at_line_first_cell() {
        let result = get_cell_at_line(fixture_path("simple.jl"), 4);
        let parsed: Value = serde_json::from_str(&result).unwrap();
        assert_eq!(parsed["cell_index"].as_str().unwrap(), "0");
        assert_eq!(parsed["error"].as_str().unwrap(), "");
    }

    #[test]
    fn get_cell_at_line_second_cell() {
        let result = get_cell_at_line(fixture_path("simple.jl"), 11);
        let parsed: Value = serde_json::from_str(&result).unwrap();
        assert_eq!(parsed["cell_index"].as_str().unwrap(), "1");
    }

    #[test]
    fn get_cell_at_line_markdown() {
        let result = get_cell_at_line(fixture_path("simple.jl"), 30);
        let parsed: Value = serde_json::from_str(&result).unwrap();
        assert_eq!(parsed["cell_index"].as_str().unwrap(), "2");
    }

    #[test]
    fn get_cell_code_from_jl_valid() {
        let result = get_cell_code_from_jl(fixture_path("simple.jl"), 3);
        let parsed: Value = serde_json::from_str(&result).unwrap();
        assert_eq!(parsed["code"].as_str().unwrap(), "plot(x, y)");
        assert_eq!(parsed["error"].as_str().unwrap(), "");
    }

    #[test]
    fn get_cell_code_from_jl_missing() {
        let result = get_cell_code_from_jl(fixture_path("simple.jl"), 99);
        let parsed: Value = serde_json::from_str(&result).unwrap();
        assert!(parsed["error"].as_str().unwrap().contains("not found"));
    }

    #[test]
    fn list_jl_code_cells_all() {
        let result = list_jl_code_cells(fixture_path("simple.jl"), 0);
        let parsed: Value = serde_json::from_str(&result).unwrap();
        // Should list cells 0, 1, 3 (not markdown cell 2)
        assert_eq!(parsed["indices"].as_str().unwrap(), "0,1,3");
    }

    #[test]
    fn list_jl_code_cells_limited() {
        let result = list_jl_code_cells(fixture_path("simple.jl"), 2);
        let parsed: Value = serde_json::from_str(&result).unwrap();
        assert_eq!(parsed["indices"].as_str().unwrap(), "0,1");
    }

    #[test]
    fn source_to_string_array() {
        let v = serde_json::json!(["line1\n", "line2"]);
        assert_eq!(source_to_string(&v), "line1\nline2");
    }

    #[test]
    fn source_to_string_string() {
        let v = serde_json::json!("single string");
        assert_eq!(source_to_string(&v), "single string");
    }

    #[test]
    fn source_to_string_null() {
        assert_eq!(source_to_string(&Value::Null), "");
    }

    #[test]
    fn convert_to_ipynb_roundtrip() {
        // Write the .jl to a temp file, convert back, verify structure
        let tmp = tempfile::NamedTempFile::new().unwrap();
        let jl_content = std::fs::read_to_string(fixture_path("simple.jl")).unwrap();

        // Modify the header to point to a temp .ipynb
        let tmp_ipynb = tmp.path().with_extension("ipynb");
        std::fs::copy(fixture_path("simple.ipynb"), &tmp_ipynb).unwrap();

        let jl_path = tmp.path().with_extension("jl");
        let modified =
            jl_content.replace(&fixture_path("simple.ipynb"), &tmp_ipynb.to_string_lossy());
        std::fs::write(&jl_path, &modified).unwrap();

        let result = convert_to_ipynb(jl_path.to_string_lossy().into());
        assert!(
            result.starts_with("Synced to"),
            "Expected success, got: {result}"
        );

        // Verify the output is valid JSON with 4 cells
        let nb: Value =
            serde_json::from_str(&std::fs::read_to_string(&tmp_ipynb).unwrap()).unwrap();
        assert_eq!(nb["cells"].as_array().unwrap().len(), 4);
        assert_eq!(nb["cells"][0]["cell_type"], "code");
        assert_eq!(nb["cells"][2]["cell_type"], "markdown");
    }

    #[test]
    fn preamble_filter_drops_nothelix_macros_pragma() {
        // The converter injects `using NothelixMacros` as an LSP-
        // visibility pragma at the top of the .jl. That line is not
        // user code — it MUST NOT be turned into a synthesized
        // preamble cell, or the resulting .ipynb will have a cell that
        // fails to run in stock Julia ("Package NothelixMacros not
        // found").
        let tmp = tempfile::NamedTempFile::new().unwrap();
        let jl_path = tmp.path().with_extension("jl");
        let src = "using NothelixMacros  # cell markers for static checking\n\n\
                   # ═══ Nothelix Notebook: example.ipynb ═══\n# Cells: 1\n\n\
                   @cell 0 :julia\nx = 1\n";
        std::fs::write(&jl_path, src).unwrap();
        let (cells, _) = parse_jl_file(&jl_path.to_string_lossy()).unwrap();
        assert_eq!(cells.len(), 1, "should not emit preamble cell for pragma-only preamble, got: {}", cells.len());
        assert_eq!(cells[0].index, 0);
        assert_eq!(cells[0].code, "x = 1");
    }

    #[test]
    fn preamble_filter_keeps_real_user_preamble() {
        // User code that lives above the first @cell marker should
        // still round-trip through an index=-1 preamble cell. The
        // filter must only drop nothelix's own pragma, not arbitrary
        // user `using ...` lines.
        let tmp = tempfile::NamedTempFile::new().unwrap();
        let jl_path = tmp.path().with_extension("jl");
        let src = "using NothelixMacros\nconst MY_CONST = 42\nusing LinearAlgebra\n\n\
                   @cell 0 :julia\nA = I\n";
        std::fs::write(&jl_path, src).unwrap();
        let (cells, _) = parse_jl_file(&jl_path.to_string_lossy()).unwrap();
        assert_eq!(cells.len(), 2, "expected preamble cell + @cell 0, got {} cells", cells.len());
        assert_eq!(cells[0].index, -1);
        assert!(cells[0].code.contains("const MY_CONST = 42"));
        assert!(cells[0].code.contains("using LinearAlgebra"));
        assert!(!cells[0].code.contains("NothelixMacros"), "pragma must not leak into preamble cell");
    }

    #[test]
    fn convert_to_ipynb_drops_stale_outputs_when_code_edited() {
        // Round-trip integrity: if a code cell's source was edited in
        // the .jl after the .ipynb was last written, the orig's
        // `outputs`/`execution_count` are stale and must not be carried
        // forward. Otherwise the .ipynb claims a stale output is the
        // current result of code that's since been changed.
        let tmp_ipynb = tempfile::NamedTempFile::new().unwrap().path().with_extension("ipynb");
        let orig = serde_json::json!({
            "nbformat": 4,
            "nbformat_minor": 5,
            "metadata": {},
            "cells": [{
                "cell_type": "code",
                "execution_count": 7,
                "metadata": {},
                "outputs": [{"output_type": "stream", "name": "stdout", "text": "stale output\n"}],
                "source": ["old_code = 1\n"]
            }]
        });
        std::fs::write(&tmp_ipynb, serde_json::to_string_pretty(&orig).unwrap()).unwrap();

        let jl_path = tmp_ipynb.with_extension("jl");
        let jl_content = format!(
            "# ═══ Nothelix Notebook: {} ═══\n# Cells: 1\n\n@cell 0 :julia\nnew_code = 2\n",
            tmp_ipynb.display()
        );
        std::fs::write(&jl_path, jl_content).unwrap();

        let result = convert_to_ipynb(jl_path.to_string_lossy().into());
        assert!(result.starts_with("Synced to"), "got: {result}");

        let nb: Value = serde_json::from_str(&std::fs::read_to_string(&tmp_ipynb).unwrap()).unwrap();
        let cell = &nb["cells"][0];
        assert_eq!(cell["source"][0].as_str().unwrap(), "new_code = 2");
        // Code changed → orig's outputs/execution_count are stale → must be cleared.
        assert!(cell["outputs"].as_array().unwrap().is_empty(), "stale outputs should be dropped, got: {cell}");
        assert!(cell["execution_count"].is_null(), "stale execution_count should be cleared, got: {cell}");
    }

    #[test]
    fn convert_to_ipynb_preserves_outputs_when_code_unchanged() {
        // The flip side: if the orig cell's source matches the .jl
        // cell's source exactly (trimmed), orig's outputs and
        // execution_count ARE still valid and should survive round-trip.
        let tmp_ipynb = tempfile::NamedTempFile::new().unwrap().path().with_extension("ipynb");
        let orig = serde_json::json!({
            "nbformat": 4,
            "nbformat_minor": 5,
            "metadata": {},
            "cells": [{
                "cell_type": "code",
                "execution_count": 7,
                "metadata": {"tags": ["important"]},
                "outputs": [{"output_type": "stream", "name": "stdout", "text": "hi\n"}],
                "source": ["x = 1\n"]
            }]
        });
        std::fs::write(&tmp_ipynb, serde_json::to_string_pretty(&orig).unwrap()).unwrap();

        let jl_path = tmp_ipynb.with_extension("jl");
        let jl_content = format!(
            "# ═══ Nothelix Notebook: {} ═══\n# Cells: 1\n\n@cell 0 :julia\nx = 1\n",
            tmp_ipynb.display()
        );
        std::fs::write(&jl_path, jl_content).unwrap();

        let result = convert_to_ipynb(jl_path.to_string_lossy().into());
        assert!(result.starts_with("Synced to"), "got: {result}");

        let nb: Value = serde_json::from_str(&std::fs::read_to_string(&tmp_ipynb).unwrap()).unwrap();
        let cell = &nb["cells"][0];
        assert_eq!(cell["execution_count"].as_i64(), Some(7), "execution_count should survive: {cell}");
        assert_eq!(cell["metadata"]["tags"][0].as_str(), Some("important"), "metadata should survive: {cell}");
        assert_eq!(cell["outputs"].as_array().unwrap().len(), 1, "outputs should survive: {cell}");
    }

    #[test]
    fn convert_to_ipynb_clears_code_fields_when_turned_into_markdown() {
        // Cell-type change: orig was code (has outputs/execution_count),
        // user converted to markdown in .jl. Resulting markdown cell
        // must not carry those now-meaningless fields.
        let tmp_ipynb = tempfile::NamedTempFile::new().unwrap().path().with_extension("ipynb");
        let orig = serde_json::json!({
            "nbformat": 4,
            "nbformat_minor": 5,
            "metadata": {},
            "cells": [{
                "cell_type": "code",
                "execution_count": 3,
                "metadata": {},
                "outputs": [{"output_type": "stream", "name": "stdout", "text": "old\n"}],
                "source": ["println(\"old\")\n"]
            }]
        });
        std::fs::write(&tmp_ipynb, serde_json::to_string_pretty(&orig).unwrap()).unwrap();

        let jl_path = tmp_ipynb.with_extension("jl");
        let jl_content = format!(
            "# ═══ Nothelix Notebook: {} ═══\n# Cells: 1\n\n@markdown 0\n# Now a heading\n",
            tmp_ipynb.display()
        );
        std::fs::write(&jl_path, jl_content).unwrap();

        let result = convert_to_ipynb(jl_path.to_string_lossy().into());
        assert!(result.starts_with("Synced to"), "got: {result}");

        let nb: Value = serde_json::from_str(&std::fs::read_to_string(&tmp_ipynb).unwrap()).unwrap();
        let cell = &nb["cells"][0];
        assert_eq!(cell["cell_type"].as_str(), Some("markdown"));
        assert!(cell.get("outputs").is_none(), "markdown cell should not have outputs: {cell}");
        assert!(cell.get("execution_count").is_none(), "markdown cell should not have execution_count: {cell}");
    }

    #[test]
    fn convert_to_ipynb_embeds_nothelix_image_sidecar() {
        // User workflow: executes a plot-producing cell in nothelix, the
        // kernel writes .nothelix/images/cell-5.png, then they :sync-to-
        // ipynb. The resulting .ipynb should carry the plot as a
        // display_data base64 PNG so the notebook stays portable —
        // opened in vanilla Jupyter or pushed to a repo, the image
        // still renders without the sidecar directory.
        let tmp_dir = tempfile::TempDir::new().unwrap();
        let dir = tmp_dir.path();
        let tmp_ipynb = dir.join("nb.ipynb");
        let jl_path = dir.join("nb.jl");

        // Minimal orig .ipynb — outputs are empty so we can tell the
        // sidecar is what populated them.
        let orig = serde_json::json!({
            "nbformat": 4,
            "nbformat_minor": 5,
            "metadata": {},
            "cells": [{
                "cell_type": "code",
                "execution_count": null,
                "metadata": {},
                "outputs": [],
                "source": ["plot(x, y)\n"]
            }]
        });
        std::fs::write(&tmp_ipynb, serde_json::to_string_pretty(&orig).unwrap()).unwrap();

        // Drop a fake PNG into the sidecar dir at cell index 5. The
        // exact bytes don't matter — we only check the .ipynb round-
        // trip base64-encodes them.
        let img_dir = dir.join(".nothelix").join("images");
        std::fs::create_dir_all(&img_dir).unwrap();
        let fake_png: &[u8] = &[0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A]; // PNG signature
        std::fs::write(img_dir.join("cell-5.png"), fake_png).unwrap();

        let jl_content = format!(
            "# ═══ Nothelix Notebook: {} ═══\n# Cells: 1\n\n@cell 5 :julia\nplot(x, y)\n",
            tmp_ipynb.display()
        );
        std::fs::write(&jl_path, jl_content).unwrap();

        let result = convert_to_ipynb(jl_path.to_string_lossy().into());
        assert!(result.starts_with("Synced to"), "got: {result}");

        let nb: Value = serde_json::from_str(&std::fs::read_to_string(&tmp_ipynb).unwrap()).unwrap();
        let outputs = nb["cells"][0]["outputs"].as_array().unwrap();
        assert_eq!(outputs.len(), 1, "should attach exactly one display_data output, got: {outputs:#?}");
        assert_eq!(outputs[0]["output_type"].as_str(), Some("display_data"));
        let b64 = outputs[0]["data"]["image/png"].as_str().unwrap();
        assert!(!b64.is_empty(), "PNG data should be base64-encoded, got: {b64:?}");
        // Round-trip: decoded bytes equal the fake PNG we wrote.
        let decoded = base64::engine::general_purpose::STANDARD.decode(b64).unwrap();
        assert_eq!(decoded, fake_png);
    }

    #[test]
    fn convert_to_ipynb_embeds_markdown_image_as_attachment() {
        // User workflow: :insert-image diagram.png inside a markdown
        // cell. On sync-to-ipynb the image should land as a base64
        // attachment keyed by filename, with the markdown source
        // referencing `attachment:diagram.png` — matches Jupyter's
        // native convention and survives gist/GitHub rendering.
        let tmp_dir = tempfile::TempDir::new().unwrap();
        let dir = tmp_dir.path();
        let tmp_ipynb = dir.join("nb.ipynb");
        let jl_path = dir.join("nb.jl");
        let img_path = dir.join("diagram.png");

        let fake_png: [u8; 8] = [0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A];
        std::fs::write(&img_path, &fake_png).unwrap();

        // Minimal orig .ipynb so convert_to_ipynb has something to read.
        let orig = serde_json::json!({
            "nbformat": 4,
            "nbformat_minor": 5,
            "metadata": {},
            "cells": [{"cell_type": "markdown", "metadata": {}, "source": []}]
        });
        std::fs::write(&tmp_ipynb, serde_json::to_string_pretty(&orig).unwrap()).unwrap();

        // Markdown cell body with an @image marker — this is what
        // :insert-image would produce inside a markdown cell.
        let jl_content = format!(
            "# ═══ Nothelix Notebook: {} ═══\n# Cells: 1\n\n@markdown 0\n# See the figure below.\n# @image diagram.png\n",
            tmp_ipynb.display()
        );
        std::fs::write(&jl_path, jl_content).unwrap();

        let result = convert_to_ipynb(jl_path.to_string_lossy().into());
        assert!(result.starts_with("Synced to"), "got: {result}");

        let nb: Value = serde_json::from_str(&std::fs::read_to_string(&tmp_ipynb).unwrap()).unwrap();
        let cell = &nb["cells"][0];
        assert_eq!(cell["cell_type"].as_str(), Some("markdown"));

        // attachments.diagram.png.image/png = base64 of fake_png
        let attachment_b64 = cell["attachments"]["diagram.png"]["image/png"]
            .as_str()
            .unwrap_or_else(|| panic!("expected attachments.diagram.png.image/png, got:\n{cell:#?}"));
        let decoded = base64::engine::general_purpose::STANDARD.decode(attachment_b64).unwrap();
        assert_eq!(decoded, &fake_png[..]);

        // The markdown body now references attachment:diagram.png.
        let source_joined: String = cell["source"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap_or(""))
            .collect();
        assert!(
            source_joined.contains("![](attachment:diagram.png)"),
            "expected markdown body to reference the attachment, got:\n{source_joined}"
        );
    }

    #[test]
    fn convert_to_ipynb_skips_missing_image_files() {
        // If a `# @image` path points at a file that doesn't exist,
        // don't bail the whole conversion — just skip that attachment
        // and leave the markdown body unchanged. The `.jl` can still
        // render via the sidecar marker even if the `.ipynb` can't
        // embed it.
        let tmp_dir = tempfile::TempDir::new().unwrap();
        let dir = tmp_dir.path();
        let tmp_ipynb = dir.join("nb.ipynb");
        let jl_path = dir.join("nb.jl");

        let orig = serde_json::json!({
            "nbformat": 4, "nbformat_minor": 5, "metadata": {},
            "cells": [{"cell_type": "markdown", "metadata": {}, "source": []}]
        });
        std::fs::write(&tmp_ipynb, serde_json::to_string_pretty(&orig).unwrap()).unwrap();

        let jl_content = format!(
            "# ═══ Nothelix Notebook: {} ═══\n# Cells: 1\n\n@markdown 0\n# Some prose.\n# @image vanished.png\n",
            tmp_ipynb.display()
        );
        std::fs::write(&jl_path, jl_content).unwrap();

        let result = convert_to_ipynb(jl_path.to_string_lossy().into());
        assert!(result.starts_with("Synced to"), "got: {result}");

        let nb: Value = serde_json::from_str(&std::fs::read_to_string(&tmp_ipynb).unwrap()).unwrap();
        let cell = &nb["cells"][0];
        assert!(cell.get("attachments").is_none(), "no attachment should be written for missing file, got: {cell}");
    }

    #[test]
    fn parse_bare_cell_marker_is_a_boundary_and_stripped_from_body() {
        // Regression test for the "LoadError: MethodError: no method
        // matching var\"@cell\"" crash that happens when a user typed
        // a bare `@cell` line mid-cell before the autofill hook
        // expanded it. The bare line must:
        //   1. act as a cell boundary (so we don't collapse two
        //      logical cells into one body),
        //   2. never appear in any cell's emitted code string (so
        //      the Julia kernel never tries to re-interpret it as
        //      a 0-arg macro call and blow up on the strict
        //      `@cell(index, exec_count, body)` definition from
        //      ~/.local/share/nothelix/kernel/cell_macros.jl).
        //
        // Also exercises `@cell 0:julia` (no space between the index
        // and the language tag — our parser's strip_prefix is tolerant
        // but early versions of this code fell over when the index
        // wasn't followed by whitespace).
        let src = "\
@cell 0:julia

using DSP

# building a matrix

@cell

A = zeros(8, 8)

display(A)
";
        let tmp = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(tmp.path(), src).unwrap();

        let (cells, _) = parse_jl_file(&tmp.path().to_string_lossy()).unwrap();

        // Two cells: the `@cell 0:julia` header and the bare `@cell`.
        assert_eq!(cells.len(), 2, "bare `@cell` must split into its own cell");

        // Neither cell's code should contain any `@cell` line — the
        // marker-stripping pass in parse_jl_file should have removed
        // them along with `# ─── Output ───` separators.
        for (i, cell) in cells.iter().enumerate() {
            assert!(
                !cell.code.contains("@cell"),
                "cell {i} still contains @cell: {:?}",
                cell.code
            );
            assert!(
                !cell.code.contains("@markdown"),
                "cell {i} still contains @markdown: {:?}",
                cell.code
            );
        }

        // Cell 0 should have the imports and comment; cell 1 should
        // have the matrix code. Confirm the content actually made it
        // through (i.e. we didn't over-strip everything).
        assert!(cells[0].code.contains("using DSP"));
        assert!(cells[1].code.contains("A = zeros(8, 8)"));
        assert!(cells[1].code.contains("display(A)"));
    }
}
