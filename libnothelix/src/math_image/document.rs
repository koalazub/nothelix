use super::color::HexColor;

pub(super) fn build_typst_document(
    typst_math: &str,
    font_size_pt: f64,
    color: &HexColor,
) -> String {
    let escaped = typst_math
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace(['\n', '\r'], " ");
    let text_color = color.as_str();
    format!(
        "#import \"/mitex/mod.typ\": mitex-scope\n\
         #import \"/mitex/compat.typ\": typst-compat-scope\n\
         #set page(width: auto, height: auto, margin: 3pt, fill: none)\n\
         #set text(size: {font_size_pt:.1}pt, fill: rgb(\"{text_color}\"))\n\
         #set math.equation(numbering: none)\n\
         #block(\n\
         \x20 stroke: (top: 0.6pt + gray, bottom: 0.6pt + gray),\n\
         \x20 inset: (top: 9pt, bottom: 9pt, left: 16pt, right: 16pt),\n\
         )[\n\
         \x20 #eval(\"$ {escaped} $\", scope: mitex-scope + typst-compat-scope)\n\
         ]"
    )
}

#[cfg(test)]
mod tests {
    use super::super::color::HexColor;
    use super::build_typst_document;

    fn document(math: &str, font_size_pt: f64, color: &str) -> String {
        build_typst_document(math, font_size_pt, &HexColor::parse(color))
    }

    #[test]
    fn builds_typst_document() {
        let doc = document("alpha + beta", 12.0, "e8e8e8");
        assert!(doc.contains("$ alpha + beta $"));
        assert!(doc.contains("12.0pt"));
        assert!(doc.contains("numbering: none"));
    }

    #[test]
    fn transparent_page_and_coloured_text() {
        let doc = document("alpha", 14.0, "ddccbb");
        assert!(
            doc.contains("fill: none"),
            "page must be transparent: {doc}"
        );
        assert!(
            doc.contains("rgb(\"ddccbb\")"),
            "text must use the supplied colour: {doc}"
        );
    }

    #[test]
    fn display_math_is_framed_for_emphasis() {
        let doc = document("alpha + beta", 14.0, "e8e8e8");
        assert!(
            doc.contains("#block("),
            "equation must be wrapped in a block: {doc}"
        );
        assert!(
            doc.contains("stroke:"),
            "frame needs top/bottom rules: {doc}"
        );
        assert!(
            doc.contains("inset:"),
            "frame needs breathing-room inset: {doc}"
        );
    }
}
