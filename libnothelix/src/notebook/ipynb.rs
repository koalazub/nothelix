use std::fs;

use serde_json::Value;

use crate::error::{Error, Result};

pub(super) fn sibling_path(jl_path: &str, new_ext: &str) -> String {
    match jl_path.strip_suffix(".jl") {
        Some(stem) => format!("{stem}{new_ext}"),
        None => format!("{jl_path}{new_ext}"),
    }
}

pub(super) fn read_notebook(path: &str) -> Result<Value> {
    let content = fs::read_to_string(path).map_err(|source| Error::reading(path, source))?;
    serde_json::from_str(&content).map_err(|source| Error::Malformed {
        subject: "notebook",
        detail: format!("invalid JSON in {path}: {source}"),
    })
}

pub(super) fn cells_of<'a>(notebook: &'a Value, path: &str) -> Result<&'a [Value]> {
    notebook["cells"]
        .as_array()
        .map(Vec::as_slice)
        .ok_or_else(|| Error::Malformed {
            subject: "notebook",
            detail: format!("{path} has no cells array"),
        })
}

pub(super) fn source_to_string(source: &Value) -> String {
    match source {
        Value::Array(lines) => lines.iter().filter_map(Value::as_str).collect(),
        Value::String(text) => text.clone(),
        _ => String::new(),
    }
}

pub(super) fn source_lines(text: &str) -> Value {
    let last = text.lines().count().saturating_sub(1);
    text.lines()
        .enumerate()
        .map(|(i, line)| {
            let mut owned = line.to_string();
            if i < last {
                owned.push('\n');
            }
            Value::String(owned)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{sibling_path, source_lines, source_to_string};
    use serde_json::{Value, json};

    #[test]
    fn only_the_trailing_suffix_is_swapped() {
        assert_eq!(
            sibling_path("my.jl.backup.jl", ".ipynb"),
            "my.jl.backup.ipynb"
        );
        assert_eq!(sibling_path("proj.jl/nb.jl", ".md"), "proj.jl/nb.md");
    }

    #[test]
    fn a_path_without_the_suffix_gains_the_extension() {
        assert_eq!(sibling_path("notebook", ".ipynb"), "notebook.ipynb");
    }

    #[test]
    fn source_lines_round_trips_through_source_to_string() {
        for text in ["", "one", "one\ntwo", "trailing\n"] {
            assert_eq!(
                source_to_string(&source_lines(text)),
                text.trim_end_matches('\n')
            );
        }
    }

    #[test]
    fn an_array_source_joins_its_chunks() {
        assert_eq!(
            source_to_string(&json!(["line1\n", "line2"])),
            "line1\nline2"
        );
    }

    #[test]
    fn non_string_chunks_of_an_array_source_are_skipped() {
        assert_eq!(source_to_string(&json!(["a\n", 7, "b"])), "a\nb");
    }

    #[test]
    fn a_string_source_passes_through_unchanged() {
        assert_eq!(source_to_string(&json!("single string")), "single string");
    }

    #[test]
    fn a_source_that_is_neither_array_nor_string_reads_as_empty() {
        assert_eq!(source_to_string(&Value::Null), "");
        assert_eq!(source_to_string(&json!(7)), "");
    }
}
