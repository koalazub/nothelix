use super::document;
use serde_json::Value;

fn as_text(value: &Value) -> String {
    match value {
        Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

fn as_text_unless_null(value: &Value) -> String {
    if value.is_null() {
        String::new()
    } else {
        as_text(value)
    }
}

fn as_flag(value: &Value) -> String {
    match value {
        Value::Bool(b) => b.to_string(),
        Value::String(s) => s.clone(),
        _ => "false".to_string(),
    }
}

pub fn json_get(json_str: String, key: String) -> String {
    document(&json_str)
        .and_then(|doc| doc.get(&key).map(as_text))
        .unwrap_or_default()
}

pub fn json_get_bool(json_str: String, key: String) -> String {
    document(&json_str)
        .and_then(|doc| doc.get(&key).map(as_flag))
        .unwrap_or_else(|| "false".to_string())
}

pub fn json_get_many(json_str: String, keys_csv: String) -> String {
    let keys = keys_csv.split(',');
    let Some(doc) = document(&json_str) else {
        return "\t".repeat(keys.count().saturating_sub(1));
    };
    keys.map(|key| {
        doc.get(key.trim())
            .map_or_else(String::new, as_text_unless_null)
    })
    .collect::<Vec<_>>()
    .join("\t")
}

pub fn json_get_plot_data(json_str: String) -> String {
    document(&json_str)
        .and_then(|doc| doc.get("plot_data").cloned())
        .filter(Value::is_array)
        .map(|plot_data| plot_data.to_string())
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
    fn plot_data_is_empty_unless_an_array_is_present() {
        assert_eq!(json_get_plot_data(r#"{"plot_data": 3}"#.into()), "");
        assert_eq!(json_get_plot_data(r#"{"plot_data": [1]}"#.into()), "[1]");
    }
}
