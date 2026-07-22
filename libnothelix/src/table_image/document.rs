use std::fmt::Write;

use super::markdown::PipeTable;
use crate::math_image::HexColor;

pub(super) fn build_table_document(
    table: &PipeTable,
    font_size_pt: f64,
    color: &HexColor,
) -> String {
    let ncols = table.aligns.len();
    let aligns = table
        .aligns
        .iter()
        .map(|a| a.typst())
        .collect::<Vec<_>>()
        .join(", ");

    let mut header_cells = String::new();
    for i in 0..ncols {
        let cell = table.header.get(i).map_or("", String::as_str);
        let _ = write!(header_cells, "strong(\"{}\"), ", typst_escape(cell));
    }

    let mut body_cells = String::new();
    for row in &table.body {
        for i in 0..ncols {
            let cell = row.get(i).map_or("", String::as_str);
            let _ = write!(body_cells, "\"{}\", ", typst_escape(cell));
        }
    }

    let color = color.as_str();
    format!(
        "#set page(width: auto, height: auto, margin: 4pt, fill: none)\n\
         #set text(size: {font_size_pt:.1}pt, fill: rgb(\"{color}\"))\n\
         #table(\n\
         \x20 columns: {ncols},\n\
         \x20 align: ({aligns}),\n\
         \x20 stroke: 0.5pt + rgb(\"{color}\"),\n\
         \x20 inset: 6pt,\n\
         \x20 table.header({header_cells}),\n\
         \x20 {body_cells}\n\
         )"
    )
}

fn typst_escape(cell: &str) -> String {
    let mut out = String::with_capacity(cell.len());
    for c in cell.chars() {
        if c == '\\' || c == '"' {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::typst_escape;

    #[test]
    fn quotes_and_backslashes_escaped_for_typst() {
        assert_eq!(typst_escape(r#"a"b\c"#), r#"a\"b\\c"#);
    }
}
