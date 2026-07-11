//! JSON field extraction utilities for the Scheme plugin layer.
//!
//! All functions accept a JSON string and return a string value, keeping the
//! Scheme side free from direct JSON parsing.

// Steel's `register_fn` marshals values from the Steel VM and requires the
// registered fn's signature to take owned types (`String`, `RVec<u8>`),
// not borrows. So while clippy::needless_pass_by_value is technically
// correct that we don't consume the args internally, the owned type is
// load-bearing for the FFI dispatcher.
#![allow(clippy::needless_pass_by_value)]

use abi_stable::std_types::RVec;
use serde_json::Value;
use steel::steel_vm::ffi::FFIValue;

pub fn json_get(json_str: String, key: String) -> String {
    serde_json::from_str::<Value>(&json_str)
        .ok()
        .and_then(|v| {
            v.get(&key).map(|val| match val {
                Value::String(s) => s.clone(),
                Value::Bool(b) => b.to_string(),
                Value::Number(n) => n.to_string(),
                other => other.to_string(),
            })
        })
        .unwrap_or_default()
}

pub fn json_get_bool(json_str: String, key: String) -> String {
    serde_json::from_str::<Value>(&json_str)
        .ok()
        .and_then(|v| {
            v.get(&key).map(|val| match val {
                Value::Bool(b) => b.to_string(),
                Value::String(s) => s.clone(),
                _ => "false".to_string(),
            })
        })
        .unwrap_or_else(|| "false".to_string())
}

pub fn json_get_first_image(json_str: String) -> String {
    let parsed: Value = serde_json::from_str(&json_str).unwrap_or(Value::Null);
    find_first_image_data(&parsed).unwrap_or_default()
}

const ANIMATED_MIMES: &[&str] = &[
    "image/gif",
    "image/apng",
    "image/webp",
    "video/mp4",
    "video/webm",
    "application/json+lottie",
];

/// If the given `display_data` JSON contains an animated MIME, returns the MIME
/// string ("image/gif" etc). Returns empty string when only static MIMEs are
/// present. The plugin uses this signal to decide whether to register an
/// animation engine vs render a static image.
pub fn json_get_animated_mime(json_str: String) -> String {
    let parsed: Value = serde_json::from_str(&json_str).unwrap_or(Value::Null);
    find_animated_mime(&parsed).unwrap_or_default()
}

fn find_animated_mime(v: &Value) -> Option<String> {
    match v {
        Value::Object(map) => {
            for &mime in ANIMATED_MIMES {
                if map
                    .get(mime)
                    .and_then(|v| v.as_str())
                    .is_some_and(|s| !s.is_empty())
                {
                    return Some(mime.to_string());
                }
            }
            for val in map.values() {
                if let Some(m) = find_animated_mime(val) {
                    return Some(m);
                }
            }
            None
        }
        Value::Array(arr) => arr.iter().find_map(find_animated_mime),
        _ => None,
    }
}

/// Resolve one image's raw data string to base64: reads and encodes a
/// `"file:"`-prefixed sidecar from `kernel_dir`, or passes legacy
/// already-base64 data through unchanged. Returns `None` if `data` is
/// empty or a sidecar read fails.
///
/// Shared by the first-image (`json_get_first_image_with_dir`) and
/// all-images (`json_get_all_images`) paths so the base64-or-sidecar
/// resolution logic lives in exactly one place.
fn resolve_one_image(data: &str, kernel_dir: &str) -> Option<String> {
    if data.is_empty() {
        return None;
    }
    match data.strip_prefix("file:") {
        Some(filename) => {
            let path = std::path::Path::new(kernel_dir).join(filename);
            std::fs::read(&path).ok().map(|bytes| {
                use base64::Engine;
                base64::engine::general_purpose::STANDARD.encode(&bytes)
            })
        }
        None => Some(data.to_string()),
    }
}

/// Like `json_get_first_image` but resolves sidecar files from `kernel_dir`.
/// If image data starts with `"file:"`, reads the raw PNG and base64-encodes it.
pub fn json_get_first_image_with_dir(json_str: String, kernel_dir: String) -> String {
    let parsed: Value = serde_json::from_str(&json_str).unwrap_or(Value::Null);
    find_first_image_data(&parsed)
        .and_then(|data| resolve_one_image(&data, &kernel_dir))
        .unwrap_or_default()
}

