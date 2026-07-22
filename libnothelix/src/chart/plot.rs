use super::braille::BrailleCanvas;
use super::series::{Series, Viewport};

pub(super) fn draw(
    series: &[Series],
    viewport: &Viewport,
    cols: usize,
    rows: usize,
) -> BrailleCanvas {
    let mut canvas = BrailleCanvas::new(cols, rows);
    let dot_w = canvas.dot_width();
    let dot_h = canvas.dot_height();

    let to_px = |x: f64| {
        ((x - viewport.x_min) / viewport.x_range() * (dot_w as f64 - 1.0)).round() as isize
    };
    let to_py = |y: f64| {
        ((viewport.y_max - y) / viewport.y_range() * (dot_h as f64 - 1.0)).round() as isize
    };

    for s in series {
        let mut previous: Option<(isize, isize)> = None;

        for (&x, &y) in s.x.iter().zip(s.y.iter()) {
            if !x.is_finite() || !y.is_finite() {
                previous = None;
                continue;
            }

            let point = (to_px(x), to_py(y));
            match previous {
                Some(from) => connect(&mut canvas, from, point, dot_w, dot_h),
                None => plot(&mut canvas, point, dot_w, dot_h),
            }
            previous = Some(point);
        }
    }

    canvas
}

fn plot(canvas: &mut BrailleCanvas, (x, y): (isize, isize), w: usize, h: usize) {
    if x >= 0 && (x as usize) < w && y >= 0 && (y as usize) < h {
        canvas.set(x as usize, y as usize);
    }
}

fn connect(
    canvas: &mut BrailleCanvas,
    (x0, y0): (isize, isize),
    (x1, y1): (isize, isize),
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
        plot(canvas, (cx, cy), w, h);
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

#[cfg(test)]
mod tests {
    use super::super::braille::BLANK;
    use super::*;
    use serde_json::{Value, json};

    fn simple_series() -> Value {
        json!([{
            "x": [0.0, 1.0, 2.0, 3.0, 4.0, 5.0],
            "y": [0.0, 1.0, 4.0, 9.0, 16.0, 25.0],
            "label": "y = x²"
        }])
    }

    fn canvas_lines(data: &Value, cols: usize, rows: usize) -> Vec<String> {
        let series = Series::parse_all(data);
        let viewport = Viewport::enclosing(&series);
        draw(&series, &viewport, cols, rows).lines()
    }

    #[test]
    fn render_produces_correct_dimensions() {
        let lines = canvas_lines(&simple_series(), 40, 10);
        assert_eq!(lines.len(), 10);
        for line in &lines {
            assert_eq!(line.chars().count(), 40);
        }
    }

    #[test]
    fn render_not_all_blank() {
        let lines = canvas_lines(&simple_series(), 40, 10);
        let has_dots = lines.iter().any(|l| l.chars().any(|c| c != BLANK));
        assert!(has_dots, "canvas should contain at least some dots");
    }

    #[test]
    fn multi_series_renders() {
        let data = json!([
            {"x": [0.0, 1.0, 2.0], "y": [0.0, 1.0, 2.0], "label": "linear"},
            {"x": [0.0, 1.0, 2.0], "y": [0.0, 1.0, 4.0], "label": "quadratic"}
        ]);
        assert_eq!(Series::parse_all(&data).len(), 2);
        assert_eq!(canvas_lines(&data, 30, 10).len(), 10);
    }

    #[test]
    fn handles_nan_and_inf() {
        let data = json!([{
            "x": [0.0, 1.0, f64::NAN, 3.0, f64::INFINITY],
            "y": [0.0, 1.0, 2.0, f64::NAN, 4.0],
            "label": "messy"
        }]);
        assert_eq!(canvas_lines(&data, 20, 5).len(), 5);
    }
}
