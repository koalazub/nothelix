#![cfg_attr(
    not(any(feature = "native", feature = "wasm")),
    allow(dead_code, unused_imports)
)]

pub mod error;
pub mod error_format;
mod math_corpus;
pub use math_corpus::CORPUS;
mod typst_export;
mod unicode;

#[cfg(feature = "native")]
mod markdown_overlays;

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
mod audio;
#[cfg(feature = "native")]
mod chart;
#[cfg(feature = "native")]
pub mod gallery;
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
mod slm;
#[cfg(feature = "native")]
mod trust;

#[cfg(all(feature = "wasm", not(feature = "native")))]
mod wasm;

#[cfg(feature = "native")]
use steel::steel_vm::ffi::FFIModule;

#[cfg(feature = "native")]
steel::declare_module!(build_module);

pub const NOTHELIX_FFI_VERSION: u32 = 31;

pub fn build_id() -> &'static str {
    env!("NOTHELIX_BUILD_ID")
}

#[cfg(feature = "native")]
fn build_module() -> FFIModule {
    let mut module = FFIModule::new("nothelix");
    steel_bindings::register_all(&mut module);
    module
}

#[cfg(feature = "native")]
mod steel_bindings {
    use steel::steel_vm::ffi::{FFIModule, RegisterFFIFn};

    mod handshake {
        use super::{FFIModule, RegisterFFIFn};
        use crate::NOTHELIX_FFI_VERSION;

        fn nothelix_ffi_version() -> isize {
            NOTHELIX_FFI_VERSION as isize
        }

        pub(super) fn register(module: &mut FFIModule) {
            module.register_fn("nothelix-ffi-version", nothelix_ffi_version);
        }
    }

    mod kernel_lifecycle {
        use super::{FFIModule, RegisterFFIFn};
        use crate::kernel;

        pub(super) fn register(module: &mut FFIModule) {
            module.register_fn("kernel-start-macro", kernel::kernel_start_macro);
            module.register_fn("kernel-adopt-macro", kernel::kernel_adopt);
            module.register_fn("kernel-stop", kernel::kernel_stop);
            module.register_fn(
                "kernel-stop-all-processes",
                kernel::kernel_stop_all_processes,
            );
        }
    }

    mod kernel_execution {
        use super::{FFIModule, RegisterFFIFn};
        use crate::kernel;

        pub(super) fn register(module: &mut FFIModule) {
            module.register_fn(
                "kernel-execute-cell-start",
                kernel::kernel_execute_cell_start,
            );
            module.register_fn("kernel-poll-result", kernel::kernel_poll_result);
            module.register_fn("kernel-interrupt", kernel::kernel_interrupt);
            module.register_fn("kernel-set-var", kernel::kernel_set_var);
            module.register_fn("kernel-runner-stale", kernel::kernel_runner_stale);
        }
    }

    mod notebook_documents {
        use super::{FFIModule, RegisterFFIFn};
        use crate::notebook;

        pub(super) fn register(module: &mut FFIModule) {
            module.register_fn("notebook-validate", notebook::notebook_validate);
            module.register_fn("notebook-convert-sync!", notebook::notebook_convert_sync);
            module.register_fn("notebook-cell-count", notebook::notebook_cell_count);
            module.register_fn("notebook-get-cell-code", notebook::notebook_get_cell_code);
            module.register_fn("get-cell-at-line", notebook::get_cell_at_line);
            module.register_fn("get-cell-code-from-jl", notebook::get_cell_code_from_jl);
            module.register_fn("list-jl-code-cells", notebook::list_jl_code_cells);
            module.register_fn(
                "scan-variable-definition",
                notebook::scan_variable_definition,
            );
            module.register_fn("convert-to-ipynb!", notebook::convert_to_ipynb);
            module.register_fn("export-to-markdown!", notebook::export_to_markdown);
            module.register_fn("export-to-typst!", notebook::export_to_typst);
        }
    }

    mod typeset_render {
        use super::{FFIModule, RegisterFFIFn};
        use crate::{markdown_overlays, math_format, math_image, table_image};

