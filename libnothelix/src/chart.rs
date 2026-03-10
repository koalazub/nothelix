//! Braille chart renderer.
//!
//! Converts x/y data series into Unicode braille characters (U+2800–U+28FF)
//! for text-based plotting in a terminal.  Each braille character encodes a
//! 2-wide × 4-tall grid of dots, giving twice the horizontal resolution and
//! four times the vertical resolution of a single character cell.
//!
//! The renderer is stateless: the Scheme component calls it repeatedly with
//! different viewport parameters to implement zoom and pan.

use serde_json::{json, Value};

// ─── Braille encoding ─────────────────────────────────────────────────────────

/// Braille dot offsets within one character cell.
///
/// The Unicode braille pattern maps 8 dots to bits as:
///   col 0   col 1
///   bit 0   bit 3    row 0
///   bit 1   bit 4    row 1
///   bit 2   bit 5    row 2
///   bit 6   bit 7    row 3
const BRAILLE_DOTS: [[u8; 2]; 4] = [[0x01, 0x08], [0x02, 0x10], [0x04, 0x20], [0x40, 0x80]];

const BRAILLE_BLANK: u32 = 0x2800;

/// A 2-D grid of braille dots backed by one `u8` per character cell.
struct BrailleCanvas {
    /// Width in character columns.
    cols: usize,
    /// Height in character rows.
    rows: usize,
    /// One byte per cell, holding the braille dot bitmask.
    cells: Vec<u8>,
}

impl BrailleCanvas {
    fn new(cols: usize, rows: usize) -> Self {
        Self {
            cols,
            rows,
            cells: vec![0u8; cols * rows],
        }
    }

    /// Set a single dot at pixel coordinates.
    /// `px` ranges over `0 .. cols*2`, `py` ranges over `0 .. rows*4`.
    fn set(&mut self, px: usize, py: usize) {
        let col = px / 2;
        let row = py / 4;
        if col >= self.cols || row >= self.rows {
            return;
        }
        let dx = px % 2;
        let dy = py % 4;
        self.cells[row * self.cols + col] |= BRAILLE_DOTS[dy][dx];
    }

    /// Render the canvas to a vector of strings, one per row (top to bottom).
    fn render(&self) -> Vec<String> {
        (0..self.rows)
            .map(|row| {
                let start = row * self.cols;
                (start..start + self.cols)
                    .map(|i| char::from_u32(BRAILLE_BLANK + self.cells[i] as u32).unwrap_or('?'))
                    .collect()
            })
            .collect()
    }
}

// ─── Axis labels ──────────────────────────────────────────────────────────────

/// Format a number for axis labels, keeping it compact.
fn fmt_num(v: f64) -> String {
    if v.abs() < 1e-12 {
        "0".into()
    } else if v.abs() >= 1e6 || (v.abs() < 0.01 && v.abs() > 0.0) {
        format!("{:.2e}", v)
    } else if v.fract().abs() < 1e-9 {
        format!("{:.0}", v)
    } else {
        format!("{:.2}", v)
    }
}

// ─── Data types ───────────────────────────────────────────────────────────────

struct Series {
    x: Vec<f64>,
    y: Vec<f64>,
    label: String,
}

struct Viewport {
    x_min: f64,
    x_max: f64,
    y_min: f64,
    y_max: f64,
}

impl Viewport {
    fn x_range(&self) -> f64 {
        self.x_max - self.x_min
    }
    fn y_range(&self) -> f64 {
        self.y_max - self.y_min
    }
}

// ─── Parsing helpers ──────────────────────────────────────────────────────────

fn parse_series(data: &Value) -> Vec<Series> {
    let arr = match data.as_array() {
        Some(a) => a,
        None => return Vec::new(),
    };

    arr.iter()
        .filter_map(|s| {
            let x: Vec<f64> = s["x"]
                .as_array()?
                .iter()
                .filter_map(|v| v.as_f64())
                .collect();
            let y: Vec<f64> = s["y"]
                .as_array()?
                .iter()
                .filter_map(|v| v.as_f64())
                .collect();
            if x.is_empty() || y.is_empty() {
                return None;
            }
            let label = s["label"].as_str().unwrap_or("").to_string();
            Some(Series { x, y, label })
        })
        .collect()
}

