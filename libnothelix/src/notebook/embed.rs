use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use serde_json::{Map, Value, json};

use crate::error::{Error, Result};

const ANIMATION_MIME: &str = "application/x-nothelix-animation";
const SIDECAR_ROOT: &str = ".nothelix";
const SIDECAR_IMAGES: &str = "images";

pub(super) struct Attachments {
    pub body: String,
    pub entries: Value,
}

pub(super) fn embed_markdown_attachments(
    markdown_body: &str,
    image_paths: &[String],
    jl_path: &str,
) -> Result<Attachments> {
    let unchanged = || Attachments {
        body: markdown_body.to_string(),
        entries: json!({}),
    };
    if image_paths.is_empty() {
        return Ok(unchanged());
    }

    let parent = parent_of(jl_path)?;
    let mut entries = Map::new();
    let mut refs = Vec::new();
    for raw_path in image_paths {
        let Some(bytes) = present_image_bytes(&parent.join(raw_path))? else {
            continue;
        };
        let filename = leaf_name(raw_path)?;
        entries.insert(filename.clone(), mime_bundle(&filename, &bytes));
        refs.push(format!("![](attachment:{filename})"));
    }

    if entries.is_empty() {
        return Ok(unchanged());
    }
    let mut body = markdown_body.trim_end().to_string();
    body.push_str("\n\n");
    body.push_str(&refs.join("\n"));
    Ok(Attachments {
        body,
        entries: Value::Object(entries),
    })
}

fn mime_bundle(filename: &str, bytes: &[u8]) -> Value {
    let mime = mime_for_extension(filename);
    let mut bundle = Map::new();
    bundle.insert(mime.to_string(), Value::String(BASE64.encode(bytes)));
    if is_animated_mime(mime) {
        bundle.insert(
            ANIMATION_MIME.to_string(),
            Value::String(json!({"mime": mime, "animated": true}).to_string()),
        );
    }
    Value::Object(bundle)
}

pub(super) fn is_attachment_ref_line(line: &str) -> bool {
    attachment_ref_name(line).is_some()
}

pub(super) fn attachment_ref_name(line: &str) -> Option<&str> {
    let inner = line.trim().strip_prefix("![")?.strip_suffix(')')?;
    let (alt, name) = inner.split_once("](attachment:")?;
    if name.is_empty() || name.contains(['(', ')']) || alt.contains(['[', ']']) {
        return None;
    }
    Some(name)
}

fn sandboxed_name(attachment_key: &str) -> Option<&str> {
    match attachment_key.rsplit(['/', '\\']).next()? {
        "" | "." | ".." => None,
        name => Some(name),
    }
}

pub(super) struct Extraction {
    pub body: String,
    pub sidecar_paths: Vec<String>,
}

pub(super) fn extract_markdown_attachments(
    markdown_body: &str,
    attachments: Option<&Value>,
    ipynb_path: &str,
) -> Result<Extraction> {
    let unchanged = || Extraction {
        body: markdown_body.to_string(),
        sidecar_paths: Vec::new(),
    };
    let Some(map) = attachments.and_then(Value::as_object) else {
        return Ok(unchanged());
    };

    let images_dir = sidecar_images_dir(ipynb_path)?;
    let mut sidecar_paths = Vec::new();
    let mut extracted: Vec<&str> = Vec::new();
    for (attachment_key, bundle) in map {
        let (Some(name), Some(bytes)) = (
            sandboxed_name(attachment_key),
            decodable_payload(attachment_key, bundle),
        ) else {
            continue;
        };
        fs::create_dir_all(&images_dir).map_err(|source| Error::creating(&images_dir, source))?;
        let written = write_content_addressed(&images_dir, name, &bytes)?;
        extracted.push(attachment_key);
        sidecar_paths.push(format!("{SIDECAR_ROOT}/{SIDECAR_IMAGES}/{written}"));
    }
    if extracted.is_empty() {
        return Ok(unchanged());
    }

    Ok(Extraction {
        body: markdown_body
            .lines()
            .filter(|line| !attachment_ref_name(line).is_some_and(|n| extracted.contains(&n)))
            .collect::<Vec<_>>()
            .join("\n"),
        sidecar_paths,
    })
}

fn decodable_payload(filename: &str, bundle: &Value) -> Option<Vec<u8>> {
    let bundle = bundle.as_object()?;
    let payload = bundle.get(mime_for_extension(filename)).or_else(|| {
        bundle
            .iter()
            .find(|(mime, _)| mime.as_str() != ANIMATION_MIME)
            .map(|(_, payload)| payload)
    })?;
    let joined: String = match payload {
        Value::String(text) => text.clone(),
        Value::Array(chunks) => chunks.iter().filter_map(Value::as_str).collect(),
        _ => return None,
    };
    let compact: String = joined.chars().filter(|c| !c.is_whitespace()).collect();
    BASE64
        .decode(compact.as_bytes())
        .ok()
        .filter(|bytes| !bytes.is_empty())
}

fn write_content_addressed(dir: &Path, filename: &str, bytes: &[u8]) -> Result<String> {
    let target = dir.join(filename);
    if present_file_bytes(&target)?.is_some_and(|existing| existing == bytes) {
        return Ok(filename.to_string());
    }
    if !target.exists() {
        fs::write(&target, bytes).map_err(|source| Error::writing(&target, source))?;
        return Ok(filename.to_string());
    }

    let (stem, ext) = match filename.rsplit_once('.') {
        Some((stem, ext)) if !stem.is_empty() => (stem, format!(".{ext}")),
        _ => (filename, String::new()),
    };
    let hashed = format!("{stem}-{:016x}{ext}", content_hash(bytes));
    let hashed_target = dir.join(&hashed);
    if present_file_bytes(&hashed_target)?.is_some_and(|existing| existing == bytes) {
        return Ok(hashed);
    }
    fs::write(&hashed_target, bytes).map_err(|source| Error::writing(&hashed_target, source))?;
    Ok(hashed)
}