/// Like `json_get_first_image_with_dir` but returns raw bytes instead of
/// base64, as a Steel bytevector (`FFIValue::ByteVector` — stock steel-core's
/// only byte-returning FFI shape). Returns an empty bytevector if no image
/// is found.
pub fn json_get_first_image_bytes(json_str: String, kernel_dir: String) -> FFIValue {
    let parsed: Value = serde_json::from_str(&json_str).unwrap_or(Value::Null);
    let bytes = match find_first_image_data(&parsed) {
        None => Vec::new(),
        Some(data) => {
            if let Some(filename) = data.strip_prefix("file:") {
                // Sidecar file: return raw bytes directly
                let path = std::path::Path::new(&kernel_dir).join(filename);
                std::fs::read(&path).unwrap_or_default()
            } else {
                // Legacy base64: decode to bytes
                use base64::Engine;
                base64::engine::general_purpose::STANDARD
                    .decode(data.trim())
                    .unwrap_or_default()
            }
        }
    };
    FFIValue::ByteVector(RVec::from(bytes))
}

/// Extract multiple fields from a JSON string in one parse.
/// `keys_csv` is comma-separated field names. Returns tab-separated values.
/// Missing fields return empty strings. Non-string values are stringified.
pub fn json_get_many(json_str: String, keys_csv: String) -> String {
    let parsed = if let Ok(v) = serde_json::from_str::<Value>(&json_str) {
        v
    } else {
        let count = keys_csv.split(',').count();
        return "\t".repeat(count.saturating_sub(1));
    };
    keys_csv
        .split(',')
        .map(|key| {
            parsed
                .get(key.trim())
                .map_or(String::new(), |val| match val {
                    Value::String(s) => s.clone(),
                    Value::Bool(b) => b.to_string(),
                    Value::Number(n) => n.to_string(),
                    Value::Null => String::new(),
                    other => other.to_string(),
                })
        })
        .collect::<Vec<_>>()
        .join("\t")
}

/// Extract the `plot_data` field as a JSON string for the braille chart renderer.
/// Returns "" if absent or not an array.
pub fn json_get_plot_data(json_str: String) -> String {
    serde_json::from_str::<Value>(&json_str)
        .ok()
        .and_then(|v| v.get("plot_data").cloned())
        .filter(serde_json::Value::is_array)
        .map(|v| v.to_string())
        .unwrap_or_default()
}

/// Recursively search `v` for image data (base64 or `file:` sidecar marker).
///
/// Handles two formats:
/// - runner.jl: `{"images": [{"format": "png", "data": "..."}]}`
/// - Jupyter:   `{"image/png": "base64..."}`
fn find_first_image_data(v: &Value) -> Option<String> {
    match v {
        Value::Object(map) => {
            // runner.jl images format.
            if let Some(arr) = map.get("images").and_then(|i| i.as_array())
                && let Some(first) = arr.first()
                && let Some(data) = first.get("data").and_then(|d| d.as_str())
                && !data.is_empty()
            {
                return Some(data.to_string());
            }
            // Jupyter-style mime types. Animated MIMEs are searched first so a
            // bundle that contains both `image/gif` and `image/png` (the kernel
            // emits PNG as a static fallback alongside the animated payload)
            // returns the animated bytes; the plugin reads the
            // `application/x-nothelix-animation` marker to know to register an
            // animation engine instead of treating it as a static image.
            for key in &[
                "image/gif",
                "image/apng",
                "image/webp",
                "video/mp4",
                "video/webm",
                "application/json+lottie",
                "image/png",
                "image/jpeg",
            ] {
                if let Some(s) = map.get(*key).and_then(|v| v.as_str())
                    && !s.is_empty()
                {
                    return Some(s.to_string());
                }
            }
            // Recurse into child values.
            for val in map.values() {
                if let Some(img) = find_first_image_data(val) {
                    return Some(img);
                }
            }
            None
        }
        Value::Array(arr) => arr.iter().find_map(find_first_image_data),
        _ => None,
    }
}

/// Collect every entry from the runner.jl `images` array, sidecar-resolved
/// via `resolve_one_image`. Unlike `find_first_image_data`, this only
/// understands the `images` array shape (multiple plots per cell are a
/// runner.jl-only concept; Jupyter mime bundles represent a single display).
fn find_all_image_data(v: &Value, kernel_dir: &str) -> Vec<String> {
    v.get("images")
        .and_then(Value::as_array)
        .map(|arr| {
            arr.iter()
                .filter_map(|img| {
                    let data = img.get("data").and_then(Value::as_str)?;
                    resolve_one_image(data, kernel_dir)
                })
                .collect()
        })
        .unwrap_or_default()
}

