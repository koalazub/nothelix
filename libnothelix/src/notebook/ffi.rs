//! Steel-facing FFI getters for `.ipynb` / `.jl` notebooks.
//!
//! Thin wrappers around [`read_notebook`] / [`parse_jl_file`] that
//! return strings / ints in the shapes the plugin's Steel code expects.
//! The actual conversion logic lives in `convert` (.ipynb ↔ .jl) and
//! the parser lives in `cells`.

// Steel's `register_fn` marshals values from the Steel VM and requires
// the registered fn's signature to take owned types (`String`), not
// borrows.
#![allow(clippy::needless_pass_by_value)]

use serde_json::json;

use super::cells::{parse_jl_file, read_notebook, source_to_string, CellKind};

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