fn present_file_bytes(path: &Path) -> Result<Option<Vec<u8>>> {
    match fs::read(path) {
        Ok(bytes) => Ok(Some(bytes)),
        Err(source) if source.kind() == ErrorKind::NotFound => Ok(None),
        Err(source) => Err(Error::reading(path, source)),
    }
}

fn present_image_bytes(path: &Path) -> Result<Option<Vec<u8>>> {
    Ok(present_file_bytes(path)?.filter(|bytes| !bytes.is_empty()))
}

fn parent_of(path: &str) -> Result<PathBuf> {
    Path::new(path)
        .parent()
        .map(Path::to_path_buf)
        .ok_or_else(|| Error::orphan(path))
}

fn sidecar_images_dir(notebook_path: &str) -> Result<PathBuf> {
    Ok(parent_of(notebook_path)?
        .join(SIDECAR_ROOT)
        .join(SIDECAR_IMAGES))
}

fn leaf_name(path: &str) -> Result<String> {
    Path::new(path)
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .ok_or_else(|| Error::Malformed {
            subject: "image marker",
            detail: format!("{path} names no file"),
        })
}

fn content_hash(bytes: &[u8]) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    bytes.hash(&mut hasher);
    hasher.finish()
}

fn mime_for_extension(filename: &str) -> &'static str {
    const BY_EXTENSION: [(&str, &str); 11] = [
        (".png", "image/png"),
        (".jpg", "image/jpeg"),
        (".jpeg", "image/jpeg"),
        (".gif", "image/gif"),
        (".apng", "image/apng"),
        (".webp", "image/webp"),
        (".svg", "image/svg+xml"),
        (".mp4", "video/mp4"),
        (".webm", "video/webm"),
        (".lottie", "application/json+lottie"),
        (".json+lottie", "application/json+lottie"),
    ];
    let lower = filename.to_lowercase();
    BY_EXTENSION
        .into_iter()
        .find(|(extension, _)| lower.ends_with(extension))
        .map_or("application/octet-stream", |(_, mime)| mime)
}

fn is_animated_mime(mime: &str) -> bool {
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

pub(super) fn read_sidecar_image_output(jl_path: &str, cell_index: isize) -> Result<Option<Value>> {
    let image = sidecar_images_dir(jl_path)?.join(format!("cell-{cell_index}.png"));
    Ok(present_image_bytes(&image)?.map(|bytes| {
        json!({
            "output_type": "display_data",
            "data": {"image/png": BASE64.encode(&bytes)},
            "metadata": {}
        })
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_image(dir: &Path, name: &str, bytes: &[u8]) -> String {
        std::fs::write(dir.join(name), bytes).unwrap();
        dir.join("nb.jl").to_string_lossy().into_owned()
    }

    #[test]
    fn animated_mime_gets_nothelix_marker() {
        use crate::animation::decoders::gif_fixture::tiny_gif_bytes;
        let tmp = tempfile::tempdir().unwrap();
        let jl_path = write_image(tmp.path(), "anim.gif", &tiny_gif_bytes());

        let attached =
            embed_markdown_attachments("hi", &["anim.gif".to_string()], &jl_path).unwrap();
        let entry = attached.entries["anim.gif"].as_object().unwrap();
        assert!(entry.contains_key("image/gif"));
        let meta = entry[ANIMATION_MIME].as_str().unwrap();
        assert!(meta.contains("\"mime\":\"image/gif\""));
        assert!(meta.contains("\"animated\":true"));
    }

    #[test]
    fn static_png_does_not_get_nothelix_marker() {
        let tmp = tempfile::tempdir().unwrap();
        let jl_path = write_image(tmp.path(), "static.png", b"\x89PNG\r\n\x1a\n");

        let attached =
            embed_markdown_attachments("hi", &["static.png".to_string()], &jl_path).unwrap();
        let entry = attached.entries["static.png"].as_object().unwrap();
        assert!(entry.contains_key("image/png"));
        assert!(!entry.contains_key(ANIMATION_MIME));
    }

    #[test]
    fn a_missing_image_is_skipped_rather_than_failing_the_conversion() {
        let tmp = tempfile::tempdir().unwrap();
        let jl_path = tmp.path().join("nb.jl").to_string_lossy().into_owned();

        let attached =
            embed_markdown_attachments("hi", &["gone.png".to_string()], &jl_path).unwrap();
        assert_eq!(attached.body, "hi");
        assert_eq!(attached.entries, json!({}));
    }

    #[test]
    fn extensions_map_to_their_iana_mime() {
        for (name, mime) in [
            ("plot.PNG", "image/png"),
            ("photo.jpeg", "image/jpeg"),
            ("anim.gif", "image/gif"),
            ("chart.svg", "image/svg+xml"),
            ("clip.webm", "video/webm"),
            ("motion.json+lottie", "application/json+lottie"),
            ("notes.txt", "application/octet-stream"),
        ] {
            assert_eq!(mime_for_extension(name), mime, "{name}");
        }
    }
}
