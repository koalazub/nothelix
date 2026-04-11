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

use serde_json::{json, Value};

// ─── Cell types ───────────────────────────────────────────────────────────────

#[derive(Debug, PartialEq)]
pub enum CellKind {
    Code,
    Markdown,
}

pub struct JlCell {
    pub index: isize,
    pub kind: CellKind,
    pub code: String,
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
            });
        } else if let Some(rest) = line.strip_prefix("@cell ") {
            let rest = rest.trim();
            // Parse the first whitespace-separated token. If it's
            // numeric, use it as the index; if it's a colon-prefixed
            // language tag (`:julia`), assume index 0 for now.
            let first = rest.split_whitespace().next().unwrap_or("");
            let idx: isize = first.parse().unwrap_or(0);
            cells.push(JlCell {
                index: idx,
                kind: CellKind::Code,
                code: String::new(),
                start_line: i,
            });
        } else if line.trim_end() == "@markdown" {
            cells.push(JlCell {
                index: 0,
                kind: CellKind::Markdown,
                code: String::new(),
                start_line: i,
            });
        } else if let Some(rest) = line.strip_prefix("@markdown ") {
            let idx: isize = rest.trim().parse().unwrap_or(0);
            cells.push(JlCell {
                index: idx,
                kind: CellKind::Markdown,
                code: String::new(),
                start_line: i,
            });
        }
    }

    // Collect code for each cell (strip output sections *and* any
    // stray marker-shaped lines that slipped into the cell body).
    // The stray-marker strip is a defense against users typing a new
    // `@cell` inside an existing cell without triggering the autofill
    // expansion — without it those lines would be forwarded to the
    // Julia kernel, which would then choke on `@cell` as a malformed
    // macro invocation.
    let n = cells.len();
    for ci in 0..n {
        let code_start = cells[ci].start_line + 1;
        let code_end = cells
            .get(ci + 1)
            .map(|c| c.start_line)
            .unwrap_or(lines.len());

        let is_marker_line = |line: &str| -> bool {
            let t = line.trim_end();
            t == "@cell"
                || t == "@markdown"
                || line.starts_with("@cell ")
                || line.starts_with("@markdown ")
        };

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
            if is_marker_line(line) {
                continue;
            }
            if line.starts_with("# @image ") {
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
            out.push_str(&format!("@cell {idx} :{lang}\n"));
            out.push_str(&source);
            if !source.ends_with('\n') {
                out.push('\n');
            }
        }
        out.push('\n');
    }

    out
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
        assert!(result.starts_with("# ═══ Nothelix Notebook:"));
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
