use super::document;
use crate::error::{Result, ffi};
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
    ffi(field(&json_str, &key))
}

fn field(json_str: &str, key: &str) -> Result<String> {
    let doc = document("json-get", json_str)?;
    Ok(doc.get(key).map_or_else(String::new, as_text))
}

pub fn json_get_bool(json_str: String, key: String) -> String {
    ffi(flag(&json_str, &key))
}

fn flag(json_str: &str, key: &str) -> Result<String> {
    let doc = document("json-get-bool", json_str)?;
    Ok(doc.get(key).map_or_else(|| "false".to_string(), as_flag))
}

pub fn json_get_many(json_str: String, keys_csv: String) -> String {
    ffi(tab_separated_fields(&json_str, &keys_csv))
}

fn tab_separated_fields(json_str: &str, keys_csv: &str) -> Result<String> {
    let doc = document("json-get-many", json_str)?;
    Ok(keys_csv
        .split(',')
        .map(|key| {
            doc.get(key.trim())
                .map_or_else(String::new, as_text_unless_null)
        })
        .collect::<Vec<_>>()
        .join("\t"))
}

pub fn json_get_notes(json_str: String) -> String {
    ffi(notes(&json_str))
}

fn notes(json_str: &str) -> Result<String> {
    let doc = document("json-get-notes", json_str)?;
    Ok(doc
        .get("notes")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_str)
                .collect::<Vec<_>>()
                .join("\n")
        })
        .unwrap_or_default())
}

pub fn json_get_cell_states(json_str: String) -> String {
    ffi(cell_states(&json_str))
}

fn cell_states(json_str: &str) -> Result<String> {
    let doc = document("json-get-cell-states", json_str)?;
    let Some(obj) = doc.get("cell_states").and_then(Value::as_object) else {
        return Ok(String::new());
    };
    let mut rows: Vec<(i64, String)> = obj
        .iter()
        .map(|(idx, entry)| {
            let state = entry
                .get("state")
                .and_then(Value::as_str)
                .unwrap_or("fresh");
            let inputs = entry
                .get("inputs")
                .and_then(Value::as_array)
                .map(|arr| {
                    arr.iter()
                        .filter_map(|inp| {
                            let name = inp.get("name").and_then(Value::as_str)?;
                            let writer = inp.get("writer").and_then(Value::as_i64)?;
                            let rel = inp.get("rel").and_then(Value::as_str)?;
                            Some(format!("{name},{writer},{rel}"))
                        })
                        .collect::<Vec<_>>()
                        .join(";")
                })
                .unwrap_or_default();
            let duration = entry
                .get("duration")
                .and_then(Value::as_i64)
                .map_or_else(String::new, |ms| ms.to_string());
            let key = idx.parse::<i64>().unwrap_or(i64::MAX);
            (key, format!("{idx}\t{state}\t{inputs}\t{duration}"))
        })
        .collect();
    rows.sort_by_key(|(key, _)| *key);
    Ok(rows
        .into_iter()
        .map(|(_, row)| row)
        .collect::<Vec<_>>()
        .join("\n"))
}

pub fn json_get_plot_data(json_str: String) -> String {
    ffi(plot_data(&json_str))
}

