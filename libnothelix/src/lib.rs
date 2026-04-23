//! libnothelix — Steel FFI dylib for the Nothelix Jupyter notebook plugin.
//!
//! This file registers FFI functions with the Steel VM and holds small
//! utility functions that don't warrant their own module.

mod chart;
pub mod error_format;
mod graphics;
mod json_utils;
mod kernel;
mod kitty_placeholder;
mod lsp;
mod math_format;
mod notebook;
mod typst_export;
mod unicode;

use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use steel::steel_vm::ffi::{FFIModule, RegisterFFIFn};

steel::declare_module!(build_module);

/// Compile-time BUILD_ID for this libnothelix. Used by
/// `nothelix doctor` to verify the installed dylib matches the
/// installed fork binary.
///
/// Format:
///   - `ci-<yyyymmdd>-<short-git-sha>` for CI builds
///   - `dev-<short-git-sha>[-dirty]`   for local developer builds
pub fn build_id() -> &'static str {
    env!("NOTHELIX_BUILD_ID")
}

fn build_module() -> FFIModule {
    let mut m = FFIModule::new("nothelix");

    // ── Kernel management ─────────────────────────────────────────────────────
    m.register_fn("kernel-start-macro", kernel::kernel_start_macro);
    m.register_fn("kernel-stop", kernel::kernel_stop);
    m.register_fn(
        "kernel-stop-all-processes",
        kernel::kernel_stop_all_processes,
    );

    // ── Notebook operations ───────────────────────────────────────────────────
    m.register_fn("notebook-validate", notebook::notebook_validate);
    m.register_fn("notebook-convert-sync!", notebook::notebook_convert_sync);
    m.register_fn("notebook-cell-count", notebook::notebook_cell_count);
    m.register_fn("notebook-get-cell-code", notebook::notebook_get_cell_code);
    m.register_fn("get-cell-at-line", notebook::get_cell_at_line);
    m.register_fn("get-cell-code-from-jl", notebook::get_cell_code_from_jl);
    m.register_fn("list-jl-code-cells", notebook::list_jl_code_cells);
    m.register_fn("scan-variable-definition", notebook::scan_variable_definition);
    m.register_fn("convert-to-ipynb!", notebook::convert_to_ipynb);
    m.register_fn("export-to-markdown!", notebook::export_to_markdown);
    m.register_fn("export-to-typst!", notebook::export_to_typst);
    m.register_fn("format-math", math_format::format_math);

    // ── Execution ─────────────────────────────────────────────────────────────
    m.register_fn(
        "kernel-execute-cell-start",
        kernel::kernel_execute_cell_start,
    );
    m.register_fn("kernel-poll-result", kernel::kernel_poll_result);
    m.register_fn("kernel-interrupt", kernel::kernel_interrupt);

    // ── JSON utilities ────────────────────────────────────────────────────────
    m.register_fn("json-get", json_utils::json_get);
    m.register_fn("json-get-bool", json_utils::json_get_bool);
    m.register_fn("json-get-many", json_utils::json_get_many);
    m.register_fn("json-get-first-image", json_utils::json_get_first_image);
    m.register_fn(
        "json-get-first-image-with-dir",
        json_utils::json_get_first_image_with_dir,
    );
    m.register_fn(
        "json-get-first-image-bytes",
        json_utils::json_get_first_image_bytes,
    );
    m.register_fn("json-get-plot-data", json_utils::json_get_plot_data);

    // ── Graphics ──────────────────────────────────────────────────────────────
    m.register_fn("viuer-protocol", graphics::viuer_protocol);
    m.register_fn("render-image-b64-bytes", graphics::render_image_b64_bytes);
    m.register_fn(
        "kitty-display-image-bytes",
        graphics::kitty_display_image_bytes,
    );

    // ── Kitty Unicode placeholder protocol (virtual placement) ───────────────
    // Direct placement (`a=T` in graphics.rs) pins images to absolute terminal
    // cells, which breaks as soon as the buffer scrolls. Virtual placement
    // transmits the image once under a stable id (`U=1`), then the plugin
    // writes a rectangular grid of placeholder cells into the text buffer;
    // Kitty substitutes the image tiles wherever those cells are drawn, so
    // scrolling and edits move the image naturally without smear artefacts.
    m.register_fn(
        "kitty-placeholder-payload",
        kitty_placeholder::kitty_placeholder_payload,
    );
    m.register_fn(
        "kitty-placeholder-payload-bytes",
        kitty_placeholder::kitty_placeholder_payload_bytes,
    );
    m.register_fn(
        "kitty-placeholder-rows",
        kitty_placeholder::kitty_placeholder_rows,
    );
    m.register_fn(
        "kitty-placeholder-max-dim",
        kitty_placeholder::kitty_placeholder_max_dim,
    );

    // ── File I/O & utilities ─────────────────────────────────────────────────
    m.register_fn("write-string-to-file!", write_string_to_file);
    m.register_fn("path-exists", path_exists);
    m.register_fn("read-file-tail", read_file_tail);
    m.register_fn("resolve-symlink-dir", resolve_symlink_dir);
    m.register_fn("sleep-ms", sleep_ms);

    // ── Image cache persistence ──────────────────────────────────────────────
    m.register_fn("save-image-to-cache!", save_image_to_cache);
    m.register_fn("load-image-from-cache", load_image_from_cache);

    // ── Braille chart rendering ───────────────────────────────────────────
    m.register_fn("render-braille-chart", chart::render_braille_chart);

    // ── Unicode / backslash completion ────────────────────────────────────
    m.register_fn("unicode-lookup", unicode::unicode_lookup);
    m.register_fn(
        "unicode-completions-for-prefix",
        unicode::unicode_completions_for_prefix,
    );
    m.register_fn("latex-overlays", unicode::latex_overlays);
    m.register_fn(
        "latex-overlays-with-options",
        unicode::latex_overlays_with_options,
    );
    m.register_fn("parse-math-spans", unicode::parse_math_spans_json);
    m.register_fn(
        "compute-conceal-overlays-ffi",
        unicode::compute_conceal_overlays,
    );
    m.register_fn(
        "compute-conceal-overlays-for-comments",
        unicode::compute_conceal_overlays_for_comments,
    );
    m.register_fn(
        "compute-conceal-overlays-for-comments-with-options",
        unicode::compute_conceal_overlays_for_comments_with_options,
    );
    m.register_fn(
        "compute-conceal-overlays-for-typst",
        unicode::typst_conceal::typst_overlays_to_tsv,
    );

    // ── Error formatting ─────────────────────────────────────────────
    m.register_fn("format-julia-error", format_julia_error);
    m.register_fn("format-julia-error-with-notebook", format_julia_error_with_notebook);

    // ── LSP environment ───────────────────────────────────────────────
    m.register_fn("ensure-lsp-environment", lsp::ensure_lsp_environment);
    m.register_fn("lsp-environment-ready", lsp::lsp_environment_ready);
    m.register_fn("lsp-project-dir", lsp::lsp_project_dir);
    m.register_fn("lsp-depot-dir", lsp::lsp_depot_dir);

    m
}

