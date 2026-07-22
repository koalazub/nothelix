use serde_json::Value;

const PADDING_FRACTION: f64 = 0.05;
const DEGENERATE_SPAN: f64 = 1e-12;

pub(super) struct Series {
    pub(super) x: Vec<f64>,
    pub(super) y: Vec<f64>,
    pub(super) label: String,
}

impl Series {
    pub(super) fn parse_all(data: &Value) -> Vec<Self> {
        let Some(entries) = data.as_array() else {
            return Vec::new();
        };
        entries
            .iter()
            .filter_map(|entry| {
                let x = numbers(&entry["x"])?;
                let y = numbers(&entry["y"])?;
                if x.is_empty() || y.is_empty() {
                    return None;
                }
                Some(Self {
                    x,
                    y,
                    label: entry["label"].as_str().unwrap_or("").to_string(),
                })
            })
            .collect()
    }
}

fn numbers(value: &Value) -> Option<Vec<f64>> {
    Some(value.as_array()?.iter().filter_map(Value::as_f64).collect())
}

pub(super) struct Viewport {
    pub(super) x_min: f64,
    pub(super) x_max: f64,
    pub(super) y_min: f64,
    pub(super) y_max: f64,
}

impl Viewport {
    pub(super) fn x_range(&self) -> f64 {
        self.x_max - self.x_min
    }

    pub(super) fn y_range(&self) -> f64 {
        self.y_max - self.y_min
    }

    pub(super) fn enclosing(series: &[Series]) -> Self {
        let mut x = Span::empty();
        let mut y = Span::empty();
        for s in series {
            x.extend(&s.x);
            y.extend(&s.y);
        }
        let (x_min, x_max) = x.padded();
        let (y_min, y_max) = y.padded();
        Self {
            x_min,
            x_max,
            y_min,
            y_max,
        }
    }
}

struct Span {
    min: f64,
    max: f64,
}

impl Span {
    fn empty() -> Self {
        Self {
            min: f64::INFINITY,
            max: f64::NEG_INFINITY,
        }
    }

    fn extend(&mut self, values: &[f64]) {
        for &v in values.iter().filter(|v| v.is_finite()) {
            self.min = self.min.min(v);
            self.max = self.max.max(v);
        }
    }

    fn padded(&self) -> (f64, f64) {
        let (mut min, mut max) = (self.min, self.max);
        if (max - min).abs() < DEGENERATE_SPAN {
            min -= 1.0;
            max += 1.0;
        }
        let pad = (max - min) * PADDING_FRACTION;
        (min - pad, max + pad)
    }
}

pub(super) fn axis_label(value: f64) -> String {
    if value.abs() < 1e-12 {
        "0".to_string()
    } else if value.abs() >= 1e6 || value.abs() < 0.01 {
        format!("{value:.2e}")
    } else if value.fract().abs() < 1e-9 {
        format!("{value:.0}")
    } else {
        format!("{value:.2}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn simple_series() -> Value {
        json!([{
            "x": [0.0, 1.0, 2.0, 3.0, 4.0, 5.0],
            "y": [0.0, 1.0, 4.0, 9.0, 16.0, 25.0],
            "label": "y = x²"
        }])
    }

    #[test]
    fn parse_series_basic() {
        let series = Series::parse_all(&simple_series());
        assert_eq!(series.len(), 1);
        assert_eq!(series[0].x.len(), 6);
        assert_eq!(series[0].y.len(), 6);
        assert_eq!(series[0].label, "y = x²");
    }

    #[test]
    fn parse_series_empty_input() {
        assert!(Series::parse_all(&json!([])).is_empty());
    }

    #[test]
    fn parse_series_null_input() {
        assert!(Series::parse_all(&Value::Null).is_empty());
    }

    #[test]
    fn parse_series_skips_invalid() {
        let data = json!([
            {"x": [1.0], "y": [2.0], "label": "ok"},
            {"x": [], "y": [1.0]},
            {"x": [1.0], "y": []},
            {"not_x": [1.0]}
        ]);
        let series = Series::parse_all(&data);
        assert_eq!(series.len(), 1);
        assert_eq!(series[0].label, "ok");
    }

    #[test]
    fn auto_viewport_basic() {
        let viewport = Viewport::enclosing(&Series::parse_all(&simple_series()));
        assert!(viewport.x_min < 0.0);
        assert!(viewport.x_max > 5.0);
        assert!(viewport.y_min < 0.0);
        assert!(viewport.y_max > 25.0);
    }

    #[test]
    fn auto_viewport_single_point() {
        let data = json!([{"x": [5.0], "y": [10.0], "label": ""}]);
        let viewport = Viewport::enclosing(&Series::parse_all(&data));
        assert!(viewport.x_range() > 0.0);
        assert!(viewport.y_range() > 0.0);
    }

    #[test]
    fn axis_label_zero() {
        assert_eq!(axis_label(0.0), "0");
    }

    #[test]
    fn axis_label_integer() {
        assert_eq!(axis_label(42.0), "42");
    }

    #[test]
    fn axis_label_decimal() {
        assert_eq!(axis_label(2.75), "2.75");
    }

    #[test]
    fn axis_label_scientific() {
        assert_eq!(axis_label(1e7), "1.00e7");
    }
}
