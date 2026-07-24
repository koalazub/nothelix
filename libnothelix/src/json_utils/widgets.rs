use super::document;
use crate::error::{Result, ffi};
use serde_json::Value;

pub fn json_get_widgets(json_str: String) -> String {
    ffi(widgets(&json_str))
}

fn widgets(json_str: &str) -> Result<String> {
    let doc = document("json-get-widgets", json_str)?;
    let Some(items) = doc.get("widgets").and_then(Value::as_array) else {
        return Ok(String::new());
    };
    Ok(items.iter().filter_map(line).collect::<Vec<_>>().join("\n"))
}

fn line(item: &Value) -> Option<String> {
    let kind = item.get("kind").and_then(Value::as_str)?;
    let name = item.get("name").and_then(Value::as_str)?;
    if kind.is_empty() || name.is_empty() {
        return None;
    }
    let params = item.get("params").and_then(Value::as_str).unwrap_or("");
    let current = item.get("current").and_then(Value::as_str).unwrap_or("");
    Some(format!("{kind}\t{name}\t{params}\t{current}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn widgets_absent_returns_empty() {
        assert_eq!(json_get_widgets(r#"{"stdout":"hi"}"#.to_string()), "");
    }

    #[test]
    fn widgets_not_array_returns_empty() {
        assert_eq!(json_get_widgets(r#"{"widgets":"oops"}"#.to_string()), "");
    }

    #[test]
    fn widgets_empty_array_returns_empty() {
        assert_eq!(json_get_widgets(r#"{"widgets":[]}"#.to_string()), "");
    }

    #[test]
    fn slider_spec_encodes_kind_name_params_current() {
        let json = r#"{"widgets":[{"kind":"slider","name":"freq","params":"220:880:10","current":"440"}]}"#;
        assert_eq!(
            json_get_widgets(json.to_string()),
            "slider\tfreq\t220:880:10\t440"
        );
    }

    #[test]
    fn choice_spec_encodes_pipe_options() {
        let json = r#"{"widgets":[{"kind":"choice","name":"wave","params":"sin|cos|tan","current":"sin"}]}"#;
        assert_eq!(
            json_get_widgets(json.to_string()),
            "choice\twave\tsin|cos|tan\tsin"
        );
    }

    #[test]
    fn multiple_specs_join_one_per_line() {
        let json = r#"{"widgets":[
            {"kind":"slider","name":"freq","params":"220:880:10","current":"440"},
            {"kind":"choice","name":"wave","params":"sin|cos","current":"cos"}
        ]}"#;
        assert_eq!(
            json_get_widgets(json.to_string()),
            "slider\tfreq\t220:880:10\t440\nchoice\twave\tsin|cos\tcos"
        );
    }

    #[test]
    fn spec_missing_current_defaults_to_empty_field() {
        let json = r#"{"widgets":[{"kind":"slider","name":"g","params":"0:1:0"}]}"#;
        assert_eq!(json_get_widgets(json.to_string()), "slider\tg\t0:1:0\t");
    }

    #[test]
    fn spec_without_a_kind_or_name_is_skipped() {
        let json = r#"{"widgets":[{"name":"x","params":"0:1:0"},{"kind":"slider","name":"y","params":"0:9:1","current":"3"}]}"#;
        assert_eq!(json_get_widgets(json.to_string()), "slider\ty\t0:9:1\t3");
    }

    #[test]
    fn reports_a_malformed_document_instead_of_an_empty_blob() {
        let result = json_get_widgets("not json".to_string());
        assert!(
            result.starts_with("ERROR: json-get-widgets: invalid JSON: "),
            "{result}"
        );
    }
}
