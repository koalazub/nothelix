#![allow(clippy::needless_pass_by_value)]

mod braille;
mod plot;
mod series;

use serde_json::{Value, json};

use crate::error::{Error, Result};
use series::{Series, Viewport, axis_label};

const DEFAULT_COLS: i64 = 60;
const DEFAULT_ROWS: i64 = 15;

pub fn render_braille_chart(params_json: String) -> String {
    match chart_reply(&params_json) {
        Ok(reply) => reply,
        Err(failure) => json!({ "error": failure.to_string() }).to_string(),
    }
}

fn chart_reply(params_json: &str) -> Result<String> {
    let params: Value = serde_json::from_str(params_json).map_err(|source| Error::Json {
        subject: "braille chart parameters",
        source,
    })?;

    let series = Series::parse_all(&params["plot_data"]);
    if series.is_empty() {
        return Err(Error::Malformed {
            subject: "braille chart",
            detail: "no valid series data".to_string(),
        });
    }

    let auto = Viewport::enclosing(&series);
    let viewport = Viewport {
        x_min: params["x_min"].as_f64().unwrap_or(auto.x_min),
        x_max: params["x_max"].as_f64().unwrap_or(auto.x_max),
        y_min: params["y_min"].as_f64().unwrap_or(auto.y_min),
        y_max: params["y_max"].as_f64().unwrap_or(auto.y_max),
    };

    let cols = params["cols"].as_i64().unwrap_or(DEFAULT_COLS) as usize;
    let rows = params["rows"].as_i64().unwrap_or(DEFAULT_ROWS) as usize;
    let canvas = plot::draw(&series, &viewport, cols, rows);

    Ok(json!({
        "lines": canvas.lines(),
        "x_min": viewport.x_min,
        "x_max": viewport.x_max,
        "y_min": viewport.y_min,
        "y_max": viewport.y_max,
        "x_label_left": axis_label(viewport.x_min),
        "x_label_right": axis_label(viewport.x_max),
        "y_label_top": axis_label(viewport.y_max),
        "y_label_bottom": axis_label(viewport.y_min),
        "series_labels": series.iter().map(|s| s.label.clone()).collect::<Vec<_>>(),
        "error": ""
    })
    .to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reply(params: &Value) -> Value {
        serde_json::from_str(&render_braille_chart(params.to_string())).expect("valid JSON reply")
    }

    #[test]
    fn render_braille_chart_ffi_basic() {
        let result = reply(&json!({
            "plot_data": [{"x": [0.0, 1.0, 2.0], "y": [0.0, 1.0, 4.0], "label": "test"}],
            "cols": 30,
            "rows": 8
        }));
        assert_eq!(result["error"].as_str().expect("error field"), "");
        assert_eq!(result["lines"].as_array().expect("lines").len(), 8);
        assert_eq!(result["series_labels"][0].as_str().expect("label"), "test");
    }

    #[test]
    fn render_braille_chart_ffi_custom_viewport() {
        let result = reply(&json!({
            "plot_data": [{"x": [0.0, 5.0, 10.0], "y": [0.0, 25.0, 100.0], "label": ""}],
            "cols": 40,
            "rows": 12,
            "x_min": 0.0,
            "x_max": 10.0,
            "y_min": 0.0,
            "y_max": 100.0
        }));
        assert_eq!(result["error"].as_str().expect("error field"), "");
        assert_eq!(result["x_min"].as_f64().expect("x_min"), 0.0);
        assert_eq!(result["x_max"].as_f64().expect("x_max"), 10.0);
    }

    #[test]
    fn render_braille_chart_ffi_no_data() {
        let result = reply(&json!({"plot_data": []}));
        assert!(
            !result["error"].as_str().expect("error field").is_empty(),
            "empty plot data must report a failure"
        );
    }

    #[test]
    fn render_braille_chart_ffi_bad_json() {
        let result: Value = serde_json::from_str(&render_braille_chart("not json".into()))
            .expect("valid JSON reply");
        assert!(
            result["error"]
                .as_str()
                .expect("error field")
                .contains("invalid JSON"),
            "{result}"
        );
    }
}
