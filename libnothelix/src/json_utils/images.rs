use super::document;
use crate::error::{Result, ffi};
use abi_stable::std_types::{RString, RVec};
use base64::Engine as _;
use serde_json::Value;
use std::path::Path;
use steel::steel_vm::ffi::FFIValue;

const ANIMATED_MIMES: [&str; 6] = [
    "image/gif",
    "image/apng",
    "image/webp",
    "video/mp4",
    "video/webm",
    "application/json+lottie",
];

const STATIC_MIMES: [&str; 2] = ["image/png", "image/jpeg"];

enum Payload<'a> {
    Sidecar(&'a str),
    Inline(&'a str),
}

impl<'a> Payload<'a> {
    fn of(data: &'a str) -> Option<Self> {
        if data.is_empty() {
            return None;
        }
        Some(match data.strip_prefix("file:") {
            Some(file_name) => Self::Sidecar(file_name),
            None => Self::Inline(data),
        })
    }

    fn base64(&self, kernel_dir: &str) -> Option<String> {
        match self {
            Self::Sidecar(file_name) => std::fs::read(Path::new(kernel_dir).join(file_name))
                .ok()
                .map(|bytes| base64::engine::general_purpose::STANDARD.encode(&bytes)),
            Self::Inline(data) => Some((*data).to_string()),
        }
    }

    fn bytes(&self, kernel_dir: &str) -> Option<Vec<u8>> {
        match self {
            Self::Sidecar(file_name) => std::fs::read(Path::new(kernel_dir).join(file_name)).ok(),
            Self::Inline(data) => base64::engine::general_purpose::STANDARD
                .decode(data.trim())
                .ok(),
        }
    }
}

fn first_animated_mime(value: &Value) -> Option<String> {
    match value {
        Value::Object(map) => ANIMATED_MIMES
            .iter()
            .find(|mime| {
                map.get(**mime)
                    .and_then(Value::as_str)
                    .is_some_and(|data| !data.is_empty())
            })
            .map(|mime| (*mime).to_string())
            .or_else(|| map.values().find_map(first_animated_mime)),
        Value::Array(items) => items.iter().find_map(first_animated_mime),
        _ => None,
    }
}

fn first_image_data(value: &Value) -> Option<String> {
    match value {
        Value::Object(map) => {
            if let Some(data) = map
                .get("images")
                .and_then(Value::as_array)
                .and_then(|images| images.first())
                .and_then(|image| image.get("data"))
                .and_then(Value::as_str)
                .filter(|data| !data.is_empty())
            {
                return Some(data.to_string());
            }
            ANIMATED_MIMES
                .iter()
                .chain(STATIC_MIMES.iter())
                .find_map(|mime| {
                    map.get(*mime)
                        .and_then(Value::as_str)
                        .filter(|data| !data.is_empty())
                        .map(str::to_string)
                })
                .or_else(|| map.values().find_map(first_image_data))
        }
        Value::Array(items) => items.iter().find_map(first_image_data),
        _ => None,
    }
}

fn every_image_base64(value: &Value, kernel_dir: &str) -> Vec<String> {
    let listed: Vec<String> = value
        .get("images")
        .and_then(Value::as_array)
        .map(|images| {
            images
                .iter()
                .filter_map(|image| {
                    let data = image.get("data").and_then(Value::as_str)?;
                    Payload::of(data)?.base64(kernel_dir)
                })
                .collect()
        })
        .unwrap_or_default();
    if !listed.is_empty() {
        return listed;
    }
    first_image_data(value)
        .and_then(|data| Payload::of(&data)?.base64(kernel_dir))
        .into_iter()
        .collect()
}

pub fn json_get_animated_mime(json_str: String) -> String {
    ffi(animated_mime(&json_str))
}

fn animated_mime(json_str: &str) -> Result<String> {
    let doc = document("json-get-animated-mime", json_str)?;
    Ok(first_animated_mime(&doc).unwrap_or_default())
}

pub fn json_get_first_image_bytes(json_str: String, kernel_dir: String) -> FFIValue {
    match first_image_bytes(&json_str, &kernel_dir) {
        Ok(bytes) => FFIValue::ByteVector(RVec::from(bytes)),
        Err(failure) => FFIValue::StringV(RString::from(ffi(Err(failure)))),
    }
}

fn first_image_bytes(json_str: &str, kernel_dir: &str) -> Result<Vec<u8>> {
    let doc = document("json-get-first-image-bytes", json_str)?;
    Ok(first_image_data(&doc)
        .and_then(|data| Payload::of(&data)?.bytes(kernel_dir))
        .unwrap_or_default())
}

pub fn json_get_all_images(json_str: String, kernel_dir: String) -> String {
    ffi(all_images(&json_str, &kernel_dir))
}

fn all_images(json_str: &str, kernel_dir: &str) -> Result<String> {
    let doc = document("json-get-all-images", json_str)?;
    Ok(every_image_base64(&doc, kernel_dir).join("\n"))
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn all_images_falls_back_to_mime_bundle_when_no_images_array() {
        let json = r#"{"image/png": "MIMEBASE64"}"#;
        let out = json_get_all_images(json.to_string(), String::new());
        assert_eq!(out, "MIMEBASE64");
    }

    #[test]
    fn all_images_fallback_picks_animated_before_static() {
        let json = r#"{"data": {"image/gif": "GIFBASE64", "image/png": "PNGBASE64"}}"#;
        let out = json_get_all_images(json.to_string(), String::new());
        assert_eq!(out, "GIFBASE64");
    }

    #[test]
    fn all_images_fallback_resolves_sidecar() {
        let dir = tempfile::tempdir().unwrap();
        let png_path = dir.path().join("image_1.png");
        std::fs::write(&png_path, b"\x89PNG fake data").unwrap();

        let json = r#"{"image/png": "file:image_1.png"}"#;
        let out = json_get_all_images(json.to_string(), dir.path().to_string_lossy().into_owned());
        assert!(!out.is_empty());
        let decoded = base64::engine::general_purpose::STANDARD
            .decode(&out)
            .unwrap();
        assert_eq!(decoded, b"\x89PNG fake data");
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
    fn a_malformed_document_is_reported_by_every_image_accessor() {
        let all = json_get_all_images("not json".into(), String::new());
        let mime = json_get_animated_mime("not json".into());
        assert!(
            all.starts_with("ERROR: json-get-all-images: invalid JSON: "),
            "{all}"
        );
        assert!(
            mime.starts_with("ERROR: json-get-animated-mime: invalid JSON: "),
            "{mime}"
        );
    }

    #[test]
    fn first_image_bytes_reports_a_malformed_document_instead_of_empty_bytes() {
        let absent = json_get_first_image_bytes(r#"{"stdout":"hi"}"#.into(), String::new());
        let malformed = json_get_first_image_bytes("not json".into(), String::new());
        assert!(matches!(absent, FFIValue::ByteVector(bytes) if bytes.is_empty()));
        let FFIValue::StringV(reported) = malformed else {
            panic!("a malformed document must not be reported as empty bytes");
        };
        assert!(
            reported.starts_with("ERROR: json-get-first-image-bytes: invalid JSON: "),
            "{reported}"
        );
    }
}
