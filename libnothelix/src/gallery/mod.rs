mod fixtures;

pub use fixtures::{DOCS_VIEWPORT, VIEWPORTS, Viewport};

use std::collections::HashMap;
use std::fmt::Write as _;

use fixtures::{
    CELL_ASPECT, CONCEAL_FIXTURES, EQUATION_SIZES, ERROR_FIXTURES, EquationSize, ErrorFixture,
    MARKDOWN_CELL, PLOT_SERIES_JSON, PT_PER_ROW, REFLOW_FIXTURES, RESERVE_BLOCK_SIZES,
    RESERVE_DOCUMENT,
};

use crate::chart::render_braille_chart;
use crate::error_format::{FormatContext, format_error};
use crate::markdown_overlays::scan_markdown_overlays;
use crate::math_format::{format_math, reserve_math_lines};
use crate::math_image::math_image_grid;
use crate::typst_export::md_to_typst;
use crate::unicode::{
    compute_conceal_overlays, compute_conceal_overlays_for_comments_with_options,
};

pub struct GalleryCase {
    pub name: &'static str,
    pub viewport: Option<Viewport>,
    pub output: String,
}

impl GalleryCase {
    pub fn snapshot_name(&self) -> String {
        match self.viewport {
            Some(viewport) => format!("{}@{}x{}", self.name, viewport.cols, viewport.rows),
            None => self.name.to_string(),
        }
    }

    pub fn document_name(&self) -> String {
        match self.viewport {
            Some(viewport) => format!("{}-{}x{}", self.name, viewport.cols, viewport.rows),
            None => self.name.to_string(),
        }
    }
}

pub fn fixed_cases() -> Result<Vec<GalleryCase>, String> {
    let mut cases = Vec::new();

    for fixture in CONCEAL_FIXTURES {
        cases.push(GalleryCase {
            name: fixture.name,
            viewport: None,
            output: apply_conceal(fixture.source),
        });
    }
    for fixture in CONCEAL_FIXTURES {
        cases.push(GalleryCase {
            name: fixture.overlays_name,
            viewport: None,
            output: pretty_overlay_json(&compute_conceal_overlays(fixture.source.to_string()))?,
        });
    }

    for fixture in ERROR_FIXTURES {
        cases.push(GalleryCase {
            name: fixture.name,
            viewport: None,
            output: render_error(fixture)?,
        });
    }

    for fixture in REFLOW_FIXTURES {
        cases.push(GalleryCase {
            name: fixture.name,
            viewport: None,
            output: format_math(fixture.source.to_string()),
        });
    }

    cases.push(GalleryCase {
        name: "markdown-overlays",
        viewport: None,
        output: scan_markdown_overlays(comment_prefixed(MARKDOWN_CELL), 0),
    });

    cases.push(GalleryCase {
        name: "typst-export",
        viewport: None,
        output: md_to_typst(MARKDOWN_CELL)
            .map_err(|e| format!("gallery: typst export of the markdown cell failed: {e}"))?,
    });

    Ok(cases)
}

pub fn viewport_cases(viewport: Viewport) -> Result<Vec<GalleryCase>, String> {
    Ok(vec![
        GalleryCase {
            name: "braille-chart",
            viewport: Some(viewport),
            output: render_braille_plot(viewport)?,
        },
        GalleryCase {
            name: "math-image-grid",
            viewport: Some(viewport),
            output: render_grid_table(viewport, EQUATION_SIZES),
        },
        GalleryCase {
            name: "math-reserve",
            viewport: Some(viewport),
            output: reserve_math_lines(RESERVE_DOCUMENT.to_string(), reserve_spec(viewport)),
        },
    ])
}

pub fn document_cases() -> Result<Vec<GalleryCase>, String> {
    let mut cases = fixed_cases()?;
    cases.extend(viewport_cases(DOCS_VIEWPORT)?);
    Ok(cases)
}

fn apply_conceal(source: &str) -> String {
    let tsv = compute_conceal_overlays_for_comments_with_options(source.to_string(), false);
    let mut replacements: HashMap<usize, &str> = HashMap::new();
    for line in tsv.lines() {
        if let Some((offset, replacement)) = line.split_once('\t')
            && let Ok(index) = offset.parse::<usize>()
        {
            replacements.insert(index, replacement);
        }
    }
    let mut out = String::new();
    for (index, ch) in source.chars().enumerate() {
        match replacements.get(&index) {
            Some(replacement) => out.push_str(replacement),
            None => out.push(ch),
        }
    }
    out
}

fn pretty_overlay_json(json: &str) -> Result<String, String> {
    let parsed: serde_json::Value = serde_json::from_str(json)
        .map_err(|e| format!("gallery: conceal overlays were not valid JSON: {e}"))?;
    let mut out = String::new();
    let items = parsed
        .as_array()
        .ok_or_else(|| "gallery: conceal overlays were not a JSON array".to_string())?;
    for item in items {
        let offset = item["offset"]
            .as_u64()
            .ok_or_else(|| format!("gallery: overlay entry without an offset: {item}"))?;
        let replacement = item["replacement"]
            .as_str()
            .ok_or_else(|| format!("gallery: overlay entry without a replacement: {item}"))?;
        let _ = writeln!(out, "{offset}\t{replacement}");
    }
    Ok(out)
}

