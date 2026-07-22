use std::collections::HashSet;
use std::fs;
use std::io::ErrorKind;

use serde_json::{Map, Value, json};

use super::cells::{JlCell, parse_jl_file};
use super::embed::{
    attachment_ref_name, embed_markdown_attachments, extract_markdown_attachments,
    is_attachment_ref_line, read_sidecar_image_output,
};
use super::ipynb::{cells_of, read_notebook, sibling_path, source_lines, source_to_string};
use super::marker::CellKind;
use crate::error::{Error, FileAction, Result};

const DEFAULT_LANGUAGE: &str = "julia";
const FIRST_MINOR_WITH_CELL_IDS: i64 = 5;

pub fn notebook_convert_sync(path: String) -> String {
    crate::error::ffi(ipynb_to_jl(&path))
}

pub fn convert_to_ipynb(jl_path: String) -> String {
    crate::error::ffi(jl_to_ipynb(&jl_path))
}

fn ipynb_to_jl(path: &str) -> Result<String> {
    let notebook = read_notebook(path)?;
    let cells = cells_of(&notebook, path)?;
    let language = notebook["metadata"]["kernelspec"]["language"]
        .as_str()
        .unwrap_or(DEFAULT_LANGUAGE);

    let mut out = format!(
        "# ═══ Nothelix Notebook: {path} ═══\n# Cells: {}\n\n",
        cells.len()
    );
    for (index, cell) in cells.iter().enumerate() {
        let cell_type = declared_cell_type(cell, index)?;
        let source = source_to_string(&cell["source"]);
        match cell_type {
            "markdown" => {
                let extraction =
                    extract_markdown_attachments(&source, cell.get("attachments"), path)?;
                out.push_str(&format!("@markdown {index}\n"));
                push_commented(&mut out, &extraction.body);
                for sidecar in &extraction.sidecar_paths {
                    out.push_str(&format!("# @image {sidecar}\n"));
                }
            }
            "raw" => {
                out.push_str(&format!("@raw {index}\n"));
                push_commented(&mut out, &source);
            }
            _ => {
                out.push_str(&format!("@cell {index} :{language}\n"));
                for line in body_lines(&source) {
                    out.push_str(line);
                    out.push('\n');
                }
            }
        }
        out.push('\n');
    }

    Ok(crate::math_format::format_math(out))
}

fn declared_cell_type(cell: &Value, index: usize) -> Result<&str> {
    cell["cell_type"].as_str().ok_or_else(|| Error::Malformed {
        subject: "notebook cell",
        detail: format!("cell {index} has no cell_type"),
    })
}

fn push_commented(out: &mut String, body: &str) {
    for line in body_lines(body) {
        out.push_str("# ");
        out.push_str(line);
        out.push('\n');
    }
}

fn body_lines(source: &str) -> Vec<&str> {
    let mut lines: Vec<&str> = source.lines().collect();
    while lines.last().is_some_and(|line| line.trim().is_empty()) {
        lines.pop();
    }
    lines
}

fn strip_attachment_refs(body: &str) -> String {
    body.lines()
        .filter(|line| !is_attachment_ref_line(line))
        .collect::<Vec<_>>()
        .join("\n")
        .trim()
        .to_string()
}

fn match_key(cell_type: &str, source: &str) -> String {
    if cell_type == "markdown" {
        strip_attachment_refs(source)
    } else {
        source.trim().to_string()
    }
}

fn deterministic_cell_id(cell_type: &str, source: &str, position: usize) -> String {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    cell_type.hash(&mut hasher);
    source.trim().hash(&mut hasher);
    position.hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}

fn jl_to_ipynb_source(cell: &JlCell) -> String {
    if cell.kind.is_prose() {
        uncommented(&cell.code)
    } else {
        cell.code.clone()
    }
}

fn uncommented(code: &str) -> String {
    code.lines()
        .map(|line| line.strip_prefix("# ").unwrap_or(line))
        .collect::<Vec<_>>()
        .join("\n")
}

struct OriginalKey {
    cell_type: String,
    source_key: String,
}

struct OriginalCells {
    cells: Vec<Value>,
    keys: Vec<Option<OriginalKey>>,
    claimed: Vec<bool>,
}

