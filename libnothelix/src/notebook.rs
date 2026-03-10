//! Notebook parsing and conversion (.ipynb ↔ .jl).
//!
//! Implements the Nothelix `.jl` cell format:
//!
//! ```text
//! # ═══ Nothelix Notebook: /full/path/to/notebook.ipynb ═══
//! # Cells: N
//!
//! @cell 0 julia
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

use serde_json::{json, Value};

// ─── Cell types ───────────────────────────────────────────────────────────────

#[derive(PartialEq)]
pub enum CellKind {
    Code,
    Markdown,
}

pub struct JlCell {
    pub index: isize,
    pub kind: CellKind,
    #[allow(dead_code)]
    pub lang: String,
    /// Cell code, with output sections stripped.
    pub code: String,
    /// 0-indexed line of the `@cell` / `@markdown` marker.
    pub start_line: usize,
}

// ─── Helpers ──────────────────────────────────────────────────────────────────

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
    let mut cells: Vec<JlCell> = Vec::new();
    for (i, line) in lines.iter().enumerate() {
        if let Some(rest) = line.strip_prefix("@cell ") {
            let mut parts = rest.splitn(2, ' ');
            let idx: isize = parts.next().unwrap_or("0").parse().unwrap_or(0);
            let lang = parts.next().unwrap_or("julia").trim().to_string();
            cells.push(JlCell {
                index: idx,
                kind: CellKind::Code,
                lang,
                code: String::new(),
                start_line: i,
            });
        } else if let Some(rest) = line.strip_prefix("@markdown ") {
            let idx: isize = rest.trim().parse().unwrap_or(0);
            cells.push(JlCell {
                index: idx,
                kind: CellKind::Markdown,
                lang: String::new(),
                code: String::new(),
                start_line: i,
            });
        }
    }

    // Collect code for each cell (strip output sections).
    let n = cells.len();
    for ci in 0..n {
        let code_start = cells[ci].start_line + 1;
        let code_end = cells
            .get(ci + 1)
            .map(|c| c.start_line)
            .unwrap_or(lines.len());

        let mut filtered: Vec<&str> = Vec::new();
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
            filtered.push(line);
        }

        // Trim trailing blank lines.
        while filtered
            .last()
            .map(|l: &&str| l.trim().is_empty())
            .unwrap_or(false)
        {
            filtered.pop();
        }

        cells[ci].code = filtered.join("\n");
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
            out.push_str(&format!("@cell {idx} {lang}\n"));
            out.push_str(&source);
            if !source.ends_with('\n') {
                out.push('\n');
            }
        }
        out.push('\n');
    }

    out
}

pub fn get_notebook_source_path(jl_path: String) -> String {
    if let Ok(content) = fs::read_to_string(&jl_path) {
        for line in content.lines() {
            if let Some(rest) = line.strip_prefix("# ═══ Nothelix Notebook: ") {
                let src = rest.trim_end_matches(" ═══").trim();
                if !src.is_empty() {
                    return src.to_string();
                }
            }
        }
    }
    jl_path.replace(".jl", ".ipynb")
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

    let new_cells: Vec<Value> = cells
        .iter()
        .map(|cell| {
            let orig = orig_cells.get(cell.index as usize).cloned();

            let make_source_lines = |text: &str| -> Value {
                let lines: Vec<Value> = text
                    .lines()
                    .enumerate()
                    .map(|(i, l)| {
                        let mut s = l.to_string();
                        if i < text.lines().count().saturating_sub(1) {
                            s.push('\n');
                        }
                        Value::String(s)
                    })
                    .collect();
                Value::Array(lines)
            };

            if cell.kind == CellKind::Markdown {
                // Strip leading "# " comment prefix from each line.
                let md: String = cell
                    .code
                    .lines()
                    .map(|l| l.strip_prefix("# ").unwrap_or(l))
                    .collect::<Vec<_>>()
                    .join("\n");
                let mut c = orig.unwrap_or_else(
                    || json!({"cell_type": "markdown", "metadata": {}, "source": []}),
                );
                c["cell_type"] = json!("markdown");
                c["source"] = make_source_lines(&md);
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
                c["source"] = make_source_lines(&cell.code);
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

pub fn notebook_cell_image_data(path: String, cell_index: isize) -> String {
    let nb = match read_notebook(&path) {
        Err(e) => return format!("ERROR: {e}"),
        Ok(v) => v,
    };

    let cells = match nb["cells"].as_array() {
        None => return String::new(),
        Some(c) => c,
    };

    let cell = match cells.get(cell_index as usize) {
        None => return String::new(),
        Some(c) => c,
    };

    for output in cell["outputs"].as_array().into_iter().flatten() {
        if let Some(data) = output.get("data") {
            for key in &["image/png", "image/jpeg", "image/gif"] {
                if let Some(img) = data.get(*key) {
                    if let Some(s) = img.as_str() {
                        if !s.is_empty() {
                            return s.to_string();
                        }
                    }
                    if let Some(lines) = img.as_array() {
                        let joined: String = lines.iter().filter_map(|v| v.as_str()).collect();
                        if !joined.is_empty() {
                            return joined;
                        }
                    }
                }
            }
        }
    }

    String::new()
}
