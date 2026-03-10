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
    find_first_image_b64(&parsed).unwrap_or_default()
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
}

/// Recursively search `v` for base64 image data.
///
/// Handles two formats:
/// - runner.jl: `{"images": [{"format": "png", "data": "..."}]}`
/// - Jupyter:   `{"image/png": "base64..."}`
fn find_first_image_b64(v: &Value) -> Option<String> {
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
                if let Some(img) = find_first_image_b64(val) {
                    return Some(img);
                }
            }
            None
        }
        Value::Array(arr) => arr.iter().find_map(find_first_image_b64),
        _ => None,
    }
}