fn auto_viewport(series: &[Series]) -> Viewport {
    let mut x_min = f64::INFINITY;
    let mut x_max = f64::NEG_INFINITY;
    let mut y_min = f64::INFINITY;
    let mut y_max = f64::NEG_INFINITY;

    for s in series {
        for &v in &s.x {
            if v.is_finite() {
                x_min = x_min.min(v);
                x_max = x_max.max(v);
            }
        }
        for &v in &s.y {
            if v.is_finite() {
                y_min = y_min.min(v);
                y_max = y_max.max(v);
            }
        }
    }

    // Ensure non-degenerate ranges.
    if (x_max - x_min).abs() < 1e-12 {
        x_min -= 1.0;
        x_max += 1.0;
    }
    if (y_max - y_min).abs() < 1e-12 {
        y_min -= 1.0;
        y_max += 1.0;
    }

    // Add 5 % padding so points don't sit on the edges.
    let x_pad = (x_max - x_min) * 0.05;
    let y_pad = (y_max - y_min) * 0.05;

    Viewport {
        x_min: x_min - x_pad,
        x_max: x_max + x_pad,
        y_min: y_min - y_pad,
        y_max: y_max + y_pad,
    }
}

// ─── Core renderer ────────────────────────────────────────────────────────────

/// Render series data onto a braille canvas.
///
/// Consecutive points in each series are connected with a line so that
/// continuous functions look solid rather than scattered.
fn render_to_canvas(series: &[Series], vp: &Viewport, cols: usize, rows: usize) -> BrailleCanvas {
    let mut canvas = BrailleCanvas::new(cols, rows);
    let dot_w = cols * 2;
    let dot_h = rows * 4;

    let to_px = |x: f64| -> isize {
        ((x - vp.x_min) / vp.x_range() * (dot_w as f64 - 1.0)).round() as isize
    };
    let to_py = |y: f64| -> isize {
        // Invert y: top of canvas = high y values.
        ((vp.y_max - y) / vp.y_range() * (dot_h as f64 - 1.0)).round() as isize
    };

    for s in series {
        let n = s.x.len().min(s.y.len());
        let mut prev: Option<(isize, isize)> = None;

        for i in 0..n {
            let (x, y) = (s.x[i], s.y[i]);
            if !x.is_finite() || !y.is_finite() {
                prev = None;
                continue;
            }

            let px = to_px(x);
            let py = to_py(y);

            // Draw a line from the previous point to this one (Bresenham).
            if let Some((px0, py0)) = prev {
                bresenham(&mut canvas, px0, py0, px, py, dot_w, dot_h);
            } else {
                // Single point.
                if px >= 0 && (px as usize) < dot_w && py >= 0 && (py as usize) < dot_h {
                    canvas.set(px as usize, py as usize);
                }
            }

            prev = Some((px, py));
        }
    }

    canvas
}

/// Bresenham line drawing, clipping to the canvas bounds.
fn bresenham(
    canvas: &mut BrailleCanvas,
    x0: isize,
    y0: isize,
    x1: isize,
    y1: isize,
    w: usize,
    h: usize,
) {
    let dx = (x1 - x0).abs();
    let dy = -(y1 - y0).abs();
    let sx: isize = if x0 < x1 { 1 } else { -1 };
    let sy: isize = if y0 < y1 { 1 } else { -1 };
    let mut err = dx + dy;
    let mut cx = x0;
    let mut cy = y0;

    loop {
        if cx >= 0 && (cx as usize) < w && cy >= 0 && (cy as usize) < h {
            canvas.set(cx as usize, cy as usize);
        }
        if cx == x1 && cy == y1 {
            break;
        }
        let e2 = 2 * err;
        if e2 >= dy {
            if cx == x1 {
                break;
            }
            err += dy;
            cx += sx;
        }
        if e2 <= dx {
            if cy == y1 {
                break;
            }
            err += dx;
            cy += sy;
        }
    }
}

// ─── FFI-facing function ──────────────────────────────────────────────────────

