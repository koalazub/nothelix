use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use serde_json::{Value, json};

use super::ipynb::source_to_string;
use super::{convert_to_ipynb, notebook_convert_sync};

fn assert_fixpoint(name: &str, nb: &Value) {
    let dir = tempfile::TempDir::new().unwrap();
    let ipynb = dir.path().join("nb.ipynb");
    let jl = dir.path().join("nb.jl");
    std::fs::write(&ipynb, serde_json::to_string_pretty(nb).unwrap()).unwrap();

    let jl_1 = notebook_convert_sync(ipynb.to_string_lossy().into());
    assert!(!jl_1.starts_with("ERROR"), "{name}: {jl_1}");
    std::fs::write(&jl, &jl_1).unwrap();
    let synced = convert_to_ipynb(jl.to_string_lossy().into());
    assert!(synced.starts_with("Synced to"), "{name}: {synced}");
    let ipynb_1: Value = serde_json::from_str(&std::fs::read_to_string(&ipynb).unwrap()).unwrap();

    let jl_2 = notebook_convert_sync(ipynb.to_string_lossy().into());
    assert_eq!(jl_2, jl_1, "{name}: second .jl must be byte-identical");
    std::fs::write(&jl, &jl_2).unwrap();
    let synced = convert_to_ipynb(jl.to_string_lossy().into());
    assert!(synced.starts_with("Synced to"), "{name}: {synced}");
    let ipynb_2: Value = serde_json::from_str(&std::fs::read_to_string(&ipynb).unwrap()).unwrap();
    assert_eq!(
        ipynb_2, ipynb_1,
        "{name}: second .ipynb must equal the first"
    );
}

fn synthesized_corpus() -> Vec<(String, Value)> {
    let meta = json!({
        "kernelspec": {"language": "julia", "name": "julia-1.10", "display_name": "Julia 1.10"}
    });
    let nb = |name: &str, cells: Value| -> (String, Value) {
        (
            name.to_string(),
            json!({
                "nbformat": 4,
                "nbformat_minor": 5,
                "metadata": meta,
                "cells": cells
            }),
        )
    };
    let png_b64 = BASE64.encode([0x89u8, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A]);

    vec![
        nb(
            "raw-cells",
            json!([
                {"cell_type": "code", "id": "c0", "execution_count": null, "metadata": {}, "outputs": [], "source": ["x = 1"]},
                {"cell_type": "raw", "id": "r1", "metadata": {"format": "text/latex"},
                 "source": ["\\begin{align}\n", "E = mc^2\n", "\\end{align}"]}
            ]),
        ),
        nb(
            "markdown-attachments",
            json!([
                {"cell_type": "markdown", "id": "md0", "metadata": {},
                 "source": ["A figure:\n", "\n", "![](attachment:fig.png)"],
                 "attachments": {"fig.png": {"image/png": png_b64}}}
            ]),
        ),
        nb(
            "alt-text-refs",
            json!([
                {"cell_type": "markdown", "id": "alt0", "metadata": {},
                 "source": ["Vanilla Jupyter shape:\n", "\n", "![image.png](attachment:image.png)"],
                 "attachments": {"image.png": {"image/png": png_b64}}}
            ]),
        ),
        nb(
            "mixed-attachments",
            json!([
                {"cell_type": "markdown", "id": "mx0", "metadata": {},
                 "source": ["prose\n", "\n", "![](attachment:good.png)\n", "![](attachment:bad.bin)"],
                 "attachments": {
                     "good.png": {"image/png": png_b64},
                     "bad.bin": {"application/octet-stream": "!!!not-base64!!!"}
                 }}
            ]),
        ),
        nb(
            "traversal-key",
            json!([
                {"cell_type": "markdown", "id": "tr0", "metadata": {},
                 "source": ["prose\n", "\n", "![](attachment:../../escape.png)\n", "![](attachment:..)"],
                 "attachments": {
                     "../../escape.png": {"image/png": png_b64},
                     "..": {"image/png": png_b64}
                 }}
            ]),
        ),
        nb(
            "missing-ids",
            json!([
                {"cell_type": "code", "execution_count": 1, "metadata": {},
                 "outputs": [{"output_type": "stream", "name": "stdout", "text": "hi\n"}],
                 "source": ["println(\"hi\")"]},
                {"cell_type": "markdown", "metadata": {}, "source": ["## heading"]},
                {"cell_type": "code", "execution_count": null, "metadata": {}, "outputs": [], "source": ["y = 2"]}
            ]),
        ),
        nb(
            "exotic-outputs",
            json!([
                {"cell_type": "code", "id": "xo0", "execution_count": 3, "metadata": {},
                 "outputs": [
                    {"output_type": "stream", "name": "stdout", "text": ["line 1\n", "line 2\n"]},
                    {"output_type": "display_data", "metadata": {},
                     "data": {"image/png": png_b64, "text/plain": ["a plot"]}},
                    {"output_type": "execute_result", "execution_count": 3, "metadata": {},
                     "data": {"text/plain": ["42"]}},
                    {"output_type": "error", "ename": "UndefVarError", "evalue": "z not defined",
                     "traceback": ["UndefVarError: z not defined", "Stacktrace: [1] top-level scope"]}
                 ],
                 "source": ["compute(z)"]}
            ]),
        ),
        nb(
            "empty-cells",
            json!([
                {"cell_type": "code", "id": "e0", "execution_count": null, "metadata": {}, "outputs": [], "source": []},
                {"cell_type": "markdown", "id": "e1", "metadata": {}, "source": []},
                {"cell_type": "raw", "id": "e2", "metadata": {}, "source": []}
            ]),
        ),
        nb(
            "duplicate-sources",
            json!([
                {"cell_type": "code", "id": "d0", "execution_count": 1, "metadata": {},
                 "outputs": [{"output_type": "stream", "name": "stdout", "text": "first\n"}],
                 "source": ["repeat()"]},
                {"cell_type": "code", "id": "d1", "execution_count": 2, "metadata": {},
                 "outputs": [{"output_type": "stream", "name": "stdout", "text": "second\n"}],
                 "source": ["repeat()"]}
            ]),
        ),
    ]
}