fn comment_prefixed(markdown: &str) -> String {
    let mut out = String::new();
    for line in markdown.lines() {
        if line.is_empty() {
            out.push('#');
        } else {
            out.push_str("# ");
            out.push_str(line);
        }
        out.push('\n');
    }
    out
}

fn render_error(fixture: &ErrorFixture) -> Result<String, String> {
    let Some(notebook) = fixture.notebook else {
        return Ok(format_error(&FormatContext {
            error_json: fixture.error_json,
            raw_error: fixture.raw_error,
            notebook_path: None,
        }));
    };

    let dir = tempfile::tempdir().map_err(|e| {
        format!(
            "gallery: cannot create a temp dir for {}: {e}",
            fixture.name
        )
    })?;
    let path = dir.path().join("linear-algebra.jl");
    std::fs::write(&path, notebook).map_err(|e| {
        format!(
            "gallery: cannot write the notebook fixture {}: {e}",
            path.display()
        )
    })?;
    let path_text = path.to_string_lossy().into_owned();
    Ok(format_error(&FormatContext {
        error_json: fixture.error_json,
        raw_error: fixture.raw_error,
        notebook_path: Some(&path_text),
    }))
}

fn render_braille_plot(viewport: Viewport) -> Result<String, String> {
    let params = format!(
        r#"{{"plot_data": {PLOT_SERIES_JSON}, "cols": {}, "rows": {}}}"#,
        viewport.cols, viewport.rows
    );
    let reply: serde_json::Value = serde_json::from_str(&render_braille_chart(params))
        .map_err(|e| format!("gallery: braille chart reply was not valid JSON: {e}"))?;

    let error = reply["error"].as_str().unwrap_or_default();
    if !error.is_empty() {
        return Err(format!("gallery: braille chart failed: {error}"));
    }

    let lines = reply["lines"]
        .as_array()
        .ok_or_else(|| "gallery: braille chart reply had no lines".to_string())?;
    let labels = reply["series_labels"]
        .as_array()
        .ok_or_else(|| "gallery: braille chart reply had no series labels".to_string())?;

    let mut out = String::new();
    let _ = writeln!(
        out,
        "{} cols x {} rows   x {} .. {}   y {} .. {}",
        viewport.cols,
        viewport.rows,
        text_field(&reply, "x_label_left")?,
        text_field(&reply, "x_label_right")?,
        text_field(&reply, "y_label_bottom")?,
        text_field(&reply, "y_label_top")?
    );
    let names: Vec<&str> = labels
        .iter()
        .filter_map(serde_json::Value::as_str)
        .collect();
    let _ = writeln!(out, "series: {}", names.join(", "));
    for line in lines {
        let row = line
            .as_str()
            .ok_or_else(|| "gallery: braille chart row was not a string".to_string())?;
        out.push_str(row);
        out.push('\n');
    }
    Ok(out)
}

fn text_field(reply: &serde_json::Value, key: &str) -> Result<String, String> {
    reply[key]
        .as_str()
        .map(str::to_string)
        .ok_or_else(|| format!("gallery: braille chart reply had no {key}"))
}

fn render_grid_table(viewport: Viewport, sizes: &[EquationSize]) -> String {
    let mut out = String::new();
    let _ = writeln!(
        out,
        "viewport {} cols x {} rows",
        viewport.cols, viewport.rows
    );
    let _ = writeln!(
        out,
        "{:<18}{:>10}{:>11}{:>7}{:>7}  wider_than_viewport",
        "equation", "width_pt", "height_pt", "rows", "cols"
    );
    for size in sizes {
        let (rows, cols) = math_image_grid(
            size.width_pt,
            size.height_pt,
            grid_row_budget(viewport),
            CELL_ASPECT,
            PT_PER_ROW,
        );
        let _ = writeln!(
            out,
            "{:<18}{:>10.1}{:>11.1}{:>7}{:>7}  {}",
            size.name,
            size.width_pt,
            size.height_pt,
            rows,
            cols,
            if cols as usize > viewport.cols {
                "yes"
            } else {
                "no"
            }
        );
    }
    out
}

fn grid_row_budget(viewport: Viewport) -> u32 {
    u32::try_from(viewport.rows).unwrap_or(u32::MAX)
}

fn reserve_spec(viewport: Viewport) -> String {
    RESERVE_BLOCK_SIZES
        .iter()
        .map(|size| {
            let (rows, _) = math_image_grid(
                size.width_pt,
                size.height_pt,
                grid_row_budget(viewport),
                CELL_ASPECT,
                PT_PER_ROW,
            );
            rows.to_string()
        })
        .collect::<Vec<_>>()
        .join(",")
}