        pub(super) fn register(module: &mut FFIModule) {
            module.register_fn("render-typst-to-pdf", math_image::render_typst_to_pdf);
            module.register_fn("format-math", math_format::format_math);
            module.register_fn("reserve-math-lines", math_format::reserve_math_lines);
            module.register_fn("canonical-cell-hash", math_format::canonical_cell_hash);
            module.register_fn(
                "math-block-latex-batch",
                math_format::math_block_latex_batch,
            );
            module.register_fn("render-math-to-svg", math_image::render_math_to_svg);
            module.register_fn("start-render-batch", math_image::start_render_batch);
            module.register_fn("poll-render-batch", math_image::poll_render_batch);
            module.register_fn("render-table-to-svg", table_image::render_table_to_svg);
            module.register_fn(
                "start-render-table-batch",
                table_image::start_render_table_batch,
            );
            module.register_fn(
                "scan-markdown-overlays",
                markdown_overlays::scan_markdown_overlays,
            );
            module.register_fn("math-image-grid", math_image::math_image_grid_ffi);
        }
    }

    mod json_access {
        use super::{FFIModule, RegisterFFIFn};
        use crate::json_utils;

        pub(super) fn register(module: &mut FFIModule) {
            module.register_fn("json-get", json_utils::json_get);
            module.register_fn("json-get-bool", json_utils::json_get_bool);
            module.register_fn("json-get-many", json_utils::json_get_many);
            module.register_fn(
                "json-get-first-image-bytes",
                json_utils::json_get_first_image_bytes,
            );
            module.register_fn("json-get-plot-data", json_utils::json_get_plot_data);
            module.register_fn("json-get-notes", json_utils::json_get_notes);
            module.register_fn("json-get-cell-states", json_utils::json_get_cell_states);
            module.register_fn("json-get-animated-mime", json_utils::json_get_animated_mime);
            module.register_fn("json-get-all-images", json_utils::json_get_all_images);
            module.register_fn("json-get-text-plots", json_utils::json_get_text_plots);
            module.register_fn("json-get-audio", json_utils::json_get_audio);
            module.register_fn("json-get-widgets", json_utils::json_get_widgets);
        }
    }

    mod terminal_graphics {
        use super::{FFIModule, RegisterFFIFn};
        use crate::{graphics, kitty_placeholder};

        pub(super) fn register(module: &mut FFIModule) {
            module.register_fn("viuer-protocol", graphics::viuer_protocol);
            module.register_fn("render-image-b64-bytes", graphics::render_image_b64_bytes);
            module.register_fn(
                "kitty-display-image-bytes",
                graphics::kitty_display_image_bytes,
            );
            module.register_fn(
                "kitty-placeholder-payload",
                kitty_placeholder::kitty_placeholder_payload,
            );
            module.register_fn(
                "kitty-placeholder-payload-bytes",
                kitty_placeholder::kitty_placeholder_payload_bytes,
            );
            module.register_fn(
                "kitty-placeholder-rows",
                kitty_placeholder::kitty_placeholder_rows,
            );
            module.register_fn(
                "kitty-placeholder-max-dim",
                kitty_placeholder::kitty_placeholder_max_dim,
            );
        }
    }

    mod filesystem {
        use super::{FFIModule, RegisterFFIFn};
        use crate::error::{Error, Result, ffi};
        use std::path::{Path, PathBuf};

        fn write_string_to_file(path: String, content: String) -> String {
            ffi(overwrite_file(&path, &content))
        }

        fn overwrite_file(path: &str, content: &str) -> Result<String> {
            std::fs::write(path, content).map_err(|source| Error::writing(path, source))?;
            Ok(String::new())
        }

        fn path_exists(path: String) -> String {
            if Path::new(&path).exists() {
                "yes".into()
            } else {
                "no".into()
            }
        }

        fn read_file_tail(path: String, n: isize) -> String {
            ffi(last_lines(&path, n as usize))
        }

        fn last_lines(path: &str, wanted: usize) -> Result<String> {
            let contents =
                std::fs::read_to_string(path).map_err(|source| Error::reading(path, source))?;
            let lines: Vec<&str> = contents.lines().collect();
            let start = lines.len().saturating_sub(wanted);
            Ok(lines[start..].join("\n"))
        }

        fn expand_home(path: &str) -> PathBuf {
            match (path.strip_prefix('~'), std::env::var("HOME")) {
                (Some(rest), Ok(home)) => PathBuf::from(format!("{home}{rest}")),
                _ => PathBuf::from(path),
            }
        }

