// Steel's `register_fn` marshals values from the Steel VM and requires
// the registered fn's signature to take owned types (`String`), not
// borrows. The owned type is load-bearing for the FFI dispatcher.
#![allow(clippy::needless_pass_by_value)]

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

mod cells;
mod convert;
mod embed;
mod export;
mod ffi;
mod scan;

pub use convert::{convert_to_ipynb, notebook_convert_sync};
pub use export::{export_to_markdown, export_to_typst};
pub use ffi::{
    get_cell_at_line, get_cell_code_from_jl, list_jl_code_cells, notebook_cell_count,
    notebook_get_cell_code, notebook_validate,
};
pub use scan::scan_variable_definition;

// The remaining integration tests in this module address the public
// FFI surface end-to-end. The cells / parse internals they reach into
// for fixture parsing get re-imported under cfg(test) so they stay
// invisible to the rest of the crate.
#[cfg(test)]
use cells::{parse_jl_file, source_to_string, CellKind};
#[cfg(test)]
use serde_json::Value;
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
        use base64::Engine as _;
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
        std::fs::write(&img_path, fake_png).unwrap();

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
        use base64::Engine as _;
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
