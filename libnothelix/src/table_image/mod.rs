mod document;
mod markdown;

use crate::error::{Error, Result};
use crate::math_image::{HexColor, RenderedSvg, compile_typst_to_svg, legible_font_pt, math_json};
use markdown::PipeTable;

#[cfg(feature = "native")]
pub fn start_render_table_batch(blocks: String, font_size_pt: isize, text_color: String) -> String {
    crate::math_image::spawn_batch(blocks, font_size_pt, text_color, render_table_to_svg)
}

pub fn render_table_to_svg(block: String, font_size_pt: isize, text_color: String) -> String {
    match render_table(&block, font_size_pt, &HexColor::parse(&text_color)) {
        Ok(svg) => svg.to_json(),
        Err(failure) => math_json("", 0, 0, &failure.to_string()),
    }
}

fn render_table(block: &str, font_size_pt: isize, color: &HexColor) -> Result<RenderedSvg> {
    let lines: Vec<&str> = block.lines().collect();
    let table = PipeTable::parse(&lines).ok_or(Error::Malformed {
        subject: "table image",
        detail: "not a markdown table".to_string(),
    })?;
    let source = document::build_table_document(&table, legible_font_pt(font_size_pt), color);
    compile_typst_to_svg(source, block)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "| File | Description | URL |\n\
                          |:-----|:-----------:|----:|\n\
                          | a.h5 | strain data | link |";

    #[test]
    fn renders_table_to_svg() {
        let json = render_table_to_svg(SAMPLE.to_string(), 14, "e8e8e8".to_string());
        assert!(json.contains("\"error\":\"\""), "typst error: {json}");
        assert!(
            json.contains("\"b64\":\"PHN2Zy"),
            "expected svg payload: {json}"
        );
        assert!(!json.contains("\"width\":0"), "zero width: {json}");
    }

    #[test]
    fn non_table_input_reports_error() {
        let json = render_table_to_svg("hello world".to_string(), 14, "e8e8e8".to_string());
        assert!(
            !json.contains("\"error\":\"\""),
            "expected error for non-table: {json}"
        );
    }
}
