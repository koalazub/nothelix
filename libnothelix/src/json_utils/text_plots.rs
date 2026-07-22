use super::document;
use serde_json::Value;
use std::fmt;

const PLOT_SEP: char = '\u{1e}';
const SECTION_SEP: char = '\u{1d}';
const SPAN_SEP: char = '\u{1f}';

struct Span {
    row: i64,
    start: i64,
    end: i64,
    color: i64,
}

impl Span {
    fn read(value: &Value) -> Option<Self> {
        let fields = value.as_array()?;
        let [row, start, end, color] = fields.get(..4)? else {
            return None;
        };
        Some(Self {
            row: row.as_i64()?,
            start: start.as_i64()?,
            end: end.as_i64()?,
            color: color.as_i64()?,
        })
    }
}

impl fmt::Display for Span {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{},{},{},{}", self.row, self.start, self.end, self.color)
    }
}

struct Plot {
    rows: Vec<String>,
    spans: Vec<Span>,
}

impl Plot {
    fn read(value: &Value) -> Self {
        Self {
            rows: strings_at(value, "rows"),
            spans: value
                .get("spans")
                .and_then(Value::as_array)
                .map(|spans| spans.iter().filter_map(Span::read).collect())
                .unwrap_or_default(),
        }
    }
}

impl fmt::Display for Plot {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let spans: Vec<String> = self.spans.iter().map(Span::to_string).collect();
        write!(
            f,
            "{}{SECTION_SEP}{}",
            self.rows.join("\n"),
            spans.join(&SPAN_SEP.to_string())
        )
    }
}

fn strings_at(value: &Value, key: &str) -> Vec<String> {
    value
        .get(key)
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

pub fn json_get_text_plots(json_str: String) -> String {
    let Some(plots) = document(&json_str)
        .as_ref()
        .and_then(|doc| doc.get("text_plots"))
        .and_then(Value::as_array)
        .map(|plots| plots.iter().map(Plot::read).collect::<Vec<_>>())
    else {
        return String::new();
    };
    plots
        .iter()
        .map(Plot::to_string)
        .collect::<Vec<_>>()
        .join(&PLOT_SEP.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_plots_absent_returns_empty() {
        assert_eq!(json_get_text_plots(r#"{"stdout":"hi"}"#.to_string()), "");
    }

    #[test]
    fn text_plots_not_array_returns_empty() {
        assert_eq!(
            json_get_text_plots(r#"{"text_plots":"oops"}"#.to_string()),
            ""
        );
    }

    #[test]
    fn text_plots_empty_array_returns_empty() {
        assert_eq!(json_get_text_plots(r#"{"text_plots":[]}"#.to_string()), "");
    }

    #[test]
    fn text_plots_single_plot_encodes_rows_and_spans() {
        let json = r#"{"text_plots":[{"rows":["AB","CD"],"spans":[[0,0,1,2],[1,0,2,4]]}]}"#;
        let out = json_get_text_plots(json.to_string());
        assert_eq!(out, "AB\nCD\u{1d}0,0,1,2\u{1f}1,0,2,4");
    }

    #[test]
    fn text_plots_plot_with_no_spans_has_empty_spans_section() {
        let json = r#"{"text_plots":[{"rows":["A"],"spans":[]}]}"#;
        let out = json_get_text_plots(json.to_string());
        assert_eq!(out, "A\u{1d}");
    }

    #[test]
    fn text_plots_multi_plot_joins_with_plot_sep() {
        let json =
            r#"{"text_plots":[{"rows":["A"],"spans":[]},{"rows":["B"],"spans":[[0,0,1,3]]}]}"#;
        let out = json_get_text_plots(json.to_string());
        let parts: Vec<&str> = out.split('\u{1e}').collect();
        assert_eq!(parts, vec!["A\u{1d}", "B\u{1d}0,0,1,3"]);
    }

    #[test]
    fn text_plots_malformed_span_is_skipped() {
        let json = r#"{"text_plots":[{"rows":["A"],"spans":[[0,0,1],[0,0,1,2]]}]}"#;
        let out = json_get_text_plots(json.to_string());
        assert_eq!(out, "A\u{1d}0,0,1,2");
    }
}