        fn resolve_symlink_dir(path: String) -> String {
            let literal = expand_home(&path);
            let canonical = literal.canonicalize();
            let target = canonical.as_deref().unwrap_or(&literal);
            match target.parent() {
                Some(parent) => parent.to_string_lossy().into_owned(),
                None => String::new(),
            }
        }

        pub(super) fn register(module: &mut FFIModule) {
            module.register_fn("write-string-to-file!", write_string_to_file);
            module.register_fn("path-exists", path_exists);
            module.register_fn("read-file-tail", read_file_tail);
            module.register_fn("resolve-symlink-dir", resolve_symlink_dir);
        }
    }

    mod workspace_state {
        use super::{FFIModule, RegisterFFIFn};
        use crate::{output_store, resume, slm, trust};

        pub(super) fn register(module: &mut FFIModule) {
            module.register_fn("nothelix-trust-list", trust::trust_list);
            module.register_fn("nothelix-trust-contains", trust::trust_contains);
            module.register_fn("nothelix-trust-add", trust::trust_add);
            module.register_fn("nothelix-trust-remove", trust::trust_remove);
            module.register_fn("resume-get", resume::resume_get);
            module.register_fn("resume-set", resume::resume_set);
            module.register_fn("output-store-put", output_store::output_store_put);
            module.register_fn("output-store-get", output_store::output_store_get);
            module.register_fn("output-store-clear", output_store::output_store_clear);
            module.register_fn("slm-available", slm::slm_available);
            module.register_fn("djb2-hash", slm::djb2_hash_ffi);
            module.register_fn("slm-refresh-summaries", slm::slm_refresh_summaries);
            module.register_fn("slm-summary-for", slm::slm_summary_for);
        }
    }

    mod host_runtime {
        use super::{FFIModule, RegisterFFIFn};

        fn sleep_ms(ms: isize) {
            std::thread::sleep(std::time::Duration::from_millis(ms as u64));
        }

        fn getenv(name: String) -> String {
            std::env::var(&name).unwrap_or_default()
        }

        pub(super) fn register(module: &mut FFIModule) {
            module.register_fn("sleep-ms", sleep_ms);
            module.register_fn("getenv", getenv);
        }
    }

    mod image_cache {
        use super::{FFIModule, RegisterFFIFn};
        use crate::error::{Error, Result, ffi};
        use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
        use std::path::Path;

        fn save_image_to_cache(
            jl_path: String,
            cell_index: isize,
            img_index: isize,
            b64_data: String,
        ) -> String {
            ffi(store_decoded_image(
                &jl_path, cell_index, img_index, &b64_data,
            ))
        }

        fn store_decoded_image(
            jl_path: &str,
            cell_index: isize,
            img_index: isize,
            b64_data: &str,
        ) -> Result<String> {
            let notebook = Path::new(jl_path);
            let cache_dir = notebook
                .parent()
                .ok_or_else(|| Error::orphan(notebook))?
                .join(".nothelix")
                .join("images");
            std::fs::create_dir_all(&cache_dir)
                .map_err(|source| Error::creating(&cache_dir, source))?;

            let encoded = b64_data.trim();
            let bytes = BASE64
                .decode(encoded.as_bytes())
                .map_err(|source| Error::Base64 {
                    subject: "cached cell image",
                    length: encoded.len(),
                    source,
                })?;

            let filename = format!("cell-{cell_index}-{img_index}.png");
            let full_path = cache_dir.join(&filename);
            std::fs::write(&full_path, &bytes)
                .map_err(|source| Error::writing(&full_path, source))?;

            Ok(format!(".nothelix/images/{filename}"))
        }

        fn load_image_from_cache(jl_path: String, rel_path: String) -> String {
            cached_image(&jl_path, &rel_path).unwrap_or_default()
        }

        fn cached_image(jl_path: &str, rel_path: &str) -> Option<String> {
            let parent = Path::new(jl_path).parent()?;
            let bytes = std::fs::read(parent.join(rel_path)).ok()?;
            Some(BASE64.encode(&bytes))
        }

        pub(super) fn register(module: &mut FFIModule) {
            module.register_fn("save-image-to-cache!", save_image_to_cache);
            module.register_fn("load-image-from-cache", load_image_from_cache);
        }
    }

    mod braille_chart {
        use super::{FFIModule, RegisterFFIFn};
        use crate::chart;

        pub(super) fn register(module: &mut FFIModule) {
            module.register_fn("render-braille-chart", chart::render_braille_chart);
        }
    }