fn plot_data(json_str: &str) -> Result<String> {
    let doc = document("json-get-plot-data", json_str)?;
    Ok(doc
        .get("plot_data")
        .filter(|plot_data| plot_data.is_array())
        .map_or_else(String::new, Value::to_string))
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
    fn a_malformed_document_is_reported_not_confused_with_an_absent_field() {
        let absent = json_get(r#"{"name": "hello"}"#.into(), "key".into());
        let malformed = json_get("not json".into(), "key".into());
        assert_eq!(absent, "");
        assert!(
            malformed.starts_with("ERROR: json-get: invalid JSON: "),
            "{malformed}"
        );
    }

    #[test]
    fn a_malformed_document_is_reported_by_the_bool_accessor_too() {
        let absent = json_get_bool(r#"{"other": 1}"#.into(), "flag".into());
        let malformed = json_get_bool("not json".into(), "flag".into());
        assert_eq!(absent, "false");
        assert!(
            malformed.starts_with("ERROR: json-get-bool: invalid JSON: "),
            "{malformed}"
        );
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
    fn json_get_many_reports_a_malformed_document_instead_of_blank_fields() {
        let result = json_get_many("not json".into(), "a,b".into());
        assert!(
            result.starts_with("ERROR: json-get-many: invalid JSON: "),
            "{result}"
        );
    }

    #[test]
    fn plot_data_is_empty_unless_an_array_is_present() {
        assert_eq!(json_get_plot_data(r#"{"plot_data": 3}"#.into()), "");
        assert_eq!(json_get_plot_data(r#"{"plot_data": [1]}"#.into()), "[1]");
    }

    #[test]
    fn plot_data_reports_a_malformed_document() {
        let result = json_get_plot_data("not json".into());
        assert!(
            result.starts_with("ERROR: json-get-plot-data: invalid JSON: "),
            "{result}"
        );
    }

    #[test]
    fn notes_absent_returns_empty() {
        assert_eq!(json_get_notes(r#"{"stdout": "hi"}"#.into()), "");
    }

    #[test]
    fn notes_array_joins_with_newlines() {
        let json = r#"{"notes": ["note: A below", "note: B stale"]}"#;
        assert_eq!(json_get_notes(json.into()), "note: A below\nnote: B stale");
    }

    #[test]
    fn notes_not_an_array_returns_empty() {
        assert_eq!(json_get_notes(r#"{"notes": "oops"}"#.into()), "");
    }

    #[test]
    fn notes_empty_array_returns_empty() {
        assert_eq!(json_get_notes(r#"{"notes": []}"#.into()), "");
    }

    #[test]
    fn notes_reports_a_malformed_document_instead_of_an_empty_blob() {
        let result = json_get_notes("not json".into());
        assert!(
            result.starts_with("ERROR: json-get-notes: invalid JSON: "),
            "{result}"
        );
    }

    #[test]
    fn cell_states_absent_returns_empty() {
        assert_eq!(json_get_cell_states(r#"{"stdout": "hi"}"#.into()), "");
    }

    #[test]
    fn cell_states_emit_one_sorted_line_per_cell() {
        let json = r#"{"cell_states": {
            "3": {"state": "out-of-order", "inputs": [{"name": "A", "writer": 5, "rel": "below"}], "duration": 1400},
            "0": {"state": "fresh", "inputs": [], "duration": 12}
        }}"#;
        assert_eq!(
            json_get_cell_states(json.into()),
            "0\tfresh\t\t12\n3\tout-of-order\tA,5,below\t1400"
        );
    }

    #[test]
    fn cell_states_leave_a_never_run_duration_blank() {
        let json = r#"{"cell_states": {
            "0": {"state": "fresh", "inputs": []},
            "1": {"state": "fresh", "inputs": [], "duration": null}
        }}"#;
        assert_eq!(
            json_get_cell_states(json.into()),
            "0\tfresh\t\t\n1\tfresh\t\t"
        );
    }

    #[test]
    fn cell_states_join_multiple_inputs_with_semicolons() {
        let json = r#"{"cell_states": {
            "7": {"state": "stale-input", "duration": 34, "inputs": [
                {"name": "A", "writer": 2, "rel": "stale"},
                {"name": "B", "writer": 4, "rel": "fresh"}
            ]}
        }}"#;
        assert_eq!(
            json_get_cell_states(json.into()),
            "7\tstale-input\tA,2,stale;B,4,fresh\t34"
        );
    }

    #[test]
    fn cell_states_not_an_object_returns_empty() {
        assert_eq!(json_get_cell_states(r#"{"cell_states": []}"#.into()), "");
    }

    #[test]
    fn cell_states_reports_a_malformed_document() {
        let result = json_get_cell_states("not json".into());
        assert!(
            result.starts_with("ERROR: json-get-cell-states: invalid JSON: "),
            "{result}"
        );
    }
}
