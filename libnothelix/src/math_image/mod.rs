#![allow(clippy::needless_pass_by_value)]

mod cache;
mod color;
mod compile;
mod document;
mod grid;
mod reply;
mod world;

#[cfg(feature = "native")]
mod batch;

#[cfg(feature = "native")]
pub use grid::{math_image_grid, math_image_grid_ffi};

pub(crate) use color::HexColor;
pub(crate) use compile::compile_typst_to_svg;
pub(crate) use reply::{RenderedSvg, math_json};

use crate::error::Result;
use crate::typst_export::latex_to_typst_math;
use cache::RenderRequest;

pub(crate) const BATCH_SEP: char = '\u{1e}';

const SMALLEST_FONT_PT: isize = 8;
const LARGEST_FONT_PT: isize = 96;

pub fn render_math_to_svg(latex: String, font_size_pt: isize, text_color: String) -> String {
    let color = HexColor::parse(&text_color);
    match render_equation(&latex, font_size_pt, &color) {
        Ok(svg) => svg.to_json(),
        Err(failure) => reply::failure_json(&failure),
    }
}

fn render_equation(latex: &str, font_size_pt: isize, color: &HexColor) -> Result<RenderedSvg> {
    let request = RenderRequest::new(latex, font_size_pt, color);
    if let Some(cached) = cache::lookup(&request)? {
        return Ok(cached);
    }

    let typst_math = latex_to_typst_math(latex)?;
    let source = document::build_typst_document(&typst_math, legible_font_pt(font_size_pt), color);
    let svg = compile_typst_to_svg(source, latex)?;

    cache::store(request, &svg)?;
    Ok(svg)
}

pub(crate) fn legible_font_pt(font_size_pt: isize) -> f64 {
    font_size_pt.clamp(SMALLEST_FONT_PT, LARGEST_FONT_PT) as f64
}

#[cfg(feature = "native")]
pub(crate) fn spawn_batch(
    blocks: String,
    font_size_pt: isize,
    text_color: String,
    render_block: batch::RenderBlock,
) -> String {
    crate::error::ffi(batch::spawn(blocks, font_size_pt, text_color, render_block))
}

#[cfg(feature = "native")]
pub fn start_render_batch(blocks: String, font_size_pt: isize, text_color: String) -> String {
    spawn_batch(blocks, font_size_pt, text_color, render_math_to_svg)
}

#[cfg(feature = "native")]
pub fn poll_render_batch(job_id: String) -> String {
    batch::poll(&job_id).into_string()
}

#[cfg(feature = "native")]
pub fn render_typst_to_pdf(typst_source: String, out_path: String) -> String {
    crate::error::ffi(write_pdf(&typst_source, &out_path).map(|()| String::new()))
}

#[cfg(feature = "native")]
fn write_pdf(typst_source: &str, out_path: &str) -> Result<()> {
    let bytes = compile::compile_typst_to_pdf(typst_source)?;
    std::fs::write(out_path, &bytes).map_err(|io| crate::error::Error::writing(out_path, io))
}

