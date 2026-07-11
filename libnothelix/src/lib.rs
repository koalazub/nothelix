//! libnothelix — Steel FFI dylib for the Nothelix Jupyter notebook plugin.
//!
//! This file registers FFI functions with the Steel VM and holds small
//! utility functions that don't warrant their own module.

// Steel's `register_fn` marshals values from the Steel VM and requires
// the registered fn's signature to take owned types (`String`, `RVec<u8>`,
// `isize`), not borrows. The clippy lint is correct that some args
// aren't consumed internally, but the owned type is load-bearing for the
// FFI dispatcher.
#![allow(clippy::needless_pass_by_value)]
#![cfg_attr(not(feature = "native"), allow(dead_code))]

pub mod error_format;
mod math_corpus;
pub use math_corpus::CORPUS;
mod markdown_overlays;
mod typst_export;
mod unicode;

#[cfg(feature = "render")]
mod math_format;
#[cfg(feature = "render")]
mod math_image;
#[cfg(feature = "render")]
mod table_image;
#[cfg(feature = "render")]
pub use math_image::render_math_to_svg;

#[cfg(feature = "native")]
pub mod animation;
#[cfg(feature = "native")]
mod chart;
#[cfg(feature = "native")]
mod graphics;
#[cfg(feature = "native")]
mod health;
#[cfg(feature = "native")]
mod json_utils;
#[cfg(feature = "native")]
mod kernel;
#[cfg(feature = "native")]
mod kitty_placeholder;
#[cfg(feature = "native")]
mod notebook;
#[cfg(feature = "native")]
mod output_store;
#[cfg(feature = "native")]
mod resume;
#[cfg(feature = "native")]
mod trust;

#[cfg(all(feature = "wasm", not(feature = "native")))]
mod wasm;

#[cfg(feature = "native")]
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
#[cfg(feature = "native")]
use steel::steel_vm::ffi::{FFIModule, RegisterFFIFn};

#[cfg(feature = "native")]
steel::declare_module!(build_module);

/// FFI surface version for the libnothelix ↔ plugin handshake.
///
/// Bump rule: ANY change to the FFI surface bumps this — a function
/// added, removed, or renamed, or a signature/semantics change to an
/// existing one. Bump it together with `EXPECTED-FFI-VERSION` in
/// `plugin/nothelix/ffi-version.scm`, which hard-fails the plugin
/// load on mismatch. The plugin `.scm` files are live-linked from the
/// repo while the dylib is a copied artifact, so a forgotten `just
/// install` used to skew the two silently; the handshake turns that
/// into a loud, actionable failure.
pub const NOTHELIX_FFI_VERSION: u32 = 21;

/// Compile-time `BUILD_ID` for this libnothelix. Used by
/// `nothelix doctor` to verify the installed dylib matches the
/// installed fork binary.
///
/// Format:
///   - `ci-<yyyymmdd>-<short-git-sha>` for CI builds
///   - `dev-<short-git-sha>[-dirty]`   for local developer builds
pub fn build_id() -> &'static str {
    env!("NOTHELIX_BUILD_ID")
}