impl OriginalCells {
    fn of(notebook: &Value, source_path: &str) -> Result<Self> {
        let cells = cells_of(notebook, source_path)?.to_vec();
        let keys = cells
            .iter()
            .map(|cell| {
                let cell_type = cell.get("cell_type").and_then(Value::as_str)?;
                Some(OriginalKey {
                    cell_type: cell_type.to_string(),
                    source_key: match_key(cell_type, &source_to_string(&cell["source"])),
                })
            })
            .collect();
        let claimed = vec![false; cells.len()];
        Ok(Self {
            cells,
            keys,
            claimed,
        })
    }

    fn describes(&self, at: usize, cell_type: &str, key: &str) -> bool {
        !self.claimed[at]
            && self.keys[at]
                .as_ref()
                .is_some_and(|k| k.cell_type == cell_type && k.source_key == key)
    }

    fn claim(&mut self, preferred: isize, cell_type: &str, key: &str) -> Option<Value> {
        let at = usize::try_from(preferred)
            .ok()
            .filter(|&at| at < self.cells.len() && self.describes(at, cell_type, key))
            .or_else(|| (0..self.cells.len()).find(|&at| self.describes(at, cell_type, key)))?;
        self.claimed[at] = true;
        Some(self.cells[at].clone())
    }
}

fn jl_to_ipynb(jl_path: &str) -> Result<String> {
    let (cells, source_path) = parse_jl_file(jl_path)?;
    let mut notebook = original_notebook(&source_path)?;
    let mut originals = OriginalCells::of(&notebook, &source_path)?;

    let mut converted: Vec<Value> = Vec::with_capacity(cells.len());
    for (position, cell) in cells.iter().enumerate() {
        let source_text = jl_to_ipynb_source(cell);
        let cell_type = cell.kind.ipynb_type();
        let original = originals.claim(cell.index, cell_type, &match_key(cell_type, &source_text));

        let mut converted_cell = match cell.kind {
            CellKind::Markdown | CellKind::Typst => {
                markdown_cell(original, cell, &source_text, jl_path)?
            }
            CellKind::Raw => raw_cell(original, &source_text),
            CellKind::Code => code_cell(original, cell, &source_text, jl_path)?,
        };
        stamp_id(&mut converted_cell, cell_type, &source_text, position);
        converted.push(converted_cell);
    }

    notebook["cells"] = Value::Array(converted);
    if notebook["nbformat_minor"]
        .as_i64()
        .is_none_or(|minor| minor < FIRST_MINOR_WITH_CELL_IDS)
    {
        notebook["nbformat_minor"] = json!(FIRST_MINOR_WITH_CELL_IDS);
    }

    let out_path = if source_path.ends_with(".ipynb") {
        source_path
    } else {
        sibling_path(jl_path, ".ipynb")
    };
    let serialized = serde_json::to_string_pretty(&notebook).map_err(|source| Error::Json {
        subject: "notebook",
        source,
    })?;
    fs::write(&out_path, serialized).map_err(|source| Error::writing(&out_path, source))?;
    Ok(format!("Synced to {out_path}"))
}

fn original_notebook(source_path: &str) -> Result<Value> {
    match read_notebook(source_path) {
        Err(error) if never_written(&error) => Ok(json!({
            "nbformat": 4,
            "nbformat_minor": FIRST_MINOR_WITH_CELL_IDS,
            "metadata": {},
            "cells": []
        })),
        outcome => outcome,
    }
}

fn never_written(error: &Error) -> bool {
    matches!(
        error,
        Error::File { action: FileAction::Read, source, .. } if source.kind() == ErrorKind::NotFound
    )
}

fn markdown_cell(
    original: Option<Value>,
    cell: &JlCell,
    source_text: &str,
    jl_path: &str,
) -> Result<Value> {
    let mut converted =
        original.unwrap_or_else(|| json!({"cell_type": "markdown", "metadata": {}, "source": []}));
    converted["cell_type"] = json!("markdown");

    let attached = embed_markdown_attachments(source_text, &cell.images, jl_path)?;
    converted["source"] = source_lines(&attached.body);

    let referenced: HashSet<&str> = attached
        .body
        .lines()
        .filter_map(attachment_ref_name)
        .collect();
    let mut merged = converted
        .get("attachments")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    merged.retain(|name, _| referenced.contains(name.as_str()));
    if let Some(embedded) = attached.entries.as_object() {
        merged.extend(embedded.iter().map(|(k, v)| (k.clone(), v.clone())));
    }
    set_or_remove(&mut converted, "attachments", merged);
    drop_execution_fields(&mut converted);
    Ok(converted)
}

