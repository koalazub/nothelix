use serde_json::json;

use super::cells::parse_jl_file;
use super::ipynb::{cells_of, read_notebook, source_to_string};
use super::marker::CellKind;
use crate::error::{Error, Result, ffi};

const NO_CELLS: isize = 0;
const REQUIRED_FIELDS: [&str; 2] = ["cells", "nbformat"];

pub fn notebook_validate(path: String) -> String {
    match validate(&path) {
        Ok(()) => String::new(),
        Err(fault) => fault.to_string(),
    }
}

fn validate(path: &str) -> Result<()> {
    let notebook = read_notebook(path)?;
    for field in REQUIRED_FIELDS {
        if notebook.get(field).is_none() {
            return Err(Error::Malformed {
                subject: "notebook",
                detail: format!("missing '{field}' field"),
            });
        }
    }
    Ok(())
}

pub fn notebook_cell_count(path: String) -> isize {
    cell_count(&path).unwrap_or(NO_CELLS)
}

fn cell_count(path: &str) -> Result<isize> {
    let notebook = read_notebook(path)?;
    Ok(cells_of(&notebook, path)?.len() as isize)
}

pub fn notebook_get_cell_code(path: String, cell_index: isize) -> String {
    ffi(cell_code(&path, cell_index))
}

fn cell_code(path: &str, cell_index: isize) -> Result<String> {
    let notebook = read_notebook(path)?;
    let cells = cells_of(&notebook, path)?;
    let cell = usize::try_from(cell_index)
        .ok()
        .and_then(|at| cells.get(at))
        .ok_or_else(|| absent_cell(cell_index))?;
    Ok(source_to_string(&cell["source"]))
}

fn absent_cell(cell_index: isize) -> Error {
    Error::Malformed {
        subject: "notebook",
        detail: format!("Cell {cell_index} not found"),
    }
}

pub fn get_cell_at_line(path: String, line: isize) -> String {
    match cell_at_line(&path, line) {
        Ok(located) => located,
        Err(fault) => {
            json!({"cell_index": "", "source_path": "", "error": fault.to_string()}).to_string()
        }
    }
}

fn cell_at_line(path: &str, line: isize) -> Result<String> {
    let (cells, source_path) = parse_jl_file(path)?;
    let line = line as usize;
    let found = cells
        .iter()
        .enumerate()
        .find(|(at, cell)| {
            let next_start = cells.get(at + 1).map_or(usize::MAX, |next| next.start_line);
            line >= cell.start_line && line < next_start
        })
        .map_or(0, |(_, cell)| cell.index);
    Ok(json!({
        "cell_index": found.to_string(),
        "source_path": source_path,
        "error": ""
    })
    .to_string())
}

pub fn get_cell_code_from_jl(jl_path: String, cell_index: isize) -> String {
    match code_of_cell(&jl_path, cell_index) {
        Ok(code) => json!({"code": code, "error": ""}).to_string(),
        Err(fault) => json!({"code": "", "error": fault.to_string()}).to_string(),
    }
}

fn code_of_cell(jl_path: &str, cell_index: isize) -> Result<String> {
    let (cells, _) = parse_jl_file(jl_path)?;
    cells
        .iter()
        .find(|cell| cell.index == cell_index)
        .map(|cell| cell.code.clone())
        .ok_or_else(|| absent_cell(cell_index))
}

pub fn list_jl_code_cells(jl_path: String, limit: isize) -> String {
    match code_cell_indices(&jl_path, limit) {
        Ok(indices) => json!({"indices": indices, "error": ""}).to_string(),
        Err(fault) => json!({"indices": "", "error": fault.to_string()}).to_string(),
    }
}

