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
//!   - [`extract_markdown_attachments`] — its inverse on ipynb→jl,
//!     writing attachments out to `.nothelix/images/` sidecar files
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

/// Whether `line` is exactly an `![ALT](attachment:NAME)` ref — the
/// empty-alt form [`embed_markdown_attachments`] appends or the
/// alt-texted form vanilla Jupyter writes (`![image.png](attachment:image.png)`).
/// The converter strips these from both sides when deciding if a
/// markdown cell still matches its original — refs are transport
/// artifacts, not user prose.
pub(super) fn is_attachment_ref_line(line: &str) -> bool {
    attachment_ref_name(line).is_some()
}

/// Parse a line that consists solely of `![ALT](attachment:NAME)` and
/// return NAME. ALT may be empty (the form this module emits) or any
/// bracket-free text (vanilla Jupyter uses the filename). Scanner, not
/// regex: prefix `![`, the literal `](attachment:` separator, then a
/// non-empty paren-free name closed by `)` at end of line.
pub(super) fn attachment_ref_name(line: &str) -> Option<&str> {
    let inner = line.trim().strip_prefix("![")?.strip_suffix(')')?;
    let (alt, name) = inner.split_once("](attachment:")?;
    if name.is_empty() || name.contains(['(', ')']) || alt.contains(['[', ']']) {
        return None;
    }
    Some(name)
}

/// Reduce an attachment-map key to a safe sidecar filename: only the
/// final path component survives (both `/` and `\` count as
/// separators), and `.`, `..`, or an empty result are rejected.
/// Attachment keys come verbatim from the `.ipynb` JSON, so a hostile
/// notebook can place traversal sequences there — sanitizing here is
/// what confines every sidecar write to `.nothelix/images/`.
fn sanitize_attachment_filename(key: &str) -> Option<&str> {
    let name = key.rsplit(['/', '\\']).next()?;
    match name {
        "" | "." | ".." => None,
        _ => Some(name),
    }
}

/// Inverse of [`embed_markdown_attachments`]: write each base64
/// attachment of a markdown cell to `.nothelix/images/` next to the
/// notebook and return the body without its `![](attachment:…)` ref
/// lines plus the relative sidecar paths. The caller re-emits the
/// paths as `# @image` markers, so the next .jl → .ipynb conversion
/// re-embeds the same bytes — together the pair forms the lossless
/// attachment round-trip.
///
/// Filename collisions are content-addressed: an existing file with
/// identical bytes is reused, different bytes get a deterministic
/// `-<hash>` suffix so two cells can attach distinct images under the
/// same name. Attachment keys are sanitized to their final path
/// component before any filesystem write (see
/// [`sanitize_attachment_filename`]) so a hostile key can't escape
/// `.nothelix/images/`.
///
/// Attachments that can't be extracted — undecodable base64 payloads
/// or keys that sanitize to nothing — are left alone entirely: their
/// `![…](attachment:…)` ref line stays in the body, and the converter
/// carries the original attachment entry through on the way back for
/// every name still referenced in the body. Exact guarantee: an
/// attachment entry survives `.ipynb → .jl → .ipynb` iff its ref line
/// is still present in the cell body when converting back.
pub(super) fn extract_markdown_attachments(
    markdown_body: &str,
    attachments: Option<&Value>,
    ipynb_path: &str,
) -> (String, Vec<String>) {
    let Some(map) = attachments.and_then(Value::as_object) else {
        return (markdown_body.to_string(), Vec::new());
    };

    let images_dir = Path::new(ipynb_path)
        .parent()
        .map(|p| p.join(".nothelix").join("images"))
        .unwrap_or_else(|| Path::new(".nothelix").join("images"));

    let mut rel_paths = Vec::new();
    let mut extracted: Vec<&str> = Vec::new();
    for (filename, mime_map) in map {
        let Some(safe_name) = sanitize_attachment_filename(filename) else {
            continue;
        };
        let Some(bytes) = attachment_bytes(filename, mime_map) else {
            continue;
        };
        if fs::create_dir_all(&images_dir).is_err() {
            continue;
        }
        let Some(written_name) = write_content_addressed(&images_dir, safe_name, &bytes) else {
            continue;
        };
        extracted.push(filename);
        rel_paths.push(format!(".nothelix/images/{written_name}"));
    }
    if extracted.is_empty() {
        return (markdown_body.to_string(), Vec::new());
    }

    let body = markdown_body
        .lines()
        .filter(|l| !attachment_ref_name(l).is_some_and(|n| extracted.contains(&n)))
        .collect::<Vec<_>>()
        .join("\n");
    (body, rel_paths)
}

/// Decode an attachment's base64 payload, preferring the MIME entry
/// that matches the filename's extension (the one [`embed_markdown_attachments`]
/// writes). nbformat allows mimebundle values as line arrays and
/// base64 with embedded newlines; both are normalized before decoding.
fn attachment_bytes(filename: &str, mime_map: &Value) -> Option<Vec<u8>> {
    let map = mime_map.as_object()?;
    let entry = map.get(mime_for_extension(filename)).or_else(|| {
        map.iter()
            .find(|(k, _)| k.as_str() != "application/x-nothelix-animation")
            .map(|(_, v)| v)
    })?;
    let joined: String = match entry {
        Value::String(s) => s.clone(),
        Value::Array(parts) => parts.iter().filter_map(Value::as_str).collect(),
        _ => return None,
    };
    let compact: String = joined.chars().filter(|c| !c.is_whitespace()).collect();
    BASE64
        .decode(compact.as_bytes())
        .ok()
        .filter(|b| !b.is_empty())
}

/// Write `bytes` into `dir` as `filename`, disambiguating clashes by
/// content: identical bytes reuse the existing file, different bytes
/// land under `<stem>-<hash><ext>`. Returns the filename actually used.
fn write_content_addressed(dir: &Path, filename: &str, bytes: &[u8]) -> Option<String> {
    let target = dir.join(filename);
    if matches!(fs::read(&target), Ok(existing) if existing == bytes) {
        return Some(filename.to_string());
    }
    if !target.exists() {
        return fs::write(&target, bytes)
            .ok()
            .map(|()| filename.to_string());
    }

    let (stem, ext) = match filename.rsplit_once('.') {
        Some((stem, ext)) if !stem.is_empty() => (stem, format!(".{ext}")),
        _ => (filename, String::new()),
    };
    let hashed = format!("{stem}-{:016x}{ext}", content_hash(bytes));
    let hashed_target = dir.join(&hashed);
    if matches!(fs::read(&hashed_target), Ok(existing) if existing == bytes) {
        return Some(hashed);
    }
    fs::write(&hashed_target, bytes).ok().map(|()| hashed)
}

fn content_hash(bytes: &[u8]) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    bytes.hash(&mut h);
    h.finish()
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