fn raw_cell(original: Option<Value>, source_text: &str) -> Value {
    let mut converted =
        original.unwrap_or_else(|| json!({"cell_type": "raw", "metadata": {}, "source": []}));
    converted["cell_type"] = json!("raw");
    converted["source"] = source_lines(source_text);
    drop_execution_fields(&mut converted);
    converted
}

fn code_cell(
    original: Option<Value>,
    cell: &JlCell,
    source_text: &str,
    jl_path: &str,
) -> Result<Value> {
    let mut converted = original.unwrap_or_else(|| {
        json!({
            "cell_type": "code",
            "execution_count": null,
            "metadata": {},
            "outputs": [],
            "source": []
        })
    });
    converted["cell_type"] = json!("code");
    converted["source"] = source_lines(source_text);
    if let Some(fields) = converted.as_object_mut() {
        fields.entry("execution_count").or_insert(json!(null));
        fields.entry("outputs").or_insert(json!([]));
    }
    if let Some(image_output) = read_sidecar_image_output(jl_path, cell.index)? {
        converted["outputs"] = json!([image_output]);
    }
    Ok(converted)
}

fn set_or_remove(cell: &mut Value, field: &str, entries: Map<String, Value>) {
    if entries.is_empty() {
        if let Some(fields) = cell.as_object_mut() {
            fields.remove(field);
        }
    } else {
        cell[field] = Value::Object(entries);
    }
}

fn drop_execution_fields(cell: &mut Value) {
    if let Some(fields) = cell.as_object_mut() {
        fields.remove("execution_count");
        fields.remove("outputs");
    }
}

