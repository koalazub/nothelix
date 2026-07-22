use crate::error::{Error, RenderStage, Result};

const SUPERSAMPLE: f32 = 2.0;

pub(super) fn rasterize_to_png(svg_data: &[u8]) -> Result<Vec<u8>> {
    let subject = || format!("{}-byte svg", svg_data.len());
    let failure = |stage, detail: String| Error::Render {
        stage,
        subject: subject(),
        detail,
    };

    let tree = resvg::usvg::Tree::from_data(svg_data, &resvg::usvg::Options::default())
        .map_err(|e| failure(RenderStage::SvgParse, e.to_string()))?;
    let size = tree.size();
    let width = ((size.width() * SUPERSAMPLE).ceil() as u32).max(1);
    let height = ((size.height() * SUPERSAMPLE).ceil() as u32).max(1);

    let mut pixmap = tiny_skia::Pixmap::new(width, height).ok_or_else(|| {
        failure(
            RenderStage::Rasterize,
            format!("cannot allocate a {width}x{height} pixmap"),
        )
    })?;
    resvg::render(
        &tree,
        tiny_skia::Transform::from_scale(SUPERSAMPLE, SUPERSAMPLE),
        &mut pixmap.as_mut(),
    );

    pixmap
        .encode_png()
        .map_err(|e| failure(RenderStage::Rasterize, e.to_string()))
}
