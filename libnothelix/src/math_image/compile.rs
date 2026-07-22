use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use typst_layout::PagedDocument;
use typst_svg::{SvgOptions, svg};

#[cfg(feature = "native")]
use typst_pdf::{PdfOptions, pdf};

use super::reply::RenderedSvg;
use super::world::{MathWorld, describe};
use crate::error::{Error, RenderStage, Result};

fn stage_failure(stage: RenderStage, subject: &str, detail: String) -> Error {
    Error::Render {
        stage,
        subject: subject.to_string(),
        detail,
    }
}

pub(crate) fn compile_typst_to_svg(doc_source: String, subject: &str) -> Result<RenderedSvg> {
    let world = MathWorld::detached(doc_source);
    let document = typst::compile::<PagedDocument>(&world)
        .output
        .map_err(|errors| stage_failure(RenderStage::TypstCompile, subject, describe(&errors)))?;

    let page = document.pages().first().ok_or_else(|| {
        stage_failure(
            RenderStage::TypstCompile,
            subject,
            "no pages rendered".to_string(),
        )
    })?;

    let size = page.frame.size();
    let markup = svg(
        page,
        &SvgOptions {
            render_bleed: false,
            pretty: false,
        },
    );

    Ok(RenderedSvg {
        b64: BASE64.encode(markup.as_bytes()),
        width: size.x.to_pt().round().max(1.0) as u32,
        height: size.y.to_pt().round().max(1.0) as u32,
    })
}

#[cfg(feature = "native")]
pub(crate) fn compile_typst_to_pdf(typst_source: &str) -> Result<Vec<u8>> {
    let subject = excerpt(typst_source);
    let world = MathWorld::detached(typst_source.to_string());
    let document = typst::compile::<PagedDocument>(&world)
        .output
        .map_err(|errors| stage_failure(RenderStage::TypstCompile, &subject, describe(&errors)))?;

    pdf(&document, &PdfOptions::default())
        .map_err(|errors| stage_failure(RenderStage::PdfExport, &subject, describe(&errors)))
}

#[cfg(feature = "native")]
fn excerpt(source: &str) -> String {
    const LIMIT: usize = 80;
    let head = source
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .unwrap_or_default();
    if head.chars().count() <= LIMIT {
        head.to_string()
    } else {
        head.chars().take(LIMIT).chain(['…']).collect()
    }
}