#[cfg(feature = "native")]
fn build_module() -> FFIModule {
    let mut m = FFIModule::new("nothelix");

    // ── FFI handshake ─────────────────────────────────────────────────────────
    // The plugin probes this defensively at load (a pre-handshake dylib
    // doesn't export it at all) and refuses to load on mismatch.
    m.register_fn("nothelix-ffi-version", nothelix_ffi_version);

    // ── Kernel management ─────────────────────────────────────────────────────
    m.register_fn("kernel-start-macro", kernel::kernel_start_macro);
    m.register_fn("kernel-adopt-macro", kernel::kernel_adopt);
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
    m.register_fn(
        "scan-variable-definition",
        notebook::scan_variable_definition,
    );
    m.register_fn("convert-to-ipynb!", notebook::convert_to_ipynb);
    m.register_fn("export-to-markdown!", notebook::export_to_markdown);
    m.register_fn("export-to-typst!", notebook::export_to_typst);
    m.register_fn("render-typst-to-pdf", math_image::render_typst_to_pdf);
    m.register_fn("format-math", math_format::format_math);
    m.register_fn("reserve-math-lines", math_format::reserve_math_lines);
    m.register_fn(
        "math-block-latex-batch",
        math_format::math_block_latex_batch,
    );
    m.register_fn("render-math-to-svg", math_image::render_math_to_svg);
    m.register_fn("start-render-batch", math_image::start_render_batch);
    m.register_fn("poll-render-batch", math_image::poll_render_batch);
    m.register_fn("render-table-to-svg", table_image::render_table_to_svg);
    m.register_fn(
        "start-render-table-batch",
        table_image::start_render_table_batch,
    );
    m.register_fn(
        "scan-markdown-overlays",
        markdown_overlays::scan_markdown_overlays,
    );
    m.register_fn("math-image-grid", math_image::math_image_grid_ffi);

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
    m.register_fn("json-get-animated-mime", json_utils::json_get_animated_mime);
    m.register_fn("json-get-all-images", json_utils::json_get_all_images);
    m.register_fn("json-get-image-count", json_utils::json_get_image_count);

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
    m.register_fn("nothelix-trust-list", trust::trust_list);
    m.register_fn("nothelix-trust-contains", trust::trust_contains);
    m.register_fn("nothelix-trust-add", trust::trust_add);
    m.register_fn("nothelix-trust-remove", trust::trust_remove);
    m.register_fn("resume-get", resume::resume_get);
    m.register_fn("resume-set", resume::resume_set);
    m.register_fn("output-store-put", output_store::output_store_put);
    m.register_fn("output-store-get", output_store::output_store_get);
    m.register_fn("output-store-clear", output_store::output_store_clear);
    m.register_fn("resolve-symlink-dir", resolve_symlink_dir);
    m.register_fn("sleep-ms", sleep_ms);
    m.register_fn("getenv", getenv);

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
        "compute-conceal-overlays-for-comments-with-options",
        unicode::compute_conceal_overlays_for_comments_with_options,
    );
    m.register_fn(
        "compute-conceal-overlays-for-typst",
        unicode::typst_conceal::typst_overlays_to_tsv,
    );

    // ── Error formatting ─────────────────────────────────────────────
    m.register_fn("format-julia-error", format_julia_error);
    m.register_fn(
        "format-julia-error-with-notebook",
        format_julia_error_with_notebook,
    );

    // ── Health check ──────────────────────────────────────────────────
    // Pure-Rust subset of `nothelix doctor`'s static checks. Returns
    // TSV (one issue per line, `id\tmessage\tfix_hint`) for in-editor
    // diagnostics — empty string means healthy.
    m.register_fn(
        "nothelix-health-check-tsv",
        health::nothelix_health_check_tsv,
    );

    // ── Animation ─────────────────────────────────────────────────────────────
    // These complement the raw C-ABI exports (`nothelix_animation_*`). Steel
    // cannot call `extern "C"` functions through `register_fn`; these wrappers
    // use only Steel-marshallable types (String, i64, bool, and bytevectors
    // — RVec<u8> in, FFIValue::ByteVector out).
    //
    // Tick protocol (approach A):
    //   1. Call `(animation-tick-bytes id)` → frame bytes or empty vec.
    //   2. Immediately call the accessor functions to read per-tick metadata
    //      (they read the `last_tick_meta` snapshot set during step 1).
    m.register_fn(
        "animation-register",
        animation::steel_api::animation_register,
    );
    m.register_fn("animation-tick", animation::steel_api::animation_tick);
    m.register_fn(
        "animation-tick-bytes",
        animation::steel_api::animation_tick_bytes,
    );
    m.register_fn(
        "animation-tick-status",
        animation::steel_api::animation_tick_status,
    );
    m.register_fn(
        "animation-tick-height",
        animation::steel_api::animation_tick_height,
    );
    m.register_fn(
        "animation-tick-delay-ms",
        animation::steel_api::animation_tick_delay_ms,
    );
    m.register_fn(
        "animation-tick-frame-index",
        animation::steel_api::animation_tick_frame_index,
    );
    m.register_fn(
        "animation-set-pause",
        animation::steel_api::animation_set_pause,
    );
    m.register_fn("animation-drop", animation::steel_api::animation_drop);

    m
}

// ─── Error formatting ─────────────────────────────────────────────────────────

#[cfg(feature = "native")]
fn format_julia_error(error_json: String, raw_error: String) -> String {
    error_format::format_error(&error_format::FormatContext {
        error_json: &error_json,
        raw_error: &raw_error,
        notebook_path: None,
    })
}

/// Like `format_julia_error` but also takes the notebook `.jl` path so
/// `UndefVarError` messages can be enriched by the static-scan enricher
/// — "variable `t` is defined in @cell N (later in the notebook), move
/// it up" instead of the generic "check spelling" hint.
#[cfg(feature = "native")]
fn format_julia_error_with_notebook(
    error_json: String,
    raw_error: String,
    jl_path: String,
) -> String {
    error_format::format_error(&error_format::FormatContext {
        error_json: &error_json,
        raw_error: &raw_error,
        notebook_path: if jl_path.is_empty() {
            None
        } else {
            Some(&jl_path)
        },
    })
}

// ─── Misc helpers ─────────────────────────────────────────────────────────────

// `isize` because that's the integer type Steel's FFI marshals.
#[cfg(feature = "native")]
fn nothelix_ffi_version() -> isize {
    NOTHELIX_FFI_VERSION as isize
}

#[cfg(feature = "native")]
fn write_string_to_file(path: String, content: String) -> String {
    match std::fs::write(&path, &content) {
        Ok(()) => String::new(),
        Err(e) => format!("ERROR: Failed to write {path}: {e}"),
    }
}

#[cfg(feature = "native")]
fn path_exists(path: String) -> String {
    if std::path::Path::new(&path).exists() {
        "yes".into()
    } else {
        "no".into()
    }
}

#[cfg(feature = "native")]
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

#[cfg(feature = "native")]
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

#[cfg(feature = "native")]
fn sleep_ms(ms: isize) {
    std::thread::sleep(std::time::Duration::from_millis(ms as u64));
}

/// Read an environment variable. Returns the empty string if the
/// variable is unset, so Scheme can treat "" as falsy/missing.
#[cfg(feature = "native")]
fn getenv(name: String) -> String {
    std::env::var(&name).unwrap_or_default()
}

#[cfg(feature = "native")]
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

#[cfg(feature = "native")]
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