fn shipped_examples() -> Vec<(String, Value)> {
    let examples = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../examples");
    let Ok(entries) = std::fs::read_dir(&examples) else {
        return Vec::new();
    };
    entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "ipynb"))
        .filter_map(|path| {
            let text = std::fs::read_to_string(&path).ok()?;
            let nb = serde_json::from_str::<Value>(&text).ok()?;
            Some((path.display().to_string(), nb))
        })
        .collect()
}

#[test]
fn outputs_follow_their_cell_when_the_user_reorders_the_jl() {
    let dir = tempfile::TempDir::new().unwrap();
    let ipynb = dir.path().join("nb.ipynb");
    let jl = dir.path().join("nb.jl");
    let orig = json!({
        "nbformat": 4,
        "nbformat_minor": 5,
        "metadata": {},
        "cells": [
            {"cell_type": "code", "id": "id-a", "execution_count": 1, "metadata": {},
             "outputs": [{"output_type": "stream", "name": "stdout", "text": "from a\n"}],
             "source": ["a = 1"]},
            {"cell_type": "code", "id": "id-b", "execution_count": 2, "metadata": {},
             "outputs": [{"output_type": "stream", "name": "stdout", "text": "from b\n"}],
             "source": ["b = 2"]}
        ]
    });
    std::fs::write(&ipynb, serde_json::to_string_pretty(&orig).unwrap()).unwrap();

    let swapped = format!(
        "# ═══ Nothelix Notebook: {} ═══\n# Cells: 2\n\n@cell 0 :julia\nb = 2\n\n@cell 1 :julia\na = 1\n",
        ipynb.display()
    );
    std::fs::write(&jl, swapped).unwrap();

    let synced = convert_to_ipynb(jl.to_string_lossy().into());
    assert!(synced.starts_with("Synced to"), "got: {synced}");
    let nb: Value = serde_json::from_str(&std::fs::read_to_string(&ipynb).unwrap()).unwrap();
    let cells = nb["cells"].as_array().unwrap();

    assert_eq!(source_to_string(&cells[0]["source"]), "b = 2");
    assert_eq!(cells[0]["outputs"][0]["text"].as_str(), Some("from b\n"));
    assert_eq!(cells[0]["id"].as_str(), Some("id-b"));
    assert_eq!(cells[0]["execution_count"].as_i64(), Some(2));

    assert_eq!(source_to_string(&cells[1]["source"]), "a = 1");
    assert_eq!(cells[1]["outputs"][0]["text"].as_str(), Some("from a\n"));
    assert_eq!(cells[1]["id"].as_str(), Some("id-a"));
    assert_eq!(cells[1]["execution_count"].as_i64(), Some(1));

    assert_fixpoint("reordered", &nb);
}

#[test]
fn corpus_fixpoint_ipynb_jl_ipynb() {
    let mut corpus = synthesized_corpus();
    corpus.extend(shipped_examples());
    for (name, nb) in &corpus {
        assert_fixpoint(name, nb);
    }
}