fn code_cell_indices(jl_path: &str, limit: isize) -> Result<String> {
    let (cells, _) = parse_jl_file(jl_path)?;
    let cap = match usize::try_from(limit) {
        Ok(0) | Err(_) => usize::MAX,
        Ok(limit) => limit,
    };
    Ok(cells
        .iter()
        .filter(|cell| cell.kind == CellKind::Code)
        .take(cap)
        .map(|cell| cell.index.to_string())
        .collect::<Vec<_>>()
        .join(","))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::notebook::fixture;
    use serde_json::Value;

    #[test]
    fn validate_valid_notebook() {
        let result = notebook_validate(fixture::path("simple.ipynb"));
        assert_eq!(
            result, "",
            "Expected empty string for valid notebook, got: {result}"
        );
    }

    #[test]
    fn validate_nonexistent_file() {
        let result = notebook_validate("/nonexistent/file.ipynb".into());
        assert!(
            result.contains("cannot read") && result.contains("/nonexistent/file.ipynb"),
            "Expected read error naming the path, got: {result}"
        );
    }

    #[test]
    fn cell_count() {
        assert_eq!(notebook_cell_count(fixture::path("simple.ipynb")), 4);
    }

    #[test]
    fn cell_count_nonexistent() {
        assert_eq!(notebook_cell_count("/nonexistent.ipynb".into()), 0);
    }

    #[test]
    fn get_cell_code_first_cell() {
        let code = notebook_get_cell_code(fixture::path("simple.ipynb"), 0);
        assert_eq!(code, "using Plots");
    }

    #[test]
    fn get_cell_code_multiline() {
        let code = notebook_get_cell_code(fixture::path("simple.ipynb"), 1);
        assert_eq!(code, "x = 1:10\ny = x.^2");
    }

    #[test]
    fn get_cell_code_out_of_range_names_the_cell_instead_of_reading_as_empty() {
        let code = notebook_get_cell_code(fixture::path("simple.ipynb"), 99);
        assert_eq!(code, "ERROR: notebook: Cell 99 not found");
    }

    #[test]
    fn get_cell_code_from_an_unreadable_notebook_names_the_path() {
        let code = notebook_get_cell_code("/nonexistent/file.ipynb".into(), 0);
        assert!(
            code.starts_with("ERROR: cannot read /nonexistent/file.ipynb"),
            "{code}"
        );
    }

    #[test]
    fn get_cell_at_line_first_cell() {
        let result = get_cell_at_line(fixture::path("simple.jl"), 4);
        let parsed: Value = serde_json::from_str(&result).unwrap();
        assert_eq!(parsed["cell_index"].as_str().unwrap(), "0");
        assert_eq!(parsed["error"].as_str().unwrap(), "");
    }

    #[test]
    fn get_cell_at_line_second_cell() {
        let result = get_cell_at_line(fixture::path("simple.jl"), 11);
        let parsed: Value = serde_json::from_str(&result).unwrap();
        assert_eq!(parsed["cell_index"].as_str().unwrap(), "1");
    }

    #[test]
    fn get_cell_at_line_markdown() {
        let result = get_cell_at_line(fixture::path("simple.jl"), 30);
        let parsed: Value = serde_json::from_str(&result).unwrap();
        assert_eq!(parsed["cell_index"].as_str().unwrap(), "2");
    }

    #[test]
    fn get_cell_code_from_jl_valid() {
        let result = get_cell_code_from_jl(fixture::path("simple.jl"), 3);
        let parsed: Value = serde_json::from_str(&result).unwrap();
        assert_eq!(parsed["code"].as_str().unwrap(), "plot(x, y)");
        assert_eq!(parsed["error"].as_str().unwrap(), "");
    }

    #[test]
    fn get_cell_code_from_jl_missing() {
        let result = get_cell_code_from_jl(fixture::path("simple.jl"), 99);
        let parsed: Value = serde_json::from_str(&result).unwrap();
        assert!(parsed["error"].as_str().unwrap().contains("not found"));
    }

    #[test]
    fn list_jl_code_cells_skips_markdown() {
        let result = list_jl_code_cells(fixture::path("simple.jl"), 0);
        let parsed: Value = serde_json::from_str(&result).unwrap();
        assert_eq!(parsed["indices"].as_str().unwrap(), "0,1,3");
    }

    #[test]
    fn list_jl_code_cells_limited() {
        let result = list_jl_code_cells(fixture::path("simple.jl"), 2);
        let parsed: Value = serde_json::from_str(&result).unwrap();
        assert_eq!(parsed["indices"].as_str().unwrap(), "0,1");
    }
}