fn stamp_id(cell: &mut Value, cell_type: &str, source_text: &str, position: usize) {
    if let Some(fields) = cell.as_object_mut()
        && !fields.get("id").is_some_and(Value::is_string)
    {
        fields.insert(
            "id".to_string(),
            json!(deterministic_cell_id(cell_type, source_text, position)),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::notebook::fixture;
    use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
    use std::path::Path;

    const PNG: [u8; 8] = [0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A];

    fn notebook_of(cells: &Value) -> Value {
        json!({"nbformat": 4, "nbformat_minor": 5, "metadata": {}, "cells": cells})
    }

    fn write_json(path: &Path, value: &Value) {
        std::fs::write(path, serde_json::to_string_pretty(value).unwrap()).unwrap();
    }

    fn read_json(path: &Path) -> Value {
        serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
    }

    fn write_jl(jl: &Path, ipynb: &Path, cell_count: usize, body: &str) {
        let header = format!(
            "# ═══ Nothelix Notebook: {} ═══\n# Cells: {cell_count}\n\n{body}",
            ipynb.display()
        );
        std::fs::write(jl, header).unwrap();
    }

    fn synced(jl: &Path, ipynb: &Path) -> Value {
        let result = convert_to_ipynb(jl.to_string_lossy().into());
        assert!(result.starts_with("Synced to"), "got: {result}");
        read_json(ipynb)
    }

    fn to_jl(ipynb: &Path) -> String {
        let jl = notebook_convert_sync(ipynb.to_string_lossy().into());
        assert!(!jl.starts_with("ERROR"), "{jl}");
        jl
    }

    #[test]
    fn convert_sync_produces_cell_markers() {
        let result = notebook_convert_sync(fixture::path("simple.ipynb"));
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
        let result = notebook_convert_sync(fixture::path("simple.ipynb"));
        assert!(result.starts_with("# ═══ Nothelix Notebook:"));
        assert!(!result.contains("NothelixMacros"));
        assert!(result.contains("# Cells: 4"));
    }

    #[test]
    fn convert_sync_markdown_commented() {
        let result = notebook_convert_sync(fixture::path("simple.ipynb"));
        assert!(result.contains("# # Results"));
        assert!(result.contains("# This shows the quadratic function."));
    }

    #[test]
    fn convert_to_ipynb_roundtrip() {
        let dir = tempfile::TempDir::new().unwrap();
        let ipynb = dir.path().join("nb.ipynb");
        let jl = dir.path().join("nb.jl");
        std::fs::copy(fixture::path("simple.ipynb"), &ipynb).unwrap();

        let fixture_jl = std::fs::read_to_string(fixture::path("simple.jl")).unwrap();
        let repointed: String = fixture_jl
            .lines()
            .map(|line| {
                if line.starts_with("# ═══ Nothelix Notebook: ") {
                    format!("# ═══ Nothelix Notebook: {} ═══", ipynb.to_string_lossy())
                } else {
                    line.to_string()
                }
            })
            .collect::<Vec<_>>()
            .join("\n");
        std::fs::write(&jl, &repointed).unwrap();

        let nb = synced(&jl, &ipynb);
        assert_eq!(nb["cells"].as_array().unwrap().len(), 4);
        assert_eq!(nb["cells"][0]["cell_type"], "code");
        assert_eq!(nb["cells"][2]["cell_type"], "markdown");
    }

    #[test]
    fn stale_outputs_are_dropped_when_the_cell_code_was_edited() {
        let dir = tempfile::TempDir::new().unwrap();
        let ipynb = dir.path().join("nb.ipynb");
        let jl = dir.path().join("nb.jl");
        write_json(
            &ipynb,
            &notebook_of(&json!([{
                "cell_type": "code",
                "execution_count": 7,
                "metadata": {},
                "outputs": [{"output_type": "stream", "name": "stdout", "text": "stale output\n"}],
                "source": ["old_code = 1\n"]
            }])),
        );
        write_jl(&jl, &ipynb, 1, "@cell 0 :julia\nnew_code = 2\n");

        let nb = synced(&jl, &ipynb);
        let cell = &nb["cells"][0];
        assert_eq!(cell["source"][0].as_str().unwrap(), "new_code = 2");
        assert!(
            cell["outputs"].as_array().unwrap().is_empty(),
            "stale outputs should be dropped, got: {cell}"
        );
        assert!(
            cell["execution_count"].is_null(),
            "stale execution_count should be cleared, got: {cell}"
        );
    }

    #[test]
    fn outputs_and_metadata_survive_when_the_cell_code_is_unchanged() {
        let dir = tempfile::TempDir::new().unwrap();
        let ipynb = dir.path().join("nb.ipynb");
        let jl = dir.path().join("nb.jl");
        write_json(
            &ipynb,
            &notebook_of(&json!([{
                "cell_type": "code",
                "execution_count": 7,
                "metadata": {"tags": ["important"]},
                "outputs": [{"output_type": "stream", "name": "stdout", "text": "hi\n"}],
                "source": ["x = 1\n"]
            }])),
        );
        write_jl(&jl, &ipynb, 1, "@cell 0 :julia\nx = 1\n");

        let nb = synced(&jl, &ipynb);
        let cell = &nb["cells"][0];
        assert_eq!(
            cell["execution_count"].as_i64(),
            Some(7),
            "execution_count should survive: {cell}"
        );
        assert_eq!(
            cell["metadata"]["tags"][0].as_str(),
            Some("important"),
            "metadata should survive: {cell}"
        );
        assert_eq!(
            cell["outputs"].as_array().unwrap().len(),
            1,
            "outputs should survive: {cell}"
        );
    }

    #[test]
    fn a_code_cell_turned_into_markdown_sheds_its_execution_fields() {
        let dir = tempfile::TempDir::new().unwrap();
        let ipynb = dir.path().join("nb.ipynb");
        let jl = dir.path().join("nb.jl");
        write_json(
            &ipynb,
            &notebook_of(&json!([{
                "cell_type": "code",
                "execution_count": 3,
                "metadata": {},
                "outputs": [{"output_type": "stream", "name": "stdout", "text": "old\n"}],
                "source": ["println(\"old\")\n"]
            }])),
        );
        write_jl(&jl, &ipynb, 1, "@markdown 0\n# Now a heading\n");

        let nb = synced(&jl, &ipynb);
        let cell = &nb["cells"][0];
        assert_eq!(cell["cell_type"].as_str(), Some("markdown"));
        assert!(
            cell.get("outputs").is_none(),
            "markdown cell should not have outputs: {cell}"
        );
        assert!(
            cell.get("execution_count").is_none(),
            "markdown cell should not have execution_count: {cell}"
        );
    }

    #[test]
    fn a_sidecar_plot_becomes_a_display_data_output() {
        let dir = tempfile::TempDir::new().unwrap();
        let ipynb = dir.path().join("nb.ipynb");
        let jl = dir.path().join("nb.jl");
        write_json(
            &ipynb,
            &notebook_of(&json!([{
                "cell_type": "code",
                "execution_count": null,
                "metadata": {},
                "outputs": [],
                "source": ["plot(x, y)\n"]
            }])),
        );
        let images = dir.path().join(".nothelix").join("images");
        std::fs::create_dir_all(&images).unwrap();
        std::fs::write(images.join("cell-5.png"), PNG).unwrap();
        write_jl(&jl, &ipynb, 1, "@cell 5 :julia\nplot(x, y)\n");

        let nb = synced(&jl, &ipynb);
        let outputs = nb["cells"][0]["outputs"].as_array().unwrap();
        assert_eq!(
            outputs.len(),
            1,
            "should attach exactly one display_data output, got: {outputs:#?}"
        );
        assert_eq!(outputs[0]["output_type"].as_str(), Some("display_data"));
        let encoded = outputs[0]["data"]["image/png"].as_str().unwrap();
        assert!(
            !encoded.is_empty(),
            "PNG data should be base64-encoded, got: {encoded:?}"
        );
        assert_eq!(BASE64.decode(encoded).unwrap(), PNG);
    }

    #[test]
    fn a_markdown_image_marker_becomes_a_base64_attachment() {
        let dir = tempfile::TempDir::new().unwrap();
        let ipynb = dir.path().join("nb.ipynb");
        let jl = dir.path().join("nb.jl");
        std::fs::write(dir.path().join("diagram.png"), PNG).unwrap();
        write_json(
            &ipynb,
            &notebook_of(&json!([{"cell_type": "markdown", "metadata": {}, "source": []}])),
        );
        write_jl(
            &jl,
            &ipynb,
            1,
            "@markdown 0\n# See the figure below.\n# @image diagram.png\n",
        );

        let nb = synced(&jl, &ipynb);
        let cell = &nb["cells"][0];
        assert_eq!(cell["cell_type"].as_str(), Some("markdown"));
        let attachment = cell["attachments"]["diagram.png"]["image/png"]
            .as_str()
            .unwrap_or_else(|| {
                panic!("expected attachments.diagram.png.image/png, got:\n{cell:#?}")
            });
        assert_eq!(BASE64.decode(attachment).unwrap(), PNG);
        assert!(
            source_to_string(&cell["source"]).contains("![](attachment:diagram.png)"),
            "expected markdown body to reference the attachment, got:\n{cell:#?}"
        );
    }

    #[test]
    fn an_image_marker_pointing_at_nothing_is_skipped_not_fatal() {
        let dir = tempfile::TempDir::new().unwrap();
        let ipynb = dir.path().join("nb.ipynb");
        let jl = dir.path().join("nb.jl");
        write_json(
            &ipynb,
            &notebook_of(&json!([{"cell_type": "markdown", "metadata": {}, "source": []}])),
        );
        write_jl(
            &jl,
            &ipynb,
            1,
            "@markdown 0\n# Some prose.\n# @image vanished.png\n",
        );

        let nb = synced(&jl, &ipynb);
        assert!(
            nb["cells"][0].get("attachments").is_none(),
            "no attachment should be written for missing file, got: {}",
            nb["cells"][0]
        );
    }

    #[test]
    fn the_output_path_swaps_only_the_trailing_suffix() {
        let dir = tempfile::TempDir::new().unwrap();
        let jl = dir.path().join("my.jl.backup.jl");
        let src = "# ═══ Nothelix Notebook: not-a-notebook.txt ═══\n# Cells: 1\n\n@cell 0 :julia\nx = 1\n";
        std::fs::write(&jl, src).unwrap();

        let result = convert_to_ipynb(jl.to_string_lossy().into());
        let expected = dir.path().join("my.jl.backup.ipynb");
        assert_eq!(
            result,
            format!("Synced to {}", expected.display()),
            "suffix-only swap expected"
        );
        assert!(expected.exists());
    }

    #[test]
    fn fresh_cells_are_stamped_with_deterministic_ids() {
        let ids_for = |dir: &Path| -> Vec<String> {
            let ipynb = dir.join("missing-orig.ipynb");
            let jl = dir.join("nb.jl");
            write_jl(
                &jl,
                &ipynb,
                2,
                "@cell 0 :julia\nx = 1\n\n@markdown 1\n# hello\n",
            );
            synced(&jl, &ipynb)["cells"]
                .as_array()
                .unwrap()
                .iter()
                .map(|cell| {
                    cell["id"]
                        .as_str()
                        .expect("fresh cell must carry an id")
                        .to_string()
                })
                .collect()
        };

        let a = tempfile::TempDir::new().unwrap();
        let b = tempfile::TempDir::new().unwrap();
        let ids_a = ids_for(a.path());
        let ids_b = ids_for(b.path());
        assert_eq!(ids_a, ids_b, "ids must be deterministic across conversions");
        assert_ne!(ids_a[0], ids_a[1], "distinct cells get distinct ids");
        for id in &ids_a {
            assert!(
                (1..=64).contains(&id.len())
                    && id
                        .chars()
                        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_'),
                "id violates nbformat grammar: {id}"
            );
        }
    }

    #[test]
    fn an_original_cell_id_is_preserved() {
        let dir = tempfile::TempDir::new().unwrap();
        let ipynb = dir.path().join("nb.ipynb");
        let jl = dir.path().join("nb.jl");
        write_json(
            &ipynb,
            &notebook_of(&json!([{
                "cell_type": "code",
                "id": "keep-me_1",
                "execution_count": null,
                "metadata": {},
                "outputs": [],
                "source": ["x = 1"]
            }])),
        );
        write_jl(&jl, &ipynb, 1, "@cell 0 :julia\nx = 1\n");

        assert_eq!(
            synced(&jl, &ipynb)["cells"][0]["id"].as_str(),
            Some("keep-me_1")
        );
    }

    #[test]
    fn raw_cells_round_trip_without_becoming_code() {
        let dir = tempfile::TempDir::new().unwrap();
        let ipynb = dir.path().join("nb.ipynb");
        let jl = dir.path().join("nb.jl");
        let raw_text = "\\begin{align}\nE = mc^2\n\\end{align}";
        write_json(
            &ipynb,
            &json!({
                "nbformat": 4,
                "nbformat_minor": 5,
                "metadata": {"kernelspec": {"language": "julia", "name": "julia-1.10", "display_name": "Julia"}},
                "cells": [
                    {"cell_type": "code", "id": "c0", "execution_count": null, "metadata": {}, "outputs": [], "source": ["x = 1"]},
                    {"cell_type": "raw", "id": "r1", "metadata": {"format": "text/latex"}, "source": [raw_text]}
                ]
            }),
        );

        let jl_content = to_jl(&ipynb);
        assert!(
            jl_content.contains("@raw 1\n# \\begin{align}"),
            "raw marker + commented body expected:\n{jl_content}"
        );
        std::fs::write(&jl, &jl_content).unwrap();

        let nb = synced(&jl, &ipynb);
        let cell = &nb["cells"][1];
        assert_eq!(cell["cell_type"].as_str(), Some("raw"));
        assert_eq!(source_to_string(&cell["source"]), raw_text);
        assert_eq!(
            cell["id"].as_str(),
            Some("r1"),
            "matched raw cell keeps its id"
        );
        assert_eq!(cell["metadata"]["format"].as_str(), Some("text/latex"));
        assert!(
            cell.get("outputs").is_none(),
            "raw cells carry no outputs: {cell}"
        );
        assert!(
            cell.get("execution_count").is_none(),
            "raw cells carry no execution_count: {cell}"
        );
    }

    #[test]
    fn attachments_extract_to_the_sidecar_and_re_embed() {
        let dir = tempfile::TempDir::new().unwrap();
        let ipynb = dir.path().join("nb.ipynb");
        let jl = dir.path().join("nb.jl");
        let encoded = BASE64.encode(PNG);
        write_json(
            &ipynb,
            &notebook_of(&json!([{
                "cell_type": "markdown",
                "id": "md0",
                "metadata": {},
                "source": ["Look at this:\n", "\n", "![](attachment:fig.png)"],
                "attachments": {"fig.png": {"image/png": encoded}}
            }])),
        );

        let jl_content = to_jl(&ipynb);
        assert!(
            jl_content.contains("# @image .nothelix/images/fig.png"),
            "marker expected:\n{jl_content}"
        );
        assert!(
            !jl_content.contains("![](attachment:"),
            "ref line must be extracted:\n{jl_content}"
        );
        assert_eq!(
            std::fs::read(dir.path().join(".nothelix/images/fig.png")).unwrap(),
            PNG,
            "extracted bytes must match"
        );

        std::fs::write(&jl, &jl_content).unwrap();
        let nb = synced(&jl, &ipynb);
        let cell = &nb["cells"][0];
        assert_eq!(
            cell["attachments"]["fig.png"]["image/png"].as_str(),
            Some(encoded.as_str())
        );
        assert_eq!(
            cell["id"].as_str(),
            Some("md0"),
            "attachment cell still matches its original"
        );
        let body = source_to_string(&cell["source"]);
        assert!(
            body.contains("![](attachment:fig.png)"),
            "re-embedded body references the attachment:\n{body}"
        );
    }

    #[test]
    fn a_traversal_attachment_key_stays_inside_the_images_dir() {
        let dir = tempfile::TempDir::new().unwrap();
        let nb_dir = dir.path().join("nbdir");
        std::fs::create_dir(&nb_dir).unwrap();
        let ipynb = nb_dir.join("nb.ipynb");
        let jl = nb_dir.join("nb.jl");
        let encoded = BASE64.encode(PNG);
        write_json(
            &ipynb,
            &notebook_of(&json!([{
                "cell_type": "markdown",
                "id": "md0",
                "metadata": {},
                "source": [
                    "prose\n", "\n",
                    "![](attachment:../../escape.png)\n",
                    "![](attachment:..\\..\\evil.png)\n",
                    "![](attachment:..)"
                ],
                "attachments": {
                    "../../escape.png": {"image/png": encoded},
                    "..\\..\\evil.png": {"image/png": encoded},
                    "..": {"image/png": encoded}
                }
            }])),
        );

        let jl_content = to_jl(&ipynb);
        assert!(
            !dir.path().join("escape.png").exists(),
            "traversal escaped the sandbox"
        );
        assert!(
            !dir.path().join("evil.png").exists(),
            "backslash traversal escaped the sandbox"
        );
        assert!(!nb_dir.join("escape.png").exists());
        assert_eq!(
            std::fs::read(nb_dir.join(".nothelix/images/escape.png")).unwrap(),
            PNG
        );
        assert_eq!(
            std::fs::read(nb_dir.join(".nothelix/images/evil.png")).unwrap(),
            PNG
        );
        assert!(
            jl_content.contains("# @image .nothelix/images/escape.png"),
            "{jl_content}"
        );
        assert!(
            jl_content.contains("# @image .nothelix/images/evil.png"),
            "{jl_content}"
        );
        assert!(
            jl_content.contains("# ![](attachment:..)"),
            "unextractable ref must stay:\n{jl_content}"
        );

        std::fs::write(&jl, &jl_content).unwrap();
        let nb = synced(&jl, &ipynb);
        let cell = &nb["cells"][0];
        assert_eq!(
            cell["attachments"]["escape.png"]["image/png"].as_str(),
            Some(encoded.as_str())
        );
        assert_eq!(
            cell["attachments"]["evil.png"]["image/png"].as_str(),
            Some(encoded.as_str())
        );
        assert_eq!(
            cell["attachments"][".."]["image/png"].as_str(),
            Some(encoded.as_str()),
            "unextractable entry must be carried through: {cell}"
        );
        assert!(
            cell["attachments"]
                .as_object()
                .unwrap()
                .keys()
                .all(|key| key != "../../escape.png"),
            "hostile key must not survive re-embedding: {cell}"
        );
    }

    #[test]
    fn alt_texted_attachment_refs_round_trip_without_duplication() {
        let dir = tempfile::TempDir::new().unwrap();
        let ipynb = dir.path().join("nb.ipynb");
        let jl = dir.path().join("nb.jl");
        let encoded = BASE64.encode(PNG);
        write_json(
            &ipynb,
            &notebook_of(&json!([{
                "cell_type": "markdown",
                "id": "md0",
                "metadata": {},
                "source": ["Some prose.\n", "\n", "![image.png](attachment:image.png)"],
                "attachments": {"image.png": {"image/png": encoded}}
            }])),
        );

        let jl_content = to_jl(&ipynb);
        assert!(
            !jl_content.contains("](attachment:"),
            "alt-texted ref must be extracted:\n{jl_content}"
        );
        assert!(
            jl_content.contains("# @image .nothelix/images/image.png"),
            "{jl_content}"
        );

        std::fs::write(&jl, &jl_content).unwrap();
        let nb = synced(&jl, &ipynb);
        let cell = &nb["cells"][0];
        assert_eq!(
            cell["attachments"]["image.png"]["image/png"].as_str(),
            Some(encoded.as_str())
        );
        assert_eq!(
            cell["id"].as_str(),
            Some("md0"),
            "alt-texted cell still matches its original"
        );
        let body = source_to_string(&cell["source"]);
        let ref_lines = body
            .lines()
            .filter(|line| line.contains("](attachment:image.png)"))
            .count();
        assert_eq!(
            ref_lines, 1,
            "exactly one ref after round-trip, no duplication:\n{body}"
        );
    }

    #[test]
    fn an_undecodable_attachment_survives_beside_a_decodable_one() {
        let dir = tempfile::TempDir::new().unwrap();
        let ipynb = dir.path().join("nb.ipynb");
        let jl = dir.path().join("nb.jl");
        let encoded = BASE64.encode(PNG);
        write_json(
            &ipynb,
            &notebook_of(&json!([{
                "cell_type": "markdown",
                "id": "md0",
                "metadata": {},
                "source": [
                    "prose\n", "\n",
                    "![](attachment:good.png)\n",
                    "![](attachment:bad.bin)"
                ],
                "attachments": {
                    "good.png": {"image/png": encoded},
                    "bad.bin": {"application/octet-stream": "!!!not-base64!!!"}
                }
            }])),
        );

        let jl_content = to_jl(&ipynb);
        assert!(
            jl_content.contains("# @image .nothelix/images/good.png"),
            "{jl_content}"
        );
        assert!(
            jl_content.contains("# ![](attachment:bad.bin)"),
            "undecodable ref must stay in the body:\n{jl_content}"
        );
        assert!(
            !jl_content.contains("![](attachment:good.png)"),
            "decodable ref must be extracted:\n{jl_content}"
        );

        std::fs::write(&jl, &jl_content).unwrap();
        let nb = synced(&jl, &ipynb);
        let cell = &nb["cells"][0];
        assert_eq!(
            cell["id"].as_str(),
            Some("md0"),
            "partially-extracted cell still matches its original"
        );
        assert_eq!(
            cell["attachments"]["good.png"]["image/png"].as_str(),
            Some(encoded.as_str())
        );
        assert_eq!(
            cell["attachments"]["bad.bin"]["application/octet-stream"].as_str(),
            Some("!!!not-base64!!!"),
            "undecodable entry must be carried through verbatim: {cell}"
        );
        let body = source_to_string(&cell["source"]);
        assert!(
            body.contains("![](attachment:bad.bin)"),
            "undecodable ref survives:\n{body}"
        );
        assert!(
            body.contains("![](attachment:good.png)"),
            "re-embedded ref present:\n{body}"
        );
    }

    #[test]
    fn colliding_attachment_filenames_resolve_by_content() {
        let dir = tempfile::TempDir::new().unwrap();
        let ipynb = dir.path().join("nb.ipynb");
        let bytes_a: [u8; 4] = [1, 2, 3, 4];
        let bytes_b: [u8; 4] = [9, 8, 7, 6];
        write_json(
            &ipynb,
            &notebook_of(&json!([
                {"cell_type": "markdown", "metadata": {}, "source": ["first"],
                 "attachments": {"fig.png": {"image/png": BASE64.encode(bytes_a)}}},
                {"cell_type": "markdown", "metadata": {}, "source": ["second"],
                 "attachments": {"fig.png": {"image/png": BASE64.encode(bytes_b)}}}
            ])),
        );

        let first = to_jl(&ipynb);
        assert_eq!(first, to_jl(&ipynb), "extraction must be reproducible");
        assert!(
            first.contains("# @image .nothelix/images/fig.png"),
            "{first}"
        );
        let suffixed = first
            .lines()
            .find(|line| line.starts_with("# @image .nothelix/images/fig-"))
            .expect("collision must get a content-suffixed name");
        let relative = suffixed.strip_prefix("# @image ").unwrap();
        assert!(
            relative.ends_with(".png"),
            "extension preserved: {relative}"
        );

        assert_eq!(
            std::fs::read(dir.path().join(".nothelix/images/fig.png")).unwrap(),
            bytes_a
        );
        assert_eq!(std::fs::read(dir.path().join(relative)).unwrap(), bytes_b);
    }
}