// ─── Error formatting ─────────────────────────────────────────────────────────

fn format_julia_error(error_json: String, raw_error: String) -> String {
    error_format::format_error(&error_format::FormatContext {
        error_json: &error_json,
        raw_error: &raw_error,
        notebook_path: None,
    })
}

/// Like `format_julia_error` but also takes the notebook `.jl` path so
/// UndefVarError messages can be enriched by the static-scan enricher
/// — "variable `t` is defined in @cell N (later in the notebook), move
/// it up" instead of the generic "check spelling" hint.
fn format_julia_error_with_notebook(
    error_json: String,
    raw_error: String,
    jl_path: String,
) -> String {
    error_format::format_error(&error_format::FormatContext {
        error_json: &error_json,
        raw_error: &raw_error,
        notebook_path: if jl_path.is_empty() { None } else { Some(&jl_path) },
    })
}

// ─── Misc helpers ─────────────────────────────────────────────────────────────

fn write_string_to_file(path: String, content: String) -> String {
    match std::fs::write(&path, &content) {
        Ok(()) => String::new(),
        Err(e) => format!("ERROR: Failed to write {path}: {e}"),
    }
}

fn path_exists(path: String) -> String {
    if std::path::Path::new(&path).exists() {
        "yes".into()
    } else {
        "no".into()
    }
}

fn read_file_tail(path: String, n: isize) -> String {
    match std::fs::read_to_string(&path) {
        Ok(contents) => {
            let lines: Vec<&str> = contents.lines().collect();
            let start = lines.len().saturating_sub(n as usize);
            lines[start..].join("\n")
        }
        Err(e) => format!("ERROR: {e}"),
    }
}

fn resolve_symlink_dir(path: String) -> String {
    let expanded = if let Some(rest) = path.strip_prefix('~') {
        if let Ok(home) = std::env::var("HOME") {
            format!("{home}{rest}")
        } else {
            path
        }
    } else {
        path
    };
    let p = std::path::Path::new(&expanded);
    match p.canonicalize() {
        Ok(resolved) => resolved
            .parent()
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_default(),
        Err(_) => p
            .parent()
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_default(),
    }
}

fn sleep_ms(ms: isize) {
    std::thread::sleep(std::time::Duration::from_millis(ms as u64));
}

fn save_image_to_cache(jl_path: String, cell_index: isize, b64_data: String) -> String {
    use std::path::Path;

    let jl = Path::new(&jl_path);
    let parent = match jl.parent() {
        Some(p) => p,
        None => return "ERROR: Cannot determine parent directory".into(),
    };

    let cache_dir = parent.join(".nothelix").join("images");
    if let Err(e) = std::fs::create_dir_all(&cache_dir) {
        return format!("ERROR: Cannot create cache dir: {e}");
    }

    let filename = format!("cell-{cell_index}.png");
    let full_path = cache_dir.join(&filename);

    let bytes = match BASE64.decode(b64_data.trim().as_bytes()) {
        Ok(b) => b,
        Err(e) => return format!("ERROR: Invalid base64: {e}"),
    };

    if let Err(e) = std::fs::write(&full_path, &bytes) {
        return format!("ERROR: Cannot write image: {e}");
    }

    format!(".nothelix/images/{filename}")
}

fn load_image_from_cache(jl_path: String, rel_path: String) -> String {
    use std::path::Path;

    let jl = Path::new(&jl_path);
    let parent = match jl.parent() {
        Some(p) => p,
        None => return String::new(),
    };

    let full_path = parent.join(&rel_path);
    match std::fs::read(&full_path) {
        Ok(bytes) => BASE64.encode(&bytes),
        Err(_) => String::new(),
    }
}
