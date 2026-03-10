//! libnothelix — Steel FFI dylib for the Nothelix Jupyter notebook plugin.
//!
//! This file is intentionally thin: it only registers FFI functions with the
//! Steel VM.  All implementation lives in the sub-modules below.

mod graphics;
mod json_utils;
mod kernel;
mod notebook;

use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use steel::steel_vm::ffi::{FFIModule, RegisterFFIFn};

// ─── Steel FFI entry point ────────────────────────────────────────────────────

steel::declare_module!(build_module);

fn build_module() -> FFIModule {
    let mut m = FFIModule::new("nothelix");

    // ── Kernel management ─────────────────────────────────────────────────────
    m.register_fn("find-julia-executable", kernel::find_julia_executable);
    m.register_fn("kernel-start-macro", kernel::kernel_start_macro);
    m.register_fn("kernel-stop", kernel::kernel_stop);
    m.register_fn(
        "kernel-stop-all-processes",
        kernel::kernel_stop_all_processes,
    );

    // ── Notebook operations ───────────────────────────────────────────────────
    m.register_fn("notebook-validate", notebook::notebook_validate);
    m.register_fn("notebook-convert-sync", notebook::notebook_convert_sync);
    m.register_fn("notebook-cell-count", notebook::notebook_cell_count);
    m.register_fn("notebook-get-cell-code", notebook::notebook_get_cell_code);
    m.register_fn("get-cell-at-line", notebook::get_cell_at_line);
    m.register_fn(
        "get-notebook-source-path",
        notebook::get_notebook_source_path,
    );
    m.register_fn("get-cell-code-from-jl", notebook::get_cell_code_from_jl);
    m.register_fn("list-jl-code-cells", notebook::list_jl_code_cells);
    m.register_fn("convert-to-ipynb", notebook::convert_to_ipynb);
    m.register_fn(
        "notebook-cell-image-data",
        notebook::notebook_cell_image_data,
    );

    // ── Execution ─────────────────────────────────────────────────────────────
    m.register_fn("kernel-execute-cell", kernel::kernel_execute_cell);
    m.register_fn(
        "kernel-execute-cell-start",
        kernel::kernel_execute_cell_start,
    );
    m.register_fn("kernel-poll-result", kernel::kernel_poll_result);
    m.register_fn("kernel-interrupt", kernel::kernel_interrupt);

    // ── JSON utilities ────────────────────────────────────────────────────────
    m.register_fn("json-get", json_utils::json_get);
    m.register_fn("json-get-bool", json_utils::json_get_bool);
    m.register_fn("json-get-first-image", json_utils::json_get_first_image);

    // ── Graphics ──────────────────────────────────────────────────────────────
    m.register_fn("config-get-protocol", graphics::config_get_protocol);
    m.register_fn(
        "detect-graphics-protocol",
        graphics::detect_graphics_protocol,
    );
    m.register_fn("render-image-bytes", graphics::render_image_bytes);
    m.register_fn("render-image-b64-bytes", graphics::render_image_b64_bytes);
    m.register_fn("viuer-protocol", graphics::viuer_protocol);
    m.register_fn("image-detect-format", graphics::image_detect_format);
    m.register_fn(
        "image-detect-format-bytes",
        graphics::image_detect_format_bytes,
    );
    m.register_fn(
        "kitty-display-image-bytes",
        graphics::kitty_display_image_bytes,
    );
    m.register_fn("kitty-placeholder-image", graphics::kitty_placeholder_image);
    m.register_fn("kitty-display-image", graphics::kitty_display_image);
    m.register_fn("write-raw-to-tty", graphics::write_raw_to_tty);

    // ── File I/O & shell-free utilities ────────────────────────────────────────
    m.register_fn("write-string-to-file", write_string_to_file);
    m.register_fn("path-exists", path_exists);
    m.register_fn("read-file-tail", read_file_tail);
    m.register_fn("read-file-to-string", read_file_to_string);
    m.register_fn("resolve-symlink-dir", resolve_symlink_dir);
    m.register_fn("sleep-ms", sleep_ms);

    // ── Base64 / logging ──────────────────────────────────────────────────────
    m.register_fn("base64-decode-to-string", base64_decode_to_string);
    m.register_fn("log-info", log_info_fn);

    m
}

// ─── Misc helpers kept in lib.rs (no dedicated module warranted) ──────────────

fn write_string_to_file(path: String, content: String) -> String {
    match std::fs::write(&path, &content) {
        Ok(()) => String::new(),
        Err(e) => format!("ERROR: Failed to write {path}: {e}"),
    }
}

/// Check whether a path exists (file or directory). Returns "yes" or "no".
fn path_exists(path: String) -> String {
    if std::path::Path::new(&path).exists() {
        "yes".into()
    } else {
        "no".into()
    }
}

/// Read the last `n` lines of a file. Returns the text or an error message.
fn read_file_tail(path: String, n: isize) -> String {
    match std::fs::read_to_string(&path) {
        Ok(contents) => {
            let lines: Vec<&str> = contents.lines().collect();
            let n = n as usize;
            let start = lines.len().saturating_sub(n);
            lines[start..].join("\n")
        }
        Err(e) => format!("ERROR: {e}"),
    }
}

/// Read an entire file to a string. Returns "" on error.
fn read_file_to_string(path: String) -> String {
    std::fs::read_to_string(&path).unwrap_or_default()
}

/// Resolve a symlink and return its parent directory.
/// Given a path like `~/.config/helix/nothelix.scm` (which is a symlink),
/// resolves it and returns the dirname of the target.
/// Expands `~` to `$HOME` before resolving.
fn resolve_symlink_dir(path: String) -> String {
    let expanded = if path.starts_with('~') {
        if let Ok(home) = std::env::var("HOME") {
            format!("{home}{}", &path[1..])
        } else {
            path.clone()
        }
    } else {
        path.clone()
    };
    let p = std::path::Path::new(&expanded);
    match p.canonicalize() {
        Ok(resolved) => resolved
            .parent()
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_default(),
        Err(_) => {
            // Fallback: just return parent of the input path
            p.parent()
                .map(|p| p.to_string_lossy().into_owned())
                .unwrap_or_default()
        }
    }
}

/// Sleep for the given number of milliseconds (blocks the current thread).
fn sleep_ms(ms: isize) {
    std::thread::sleep(std::time::Duration::from_millis(ms as u64));
}

fn base64_decode_to_string(b64: String) -> String {
    BASE64
        .decode(b64.trim())
        .ok()
        .and_then(|b| String::from_utf8(b).ok())
        .unwrap_or_default()
}

fn log_info_fn(msg: String) {
    eprintln!("[nothelix] {msg}");
}