#[cfg(all(test, feature = "native"))]
fn render_math_batch(blocks: String, font_size_pt: isize, text_color: String) -> String {
    batch::compile_in_parallel(blocks, font_size_pt, text_color, render_math_to_svg)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn width_of(json: &str) -> u32 {
        let key = "\"width\":";
        let start = json.find(key).expect("width key") + key.len();
        let rest = &json[start..];
        let end = rest.find(',').expect("width end");
        rest[..end].parse().expect("width number")
    }

    #[test]
    fn async_batch_round_trips() {
        let blob = format!("alpha{BATCH_SEP}\\beta");
        let job = start_render_batch(blob, 14, "e8e8e8".to_string());
        let mut reply = poll_render_batch(job.clone());
        let mut waited = 0;
        while reply == "PENDING" {
            std::thread::sleep(std::time::Duration::from_millis(20));
            waited += 20;
            assert!(waited < 30_000, "batch never completed");
            reply = poll_render_batch(job.clone());
        }
        assert!(!reply.starts_with("ERROR:"), "batch errored: {reply}");
        let parts: Vec<&str> = reply.split(BATCH_SEP).collect();
        assert_eq!(parts.len(), 2, "expected 2 results, got: {reply}");
        assert!(parts.iter().all(|p| p.starts_with('{')), "got: {reply}");
        assert_eq!(poll_render_batch(job), "ERROR:expired");
    }

    #[test]
    fn framed_display_math_still_renders() {
        let json = render_math_to_svg(
            r"\varphi_n(t) = \mathrm{sinc}(t - t_n)".to_string(),
            14,
            "e8e8e8".to_string(),
        );
        assert!(
            json.contains("\"error\":\"\""),
            "framed render failed: {json}"
        );
        assert!(
            json.contains("\"b64\":\"PHN2Zy"),
            "expected svg payload: {json}"
        );
    }

    #[test]
    fn compiles_full_typst_source_to_pdf() {
        let bytes = compile::compile_typst_to_pdf("= Hello\n\nSome $x^2$ math.")
            .expect("pdf compile succeeds");
        assert!(
            bytes.starts_with(b"%PDF-"),
            "missing PDF magic: {:?}",
            &bytes[..bytes.len().min(8)]
        );
        assert!(
            bytes.len() > 500,
            "PDF unexpectedly small: {} bytes",
            bytes.len()
        );
    }

    #[test]
    fn render_typst_to_pdf_writes_file() {
        let dir = tempfile::tempdir().expect("tempdir");
        let out = dir.path().join("out.pdf");
        let out_path = out.to_string_lossy().into_owned();
        let result = render_typst_to_pdf("= Title\n\nBody text.".to_string(), out_path);
        assert_eq!(result, "", "expected success, got: {result}");
        let written = std::fs::read(&out).expect("pdf written to disk");
        assert!(written.starts_with(b"%PDF-"), "file is not a PDF");
    }

    #[test]
    fn render_typst_to_pdf_reports_compile_errors() {
        let dir = tempfile::tempdir().expect("tempdir");
        let out = dir.path().join("bad.pdf");
        let out_path = out.to_string_lossy().into_owned();
        let result = render_typst_to_pdf("#panic(\"boom\")".to_string(), out_path);
        assert!(
            result.starts_with("ERROR:"),
            "expected ERROR sentinel, got: {result}"
        );
        assert!(!out.exists(), "no file should be written on failure");
    }

    #[test]
    fn braceless_tfrac_with_quad_and_primes_renders() {
        let latex = r"p_0(\tfrac13)=p_1(\tfrac13),\quad p_0'(\tfrac13)=p_1'(\tfrac13),\quad p_1(\tfrac23)=p_2(\tfrac23),\quad p_1'(\tfrac23)=p_2'(\tfrac23).";
        let json = render_math_to_svg(latex.to_string(), 14, "e8e8e8".to_string());
        assert!(
            json.contains("\"error\":\"\""),
            "braceless tfrac render failed: {json}"
        );
    }

    #[test]
    fn cases_with_math_condition_renders() {
        let latex = "f(t) = \\begin{cases} t & t \\geq 0 \\\\ 0 & \\text{otherwise} \\end{cases}";
        let json = render_math_to_svg(latex.to_string(), 14, "e8e8e8".to_string());
        assert!(
            json.contains("\"error\":\"\""),
            "cases render failed: {json}"
        );
    }

    #[test]
    fn renders_matrix_to_svg_via_typst() {
        let latex = r"\widetilde{G}^{-1}(\omega) = \frac{1}{\pi} \begin{bmatrix} \pi - \omega & -i \\ \omega & i \end{bmatrix}";
        let json = render_math_to_svg(latex.to_string(), 14, "ffffff".to_string());
        assert!(
            json.contains("\"error\":\"\""),
            "expected success, got: {json}"
        );
        assert!(json.contains("\"width\":"), "expected width, got: {json}");
        assert!(json.contains("\"height\":"), "expected height, got: {json}");
        assert!(
            json.contains("\"b64\":\"PHN2Zy"),
            "expected svg b64 prefix, got: {json}"
        );
    }

    #[test]
    fn an_unconvertible_equation_names_the_latex_that_failed() {
        let json = render_math_to_svg("x }".to_string(), 14, "e8e8e8".to_string());
        let parsed: serde_json::Value = serde_json::from_str(&json).expect("valid JSON reply");
        let message = parsed["error"].as_str().expect("error field");
        assert!(!message.is_empty(), "expected a failure: {json}");
        assert!(message.contains("x }"), "{message}");
    }

    #[test]
    fn batch_renders_blocks_in_input_order() {
        let small = "x";
        let wide = r"\widetilde{G}^{-1}(\omega) = \frac{1}{\pi} \begin{bmatrix} \pi - \omega & -i \\ \omega & i \end{bmatrix}";
        let input = format!("{small}{BATCH_SEP}{wide}");
        let out = render_math_batch(input, 14, "e8e8e8".to_string());
        let parts: Vec<&str> = out.split(BATCH_SEP).collect();
        assert_eq!(parts.len(), 2, "two results for two blocks");
        assert!(
            parts[0].contains("\"error\":\"\""),
            "block 0 ok: {}",
            parts[0]
        );
        assert!(
            parts[1].contains("\"error\":\"\""),
            "block 1 ok: {}",
            parts[1]
        );
        assert!(
            width_of(parts[0]) < width_of(parts[1]),
            "order not preserved: {} vs {}",
            parts[0],
            parts[1]
        );
    }

    #[test]
    fn batch_single_block_has_one_result() {
        let out = render_math_batch("alpha".to_string(), 14, "e8e8e8".to_string());
        assert!(
            !out.contains(BATCH_SEP),
            "single block has no separator: {out}"
        );
        assert!(out.contains("\"error\":\"\""), "render ok: {out}");
    }

    #[test]
    fn batch_block_matches_single_render() {
        let latex = r"\alpha + \beta";
        let single = render_math_to_svg(latex.to_string(), 14, "ddccbb".to_string());
        let batched = render_math_batch(latex.to_string(), 14, "ddccbb".to_string());
        assert_eq!(single, batched);
    }
}