    mod unicode_completion {
        use super::{FFIModule, RegisterFFIFn};
        use crate::unicode;

        pub(super) fn register(module: &mut FFIModule) {
            module.register_fn("unicode-lookup", unicode::unicode_lookup);
            module.register_fn(
                "unicode-completions-for-prefix",
                unicode::unicode_completions_for_prefix,
            );
            module.register_fn("latex-overlays", unicode::latex_overlays);
            module.register_fn(
                "latex-overlays-with-options",
                unicode::latex_overlays_with_options,
            );
            module.register_fn("parse-math-spans", unicode::parse_math_spans_json);
            module.register_fn(
                "compute-conceal-overlays-ffi",
                unicode::compute_conceal_overlays,
            );
            module.register_fn(
                "compute-conceal-overlays-for-comments-with-options",
                unicode::compute_conceal_overlays_for_comments_with_options,
            );
            module.register_fn(
                "compute-conceal-overlays-for-typst",
                unicode::typst_conceal::typst_overlays_to_tsv,
            );
        }
    }

    mod julia_diagnostics {
        use super::{FFIModule, RegisterFFIFn};
        use crate::error_format::{FormatContext, format_error};
        use crate::health;

        fn format_julia_error(error_json: String, raw_error: String) -> String {
            format_error(&FormatContext {
                error_json: &error_json,
                raw_error: &raw_error,
                notebook_path: None,
            })
        }

        fn format_julia_error_with_notebook(
            error_json: String,
            raw_error: String,
            jl_path: String,
        ) -> String {
            format_error(&FormatContext {
                error_json: &error_json,
                raw_error: &raw_error,
                notebook_path: (!jl_path.is_empty()).then_some(jl_path.as_str()),
            })
        }

        pub(super) fn register(module: &mut FFIModule) {
            module.register_fn("format-julia-error", format_julia_error);
            module.register_fn(
                "format-julia-error-with-notebook",
                format_julia_error_with_notebook,
            );
            module.register_fn(
                "nothelix-health-check-tsv",
                health::nothelix_health_check_tsv,
            );
        }
    }

    mod animation_playback {
        use super::{FFIModule, RegisterFFIFn};
        use crate::animation::steel_api;

        pub(super) fn register(module: &mut FFIModule) {
            module.register_fn("animation-register", steel_api::animation_register);
            module.register_fn("animation-tick", steel_api::animation_tick);
            module.register_fn("animation-tick-bytes", steel_api::animation_tick_bytes);
            module.register_fn("animation-tick-status", steel_api::animation_tick_status);
            module.register_fn("animation-tick-height", steel_api::animation_tick_height);
            module.register_fn(
                "animation-tick-delay-ms",
                steel_api::animation_tick_delay_ms,
            );
            module.register_fn(
                "animation-tick-frame-index",
                steel_api::animation_tick_frame_index,
            );
            module.register_fn("animation-set-pause", steel_api::animation_set_pause);
            module.register_fn("animation-drop", steel_api::animation_drop);
        }
    }

    mod audio_playback {
        use super::{FFIModule, RegisterFFIFn};
        use crate::audio;

        pub(super) fn register(module: &mut FFIModule) {
            module.register_fn("audio-play", audio::audio_play);
            module.register_fn("audio-play-from", audio::audio_play_from);
            module.register_fn("audio-stop", audio::audio_stop);
            module.register_fn("audio-stop-all", audio::audio_stop_all);
            module.register_fn("audio-playing", audio::audio_playing);
            module.register_fn("audio-position", audio::audio_position);
            module.register_fn("audio-waveform", audio::audio_waveform);
            module.register_fn("audio-info", audio::audio_info);
        }
    }

    const REGISTRARS: &[fn(&mut FFIModule)] = &[
        handshake::register,
        kernel_lifecycle::register,
        kernel_execution::register,
        notebook_documents::register,
        typeset_render::register,
        json_access::register,
        terminal_graphics::register,
        filesystem::register,
        workspace_state::register,
        host_runtime::register,
        image_cache::register,
        braille_chart::register,
        unicode_completion::register,
        julia_diagnostics::register,
        animation_playback::register,
        audio_playback::register,
    ];

    pub(super) fn register_all(module: &mut FFIModule) {
        for register in REGISTRARS {
            register(module);
        }
    }
}
