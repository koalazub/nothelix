//! JSON field extraction utilities for the Scheme plugin layer.
//!
//! All functions accept a JSON string and return a string value, keeping the
//! Scheme side free from direct JSON parsing.

use serde_json::Value;

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

/// Like `json_get_first_image` but resolves sidecar files from `kernel_dir`.
/// If image data starts with `"file:"`, reads the raw PNG and base64-encodes it.
pub fn json_get_first_image_with_dir(json_str: String, kernel_dir: String) -> String {
    let parsed: Value = serde_json::from_str(&json_str).unwrap_or(Value::Null);
    match find_first_image_data(&parsed) {
        None => String::new(),
        Some(data) => {
            if let Some(filename) = data.strip_prefix("file:") {
                // Sidecar file: read raw bytes and base64-encode for Steel
                let path = std::path::Path::new(&kernel_dir).join(filename);
                match std::fs::read(&path) {
                    Ok(bytes) => {
                        use base64::Engine;
                        base64::engine::general_purpose::STANDARD.encode(&bytes)
                    }
                    Err(_) => String::new(),
                }
            } else {
                // Legacy: data is already base64
                data
            }
        }
    }
}

/// Like `json_get_first_image_with_dir` but returns raw bytes instead of base64.
/// Uses the new Steel ByteVector FFI return (Phase 3).
/// Returns empty vec if no image found.
pub fn json_get_first_image_bytes(json_str: String, kernel_dir: String) -> Vec<u8> {
    let parsed: Value = serde_json::from_str(&json_str).unwrap_or(Value::Null);
    match find_first_image_data(&parsed) {
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
    }
}

/// Extract multiple fields from a JSON string in one parse.
/// `keys_csv` is comma-separated field names. Returns tab-separated values.
/// Missing fields return empty strings. Non-string values are stringified.
pub fn json_get_many(json_str: String, keys_csv: String) -> String {
    let parsed = match serde_json::from_str::<Value>(&json_str) {
        Ok(v) => v,
        Err(_) => {
            let count = keys_csv.split(',').count();
            return "\t".repeat(count.saturating_sub(1));
        }
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
        .filter(|v| v.is_array())
        .map(|v| v.to_string())
        .unwrap_or_default()
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
        let result = json_get_first_image_with_dir(
            json.into(),
            dir.path().to_string_lossy().into_owned(),
        );
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
            if let Some(arr) = map.get("images").and_then(|i| i.as_array()) {
                if let Some(first) = arr.first() {
                    if let Some(data) = first.get("data").and_then(|d| d.as_str()) {
                        if !data.is_empty() {
                            return Some(data.to_string());
                        }
                    }
                }
            }
            // Jupyter-style mime types.
            for key in &["image/png", "image/jpeg", "image/gif"] {
                if let Some(s) = map.get(*key).and_then(|v| v.as_str()) {
                    if !s.is_empty() {
                        return Some(s.to_string());
                    }
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
