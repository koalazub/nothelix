//! Image embedding helpers for `.jl ↔ .ipynb` conversion.
//!
//! Notebook cells carry images via `# @image <path>` markers in the
//! .jl form. On round-trip the converter must lift those paths into
//! portable `.ipynb` forms — `display_data` outputs on code cells or
//! base64 `attachments` on markdown cells — so the notebook still
//! shows its plots when shared. This module owns:
//!
//!   - [`embed_markdown_attachments`] for markdown cells (base64
//!     in-band, rewrites the body to use `![](attachment:…)`)
//!   - [`read_sidecar_image_output`] for code cells (looks for the
//!     cached png that nothelix wrote during a prior execution)
//!   - [`mime_for_extension`] / [`is_animated_mime`] (the MIME lookup
//!     shared by both paths)

use std::fs;
use std::path::Path;

use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use serde_json::{json, Value};

/// Turn a markdown cell's `# @image <path>` markers into an
/// `attachments` map keyed by filename and rewrite the cell source to
/// reference them via `![](attachment:filename)`. Each successfully-
/// read image contributes one `{image/png: "<base64>"}` entry. Files
/// that can't be read are skipped silently — the user's source stays
/// unchanged for those, so the `.jl` can still render them via the
/// sidecar marker even if the `.ipynb` form can't embed them.
pub(super) fn embed_markdown_attachments(
    markdown_body: &str,
    image_paths: &[String],
    jl_path: &str,
) -> (String, Value) {
    if image_paths.is_empty() {
        return (markdown_body.to_string(), json!({})); // sentinel empty — caller discards
    }

    let parent = Path::new(jl_path).parent();
    let mut attachments = serde_json::Map::new();
    let mut image_lines = Vec::new();

    for raw_path in image_paths {
        let path_obj = parent
            .map(|p| p.join(raw_path))
            .unwrap_or_else(|| Path::new(raw_path).to_path_buf());
        let bytes = match fs::read(&path_obj) {
            Ok(b) if !b.is_empty() => b,
            _ => continue,
        };
        let filename = Path::new(raw_path)
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| raw_path.clone());
        let mime = mime_for_extension(&filename);
        let b64 = BASE64.encode(&bytes);
        let mut mime_map = serde_json::Map::new();
        mime_map.insert(mime.to_string(), Value::String(b64));
        if is_animated_mime(mime) {
            let meta = serde_json::json!({
                "mime": mime,
                "animated": true,
            });
            mime_map.insert(
                "application/x-nothelix-animation".to_string(),
                Value::String(meta.to_string()),
            );
        }
        attachments.insert(filename.clone(), Value::Object(mime_map));
        image_lines.push(format!("![](attachment:{filename})"));
    }

    if attachments.is_empty() {
        return (markdown_body.to_string(), json!({}));
    }

    // Inject the image refs at the end of the markdown body, separated
    // by a blank line so they render on their own paragraph.
    let mut body = markdown_body.trim_end().to_string();
    body.push_str("\n\n");
    body.push_str(&image_lines.join("\n"));
    (body, Value::Object(attachments))
}

/// Map a filename's extension to its IANA MIME type. Falls back to
/// `application/octet-stream` for unrecognised extensions so the
/// `.ipynb` writer still emits a syntactically valid output entry.
pub(super) fn mime_for_extension(filename: &str) -> &'static str {
    let lower = filename.to_lowercase();
    if lower.ends_with(".png") {
        "image/png"
    } else if lower.ends_with(".jpg") || lower.ends_with(".jpeg") {
        "image/jpeg"
    } else if lower.ends_with(".gif") {
        "image/gif"
    } else if lower.ends_with(".apng") {
        "image/apng"
    } else if lower.ends_with(".webp") {
        "image/webp"
    } else if lower.ends_with(".svg") {
        "image/svg+xml"
    } else if lower.ends_with(".mp4") {
        "video/mp4"
    } else if lower.ends_with(".webm") {
        "video/webm"
    } else if lower.ends_with(".lottie") || lower.ends_with(".json+lottie") {
        "application/json+lottie"
    } else {
        "application/octet-stream"
    }
}

/// True when the MIME refers to an animated content type the plugin
/// should treat with the animation engine rather than the static
/// kitty-image path.
pub(super) fn is_animated_mime(mime: &str) -> bool {
    matches!(
        mime,
        "image/gif"
            | "image/apng"
            | "image/webp"
            | "video/mp4"
            | "video/webm"
            | "application/json+lottie"
    )
}

/// Look for a plot that nothelix cached from an earlier execution of
/// `cell_index` (`.nothelix/images/cell-N.png` next to the notebook)
/// and build a Jupyter `display_data` output carrying its base64 PNG.
/// Used on `.jl → .ipynb` sync so plots survive into the portable form.
/// Returns `None` when no sidecar file exists.
pub(super) fn read_sidecar_image_output(jl_path: &str, cell_index: isize) -> Option<Value> {
    let parent = Path::new(jl_path).parent()?;
    let img_path = parent
        .join(".nothelix")
        .join("images")
        .join(format!("cell-{cell_index}.png"));
    let bytes = fs::read(&img_path).ok()?;
    if bytes.is_empty() {
        return None;
    }
    let b64 = BASE64.encode(&bytes);
    Some(json!({
        "output_type": "display_data",
        "data": {"image/png": b64},
        "metadata": {}
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn animated_mime_gets_nothelix_marker() {
        use crate::animation::decoders::gif_fixture::tiny_gif_bytes;
        use std::io::Write;
        let tmp = tempfile::tempdir().unwrap();
        let img_path = tmp.path().join("anim.gif");
        let mut f = std::fs::File::create(&img_path).unwrap();
        f.write_all(&tiny_gif_bytes()).unwrap();
        drop(f);

        let images = vec!["anim.gif".to_string()];
        let jl_path = tmp.path().join("nb.jl").to_string_lossy().into_owned();
        let (_body, attachments) = embed_markdown_attachments("hi", &images, &jl_path);
        let obj = attachments.as_object().unwrap();
        let entry = obj.get("anim.gif").unwrap().as_object().unwrap();
        assert!(entry.contains_key("image/gif"));
        assert!(entry.contains_key("application/x-nothelix-animation"));
        let meta_str = entry
            .get("application/x-nothelix-animation")
            .unwrap()
            .as_str()
            .unwrap();
        assert!(meta_str.contains("\"mime\":\"image/gif\""));
        assert!(meta_str.contains("\"animated\":true"));
    }

    #[test]
    fn static_png_does_not_get_nothelix_marker() {
        use std::io::Write;
        let tmp = tempfile::tempdir().unwrap();
        let img_path = tmp.path().join("static.png");
        let mut f = std::fs::File::create(&img_path).unwrap();
        f.write_all(b"\x89PNG\r\n\x1a\n").unwrap();
        drop(f);

        let images = vec!["static.png".to_string()];
        let jl_path = tmp.path().join("nb.jl").to_string_lossy().into_owned();
        let (_body, attachments) = embed_markdown_attachments("hi", &images, &jl_path);
        let obj = attachments.as_object().unwrap();
        let entry = obj.get("static.png").unwrap().as_object().unwrap();
        assert!(entry.contains_key("image/png"));
        assert!(!entry.contains_key("application/x-nothelix-animation"));
    }
}