/// Render a braille chart from JSON plot data.
///
/// # Arguments (passed as a single JSON string)
///
/// ```json
/// {
///   "plot_data": [ { "x": [...], "y": [...], "label": "..." }, ... ],
///   "cols": 60,
///   "rows": 15,
///   "x_min": null,  // null = auto
///   "x_max": null,
///   "y_min": null,
///   "y_max": null
/// }
/// ```
///
/// # Returns (JSON string)
///
/// ```json
/// {
///   "lines": ["⠀⠀⣀⡤⠤⠒⠒⠉...", ...],
///   "x_min": 0.0,
///   "x_max": 10.0,
///   "y_min": 0.0,
///   "y_max": 100.0,
///   "x_label_left": "0",
///   "x_label_right": "10",
///   "y_label_top": "100",
///   "y_label_bottom": "0",
///   "series_labels": ["y = x²"],
///   "error": ""
/// }
/// ```
pub fn render_braille_chart(params_json: String) -> String {
    let params: Value = match serde_json::from_str(&params_json) {
        Ok(v) => v,
        Err(e) => return json!({"error": format!("Invalid JSON: {e}")}).to_string(),
    };

    let series = parse_series(&params["plot_data"]);
    if series.is_empty() {
        return json!({"error": "No valid series data"}).to_string();
    }

    let cols = params["cols"].as_i64().unwrap_or(60) as usize;
    let rows = params["rows"].as_i64().unwrap_or(15) as usize;

    let auto_vp = auto_viewport(&series);

    let vp = Viewport {
        x_min: params["x_min"].as_f64().unwrap_or(auto_vp.x_min),
        x_max: params["x_max"].as_f64().unwrap_or(auto_vp.x_max),
        y_min: params["y_min"].as_f64().unwrap_or(auto_vp.y_min),
        y_max: params["y_max"].as_f64().unwrap_or(auto_vp.y_max),
    };

    let canvas = render_to_canvas(&series, &vp, cols, rows);
    let lines = canvas.render();

    let labels: Vec<String> = series.iter().map(|s| s.label.clone()).collect();

    json!({
        "lines": lines,
        "x_min": vp.x_min,
        "x_max": vp.x_max,
        "y_min": vp.y_min,
        "y_max": vp.y_max,
        "x_label_left": fmt_num(vp.x_min),
        "x_label_right": fmt_num(vp.x_max),
        "y_label_top": fmt_num(vp.y_max),
        "y_label_bottom": fmt_num(vp.y_min),
        "series_labels": labels,
        "error": ""
    })
    .to_string()
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn simple_series() -> Value {
        json!([{
            "x": [0.0, 1.0, 2.0, 3.0, 4.0, 5.0],
            "y": [0.0, 1.0, 4.0, 9.0, 16.0, 25.0],
            "label": "y = x²"
        }])
    }

    #[test]
    fn parse_series_basic() {
        let data = simple_series();
        let series = parse_series(&data);
        assert_eq!(series.len(), 1);
        assert_eq!(series[0].x.len(), 6);
        assert_eq!(series[0].y.len(), 6);
        assert_eq!(series[0].label, "y = x²");
    }

    #[test]
    fn parse_series_empty_input() {
        let data = json!([]);
        assert!(parse_series(&data).is_empty());
    }

    #[test]
    fn parse_series_null_input() {
        let data = Value::Null;
        assert!(parse_series(&data).is_empty());
    }

    #[test]
    fn parse_series_skips_invalid() {
        let data = json!([
            {"x": [1.0], "y": [2.0], "label": "ok"},
            {"x": [], "y": [1.0]},
            {"x": [1.0], "y": []},
            {"not_x": [1.0]}
        ]);
        let series = parse_series(&data);
        assert_eq!(series.len(), 1);
        assert_eq!(series[0].label, "ok");
    }

    #[test]
    fn auto_viewport_basic() {
        let data = simple_series();
        let series = parse_series(&data);
        let vp = auto_viewport(&series);
        assert!(vp.x_min < 0.0); // padding
        assert!(vp.x_max > 5.0);
        assert!(vp.y_min < 0.0);
        assert!(vp.y_max > 25.0);
    }

    #[test]
    fn auto_viewport_single_point() {
        let data = json!([{"x": [5.0], "y": [10.0], "label": ""}]);
        let series = parse_series(&data);
        let vp = auto_viewport(&series);
        assert!(vp.x_range() > 0.0);
        assert!(vp.y_range() > 0.0);
    }

    #[test]
    fn render_produces_correct_dimensions() {
        let data = simple_series();
        let series = parse_series(&data);
        let vp = auto_viewport(&series);
        let canvas = render_to_canvas(&series, &vp, 40, 10);
        let lines = canvas.render();
        assert_eq!(lines.len(), 10);
        for line in &lines {
            assert_eq!(line.chars().count(), 40);
        }
    }

    #[test]
    fn render_not_all_blank() {
        let data = simple_series();
        let series = parse_series(&data);
        let vp = auto_viewport(&series);
        let canvas = render_to_canvas(&series, &vp, 40, 10);
        let lines = canvas.render();
        let blank_char = char::from_u32(BRAILLE_BLANK).unwrap();
        let has_dots = lines.iter().any(|l| l.chars().any(|c| c != blank_char));
        assert!(has_dots, "Canvas should contain at least some dots");
    }

    #[test]
    fn render_braille_chart_ffi_basic() {
        let params = json!({
            "plot_data": [{"x": [0.0, 1.0, 2.0], "y": [0.0, 1.0, 4.0], "label": "test"}],
            "cols": 30,
            "rows": 8
        });
        let result_json = render_braille_chart(params.to_string());
        let result: Value = serde_json::from_str(&result_json).unwrap();
        assert_eq!(result["error"].as_str().unwrap(), "");
        let lines = result["lines"].as_array().unwrap();
        assert_eq!(lines.len(), 8);
        assert_eq!(result["series_labels"][0].as_str().unwrap(), "test");
    }

    #[test]
    fn render_braille_chart_ffi_custom_viewport() {
        let params = json!({
            "plot_data": [{"x": [0.0, 5.0, 10.0], "y": [0.0, 25.0, 100.0], "label": ""}],
            "cols": 40,
            "rows": 12,
            "x_min": 0.0,
            "x_max": 10.0,
            "y_min": 0.0,
            "y_max": 100.0
        });
        let result_json = render_braille_chart(params.to_string());
        let result: Value = serde_json::from_str(&result_json).unwrap();
        assert_eq!(result["error"].as_str().unwrap(), "");
        assert_eq!(result["x_min"].as_f64().unwrap(), 0.0);
        assert_eq!(result["x_max"].as_f64().unwrap(), 10.0);
    }

    #[test]
    fn render_braille_chart_ffi_no_data() {
        let params = json!({"plot_data": []});
        let result_json = render_braille_chart(params.to_string());
        let result: Value = serde_json::from_str(&result_json).unwrap();
        assert!(result["error"].as_str().unwrap().len() > 0);
    }

    #[test]
    fn render_braille_chart_ffi_bad_json() {
        let result_json = render_braille_chart("not json".into());
        let result: Value = serde_json::from_str(&result_json).unwrap();
        assert!(result["error"].as_str().unwrap().contains("Invalid JSON"));
    }

    #[test]
    fn fmt_num_zero() {
        assert_eq!(fmt_num(0.0), "0");
    }

    #[test]
    fn fmt_num_integer() {
        assert_eq!(fmt_num(42.0), "42");
    }

    #[test]
    fn fmt_num_decimal() {
        assert_eq!(fmt_num(3.14), "3.14");
    }

    #[test]
    fn fmt_num_scientific() {
        assert_eq!(fmt_num(1e7), "1.00e7");
    }

    #[test]
    fn multi_series_renders() {
        let data = json!([
            {"x": [0.0, 1.0, 2.0], "y": [0.0, 1.0, 2.0], "label": "linear"},
            {"x": [0.0, 1.0, 2.0], "y": [0.0, 1.0, 4.0], "label": "quadratic"}
        ]);
        let series = parse_series(&data);
        assert_eq!(series.len(), 2);
        let vp = auto_viewport(&series);
        let canvas = render_to_canvas(&series, &vp, 30, 10);
        let lines = canvas.render();
        assert_eq!(lines.len(), 10);
    }

    #[test]
    fn braille_canvas_set_corners() {
        let mut canvas = BrailleCanvas::new(2, 2);
        canvas.set(0, 0); // top-left
        canvas.set(3, 7); // bottom-right
        let lines = canvas.render();
        assert_eq!(lines.len(), 2);
        // Top-left dot should be set
        let tl = lines[0].chars().next().unwrap() as u32;
        assert_ne!(tl, BRAILLE_BLANK);
        // Bottom-right dot should be set
        let br = lines[1].chars().nth(1).unwrap() as u32;
        assert_ne!(br, BRAILLE_BLANK);
    }

    #[test]
    fn braille_canvas_out_of_bounds_ignored() {
        let mut canvas = BrailleCanvas::new(2, 2);
        canvas.set(100, 100); // should not panic
        let lines = canvas.render();
        let all_blank = lines
            .iter()
            .all(|l| l.chars().all(|c| c as u32 == BRAILLE_BLANK));
        assert!(all_blank);
    }

    #[test]
    fn handles_nan_and_inf() {
        let data = json!([{
            "x": [0.0, 1.0, f64::NAN, 3.0, f64::INFINITY],
            "y": [0.0, 1.0, 2.0, f64::NAN, 4.0],
            "label": "messy"
        }]);
        let series = parse_series(&data);
        let vp = auto_viewport(&series);
        let canvas = render_to_canvas(&series, &vp, 20, 5);
        let lines = canvas.render();
        assert_eq!(lines.len(), 5); // should not panic
    }
}
