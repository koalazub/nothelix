use crate::error::{Error, RenderStage, Result};

pub fn latex_to_typst_math(latex: &str) -> Result<String> {
    mitex::convert_math(latex, None).map_err(|failure| Error::Render {
        stage: RenderStage::LatexConversion,
        subject: latex.to_string(),
        detail: failure.to_string(),
    })
}
