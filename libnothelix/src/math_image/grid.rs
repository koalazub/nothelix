const MIN_ROWS: u32 = 2;
const MIN_COLS: u32 = 10;
const DEFAULT_CELL_ASPECT: f64 = 2.0;
const DEFAULT_PT_PER_ROW: f64 = 11.0;

pub fn math_image_grid(
    width_pt: f64,
    height_pt: f64,
    max_rows: u32,
    cell_aspect: f64,
    pt_per_row: f64,
) -> (u32, u32) {
    if width_pt <= 0.0 || height_pt <= 0.0 || pt_per_row <= 0.0 || cell_aspect <= 0.0 {
        return (MIN_ROWS, MIN_COLS);
    }
    let rows = ((height_pt / pt_per_row).round() as u32).clamp(MIN_ROWS, max_rows.max(MIN_ROWS));
    let cols =
        ((f64::from(rows) * (width_pt / height_pt) * cell_aspect).round() as u32).max(MIN_COLS);
    (rows, cols)
}

pub fn math_image_grid_ffi(
    width: isize,
    height: isize,
    max_rows: isize,
    cell_aspect: String,
    pt_per_row: String,
) -> String {
    let (rows, cols) = math_image_grid(
        width as f64,
        height as f64,
        max_rows.max(0) as u32,
        tunable(&cell_aspect, DEFAULT_CELL_ASPECT),
        tunable(&pt_per_row, DEFAULT_PT_PER_ROW),
    );
    format!("{rows},{cols}")
}

fn tunable(raw: &str, default: f64) -> f64 {
    raw.trim().parse::<f64>().unwrap_or(default)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn visual_aspect(rows: u32, cols: u32, cell_aspect: f64) -> f64 {
        (f64::from(rows) / f64::from(cols)) * cell_aspect
    }

    #[test]
    fn grid_preserves_aspect_ratio() {
        let wide = math_image_grid(300.0, 40.0, 12, 2.0, 11.0);
        let tall = math_image_grid(120.0, 200.0, 12, 2.0, 11.0);
        let wide_err = (visual_aspect(wide.0, wide.1, 2.0) - 40.0 / 300.0).abs();
        let tall_err = (visual_aspect(tall.0, tall.1, 2.0) - 200.0 / 120.0).abs();
        assert!(wide_err < 0.15, "wide aspect off: {wide:?} err={wide_err}");
        assert!(tall_err < 0.20, "tall aspect off: {tall:?} err={tall_err}");
    }

    #[test]
    fn grid_rows_scale_with_height() {
        let short = math_image_grid(160.0, 30.0, 12, 2.0, 11.0).0;
        let medium = math_image_grid(160.0, 90.0, 12, 2.0, 11.0).0;
        let tall = math_image_grid(160.0, 160.0, 12, 2.0, 11.0).0;
        assert!(
            short < medium && medium < tall,
            "rows must grow with height: {short} {medium} {tall}"
        );
    }

    #[test]
    fn grid_clamps_to_bounds() {
        assert_eq!(math_image_grid(100.0, 5000.0, 8, 2.0, 11.0).0, 8);
        assert_eq!(math_image_grid(100.0, 1.0, 12, 2.0, 11.0).0, MIN_ROWS);
        assert!(math_image_grid(2.0, 200.0, 12, 2.0, 11.0).1 >= MIN_COLS);
    }

    #[test]
    fn grid_degenerate_input_is_safe() {
        assert_eq!(
            math_image_grid(0.0, 80.0, 12, 2.0, 11.0),
            (MIN_ROWS, MIN_COLS)
        );
        assert_eq!(
            math_image_grid(160.0, 0.0, 12, 2.0, 11.0),
            (MIN_ROWS, MIN_COLS)
        );
        assert_eq!(
            math_image_grid(160.0, 80.0, 12, 2.0, 0.0),
            (MIN_ROWS, MIN_COLS)
        );
    }

    #[test]
    fn grid_ffi_formats_pair() {
        let s = math_image_grid_ffi(160, 80, 12, "2.0".into(), "11.0".into());
        let (rows, cols) = s.split_once(',').expect("rows,cols");
        assert_eq!(rows, "7");
        assert!(cols.parse::<u32>().expect("cols number") >= MIN_COLS);
    }

    #[test]
    fn grid_ffi_tolerates_garbage_config() {
        let s = math_image_grid_ffi(160, 80, 12, "".into(), "nan-ish".into());
        assert!(s.split_once(',').is_some(), "expected rows,cols, got {s}");
    }
}