/// Every image in a cell's `images` array, sidecar-resolved and joined by
/// `"\n"`. Empty string when there are no images.
pub fn json_get_all_images(json_str: String, kernel_dir: String) -> String {
    let parsed: Value = match serde_json::from_str(&json_str) {
        Ok(v) => v,
        Err(_) => return String::new(),
    };
    find_all_image_data(&parsed, &kernel_dir).join("\n")
}

/// Count of entries in a cell's `images` array, as a decimal string. `"0"`
/// when absent or the JSON is invalid.
pub fn json_get_image_count(json_str: String) -> String {
    let parsed: Value = match serde_json::from_str(&json_str) {
        Ok(v) => v,
        Err(_) => return "0".into(),
    };
    parsed
        .get("images")
        .and_then(Value::as_array)
        .map_or(0, Vec::len)
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn json_get_string_field() {
        let json = r#"{"name": "hello", "count": 42}"#;
        assert_eq!(json_get(json.into(), "name".into()), "hello");
    }

    #[test]
    fn json_get_number_field() {
        let json = r#"{"count": 42}"#;
        assert_eq!(json_get(json.into(), "count".into()), "42");
    }

    #[test]
    fn json_get_missing_field() {
        let json = r#"{"name": "hello"}"#;
        assert_eq!(json_get(json.into(), "missing".into()), "");
    }

    #[test]
    fn json_get_bool_true() {
        let json = r#"{"flag": true}"#;
        assert_eq!(json_get_bool(json.into(), "flag".into()), "true");
    }

    #[test]
    fn json_get_bool_missing_defaults_false() {
        let json = r#"{"other": 1}"#;
        assert_eq!(json_get_bool(json.into(), "flag".into()), "false");
    }

    #[test]
    fn json_get_bool_from_string() {
        let json = r#"{"flag": "true"}"#;
        assert_eq!(json_get_bool(json.into(), "flag".into()), "true");
    }

    #[test]
    fn first_image_runner_format() {
        let json = r#"{"images": [{"format": "png", "data": "iVBORw0KGgo="}]}"#;
        assert_eq!(json_get_first_image(json.into()), "iVBORw0KGgo=");
    }

    #[test]
    fn first_image_jupyter_format() {
        let json = r#"{"image/png": "iVBORw0KGgo="}"#;
        assert_eq!(json_get_first_image(json.into()), "iVBORw0KGgo=");
    }

    #[test]
    fn first_image_nested() {
        let json = r#"{"data": {"image/png": "abc123"}}"#;
        assert_eq!(json_get_first_image(json.into()), "abc123");
    }

    #[test]
    fn first_image_empty_data_skipped() {
        let json = r#"{"images": [{"format": "png", "data": ""}]}"#;
        assert_eq!(json_get_first_image(json.into()), "");
    }

    #[test]
    fn first_image_no_images() {
        let json = r#"{"stdout": "hello"}"#;
        assert_eq!(json_get_first_image(json.into()), "");
    }

    #[test]
    fn animated_mime_picked_before_png_when_both_present() {
        // Kernel emits the animated payload AND a PNG static fallback in the
        // same display_data bundle. find_first_image_data must return the
        // animated bytes so the plugin sees animation-eligible data.
        let json = r#"{"data": {"image/gif": "GIFBASE64", "image/png": "PNGBASE64"}}"#;
        assert_eq!(json_get_first_image(json.into()), "GIFBASE64");
    }

    #[test]
    fn json_get_animated_mime_returns_gif() {
        let json = r#"{"data": {"image/gif": "abc", "image/png": "xyz"}}"#;
        assert_eq!(json_get_animated_mime(json.into()), "image/gif");
    }

    #[test]
    fn json_get_animated_mime_returns_empty_for_static_only() {
        let json = r#"{"data": {"image/png": "xyz"}}"#;
        assert_eq!(json_get_animated_mime(json.into()), "");
    }

    #[test]
    fn json_get_invalid_json() {
        assert_eq!(json_get("not json".into(), "key".into()), "");
    }

    #[test]
    fn json_get_many_extracts_multiple() {
        let json = r#"{"error": "boom", "stdout": "hi", "stderr": "", "has_error": true}"#;
        let result = json_get_many(json.into(), "error,stdout,stderr,has_error".into());
        assert_eq!(result, "boom\thi\t\ttrue");
    }

    #[test]
    fn json_get_many_missing_fields() {
        let json = r#"{"a": "1"}"#;
        let result = json_get_many(json.into(), "a,b,c".into());
        assert_eq!(result, "1\t\t");
    }

    #[test]
    fn json_get_many_invalid_json() {
        let result = json_get_many("not json".into(), "a,b".into());
        assert_eq!(result, "\t");
    }

    #[test]
    fn first_image_sidecar_file() {
        // Create a temp dir with a fake PNG sidecar
        let dir = tempfile::tempdir().unwrap();
        let png_path = dir.path().join("image_1.png");
        std::fs::write(&png_path, b"\x89PNG fake data").unwrap();

        let json = r#"{"images": [{"format": "png", "data": "file:image_1.png"}]}"#;
        let result =
            json_get_first_image_with_dir(json.into(), dir.path().to_string_lossy().into_owned());
        // Should be base64 of the raw bytes
        assert!(!result.is_empty());
        assert!(!result.starts_with("file:"));
        // Decode and verify
        use base64::Engine;
        let decoded = base64::engine::general_purpose::STANDARD
            .decode(&result)
            .unwrap();
        assert_eq!(decoded, b"\x89PNG fake data");
    }

    #[test]
    fn first_image_legacy_base64_passthrough() {
        let json = r#"{"images": [{"format": "png", "data": "iVBORw0KGgo="}]}"#;
        let result = json_get_first_image_with_dir(json.into(), "/nonexistent".into());
        assert_eq!(result, "iVBORw0KGgo=");
    }

    #[test]
    fn first_image_sidecar_missing_file() {
        let json = r#"{"images": [{"format": "png", "data": "file:missing.png"}]}"#;
        let result = json_get_first_image_with_dir(json.into(), "/nonexistent".into());
        assert_eq!(result, "");
    }

    #[test]
    fn all_images_returns_every_entry() {
        let j = r#"{"images":[{"format":"png","data":"AAA"},{"format":"png","data":"BBB"}]}"#;
        let out = json_get_all_images(j.to_string(), String::new());
        assert_eq!(out.lines().count(), 2);
        assert!(out.contains("AAA") && out.contains("BBB"));
    }

    #[test]
    fn all_images_empty_when_none() {
        assert_eq!(
            json_get_all_images(r#"{"images":[]}"#.to_string(), String::new()),
            ""
        );
    }

    #[test]
    fn image_count_matches() {
        let j = r#"{"images":[{"data":"A"},{"data":"B"},{"data":"C"}]}"#;
        assert_eq!(json_get_image_count(j.to_string()), "3");
    }

    #[test]
    fn all_images_resolves_sidecar_file() {
        let dir = tempfile::tempdir().unwrap();
        let png_path = dir.path().join("image_1.png");
        std::fs::write(&png_path, b"\x89PNG fake data").unwrap();

        let json = r#"{"images":[{"format":"png","data":"file:image_1.png"},{"format":"png","data":"BBB"}]}"#;
        let out = json_get_all_images(json.into(), dir.path().to_string_lossy().into_owned());
        let lines: Vec<&str> = out.lines().collect();
        assert_eq!(lines.len(), 2);
        assert!(!lines[0].starts_with("file:"));
        use base64::Engine;
        let decoded = base64::engine::general_purpose::STANDARD
            .decode(lines[0])
            .unwrap();
        assert_eq!(decoded, b"\x89PNG fake data");
        assert_eq!(lines[1], "BBB");
    }

    #[test]
    fn all_images_skips_missing_sidecar() {
        let json = r#"{"images":[{"format":"png","data":"file:missing.png"},{"format":"png","data":"BBB"}]}"#;
        let out = json_get_all_images(json.into(), "/nonexistent".into());
        assert_eq!(out, "BBB");
    }

    #[test]
    fn all_images_no_images_key() {
        let json = r#"{"stdout": "hello"}"#;
        assert_eq!(json_get_all_images(json.into(), String::new()), "");
    }

    #[test]
    fn image_count_no_images_key() {
        assert_eq!(json_get_image_count(r#"{"stdout": "hi"}"#.to_string()), "0");
    }

    #[test]
    fn image_count_invalid_json() {
        assert_eq!(json_get_image_count("not json".to_string()), "0");
    }
}
