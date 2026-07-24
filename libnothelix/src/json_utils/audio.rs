use super::document;
use crate::error::{Result, ffi};
use serde_json::Value;

pub fn json_get_audio(json_str: String) -> String {
    ffi(audio(&json_str))
}

fn audio(json_str: &str) -> Result<String> {
    let doc = document("json-get-audio", json_str)?;
    let Some(items) = doc.get("audio").and_then(Value::as_array) else {
        return Ok(String::new());
    };
    Ok(items.iter().filter_map(line).collect::<Vec<_>>().join("\n"))
}

fn line(item: &Value) -> Option<String> {
    let path = item.get("path").and_then(Value::as_str)?;
    if path.is_empty() {
        return None;
    }
    let duration_ms = item.get("duration_ms").and_then(Value::as_i64).unwrap_or(0);
    Some(format!("{path}\t{duration_ms}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn audio_absent_returns_empty() {
        assert_eq!(json_get_audio(r#"{"stdout":"hi"}"#.to_string()), "");
    }

    #[test]
    fn audio_not_array_returns_empty() {
        assert_eq!(json_get_audio(r#"{"audio":"oops"}"#.to_string()), "");
    }

    #[test]
    fn audio_empty_array_returns_empty() {
        assert_eq!(json_get_audio(r#"{"audio":[]}"#.to_string()), "");
    }

    #[test]
    fn audio_single_artifact_encodes_path_and_duration() {
        let json = r#"{"audio":[{"path":"/k/audio/cell_0.wav","duration_ms":1500}]}"#;
        assert_eq!(
            json_get_audio(json.to_string()),
            "/k/audio/cell_0.wav\t1500"
        );
    }

    #[test]
    fn audio_multiple_artifacts_join_one_per_line() {
        let json = r#"{"audio":[
            {"path":"/k/audio/cell_0.wav","duration_ms":1500},
            {"path":"/k/audio/cell_0_1.wav","duration_ms":800}
        ]}"#;
        assert_eq!(
            json_get_audio(json.to_string()),
            "/k/audio/cell_0.wav\t1500\n/k/audio/cell_0_1.wav\t800"
        );
    }

    #[test]
    fn audio_missing_duration_defaults_to_zero() {
        let json = r#"{"audio":[{"path":"/k/x.wav","reason":"unparseable"}]}"#;
        assert_eq!(json_get_audio(json.to_string()), "/k/x.wav\t0");
    }

    #[test]
    fn audio_artifact_without_a_path_is_skipped() {
        let json = r#"{"audio":[{"duration_ms":10},{"path":"/k/y.wav","duration_ms":20}]}"#;
        assert_eq!(json_get_audio(json.to_string()), "/k/y.wav\t20");
    }

    #[test]
    fn audio_reports_a_malformed_document_instead_of_an_empty_blob() {
        let result = json_get_audio("not json".to_string());
        assert!(
            result.starts_with("ERROR: json-get-audio: invalid JSON: "),
            "{result}"
        );
    }
}
