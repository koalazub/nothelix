#![allow(clippy::needless_pass_by_value)]

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Mutex;

use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use typst::diag::FileError;
use typst::foundations::{Bytes, Datetime, Duration};
use typst::syntax::{FileId, Source};
use typst::text::{Font, FontBook};
use typst::utils::LazyHash;
use typst::{Library, LibraryExt, World};
use typst_kit::fonts::FontStore;
use typst_layout::PagedDocument;

use crate::typst_export::latex_to_typst_math;

static MATH_IMAGE_CACHE: Mutex<Option<HashMap<(String, isize), (String, u32, u32)>>> =
    Mutex::new(None);

fn math_json(b64: &str, width: u32, height: u32, error: &str) -> String {
    format!(
        "{{\"b64\":\"{b64}\",\"width\":{width},\"height\":{height},\"error\":\"{error}\"}}"
    )
}

pub fn render_math_to_png(latex: String, font_size_pt: isize) -> String {
    let pt = font_size_pt.max(8).min(96) as f64;

    if let Ok(result) = get_cached(&latex, font_size_pt) {
        return math_json(&result.0, result.1, result.2, "");
    }

    match render_math_to_png_impl(&latex, pt) {
        Ok((b64, width, height)) => {
            cache_result(latex, font_size_pt, (&b64, width, height));
            math_json(&b64, width, height, "")
        }
        Err(e) => math_json("", 0, 0, &e),
    }
}

fn get_cached(latex: &str, font_size_pt: isize) -> Result<(String, u32, u32), ()> {
    let mut guard = MATH_IMAGE_CACHE.lock().map_err(|_| ())?;
    let cache = guard.get_or_insert_with(HashMap::new);
    cache
        .get(&(latex.to_string(), font_size_pt))
        .cloned()
        .ok_or(())
}

fn cache_result(latex: String, font_size_pt: isize, entry: (&str, u32, u32)) {
    if let Ok(mut guard) = MATH_IMAGE_CACHE.lock() {
        let cache = guard.get_or_insert_with(HashMap::new);
        cache.insert((latex, font_size_pt), (entry.0.to_string(), entry.1, entry.2));
    }
}

fn render_math_to_png_impl(latex: &str, font_size_pt: f64) -> Result<(String, u32, u32), String> {
    let typst_math = latex_to_typst_math(latex);
    let doc_source = build_typst_document(&typst_math, font_size_pt);
    let world = build_world(Source::detached(doc_source));

    let warned = typst::compile::<PagedDocument>(&world);
    let document = warned
        .output
        .map_err(|errors| format_diagnostics(&errors))?;

    let page = document.pages().first().ok_or("no pages rendered")?;
    let pixel_per_pt = infer_pixel_per_pt();
    let pixmap = typst_render::render(
        page,
        &typst_render::RenderOptions {
            pixel_per_pt: pixel_per_pt.into(),
            render_bleed: false,
        },
    );

    let png_bytes = pixmap
        .encode_png()
        .map_err(|e| format!("png encoding failed: {e}"))?;
    let b64 = BASE64.encode(&png_bytes);

    Ok((b64, pixmap.width(), pixmap.height()))
}

fn build_typst_document(typst_math: &str, font_size_pt: f64) -> String {
    format!(
        "#set page(width: auto, height: auto, margin: 0pt)\n\
         #set text(size: {font_size_pt:.1}pt)\n\
         #set math.equation(numbering: none)\n\
         $ {typst_math} $"
    )
}

fn infer_pixel_per_pt() -> f64 {
    let ppi = std::env::var("NOTHELIX_MATH_PPI")
        .ok()
        .and_then(|s| s.parse::<f64>().ok())
        .unwrap_or_else(infer_display_ppi)
        .clamp(72.0, 2400.0);
    ppi / 72.0
}

fn infer_display_ppi() -> f64 {
    let term_program = std::env::var("TERM_PROGRAM").unwrap_or_default();
    let term = std::env::var("TERM").unwrap_or_default();
    let is_macos = std::env::consts::OS == "macos";
    let is_kitty = std::env::var("KITTY_WINDOW_ID").is_ok() || term.contains("kitty");
    let is_iterm = term_program == "iTerm.app";
    let is_wezterm = term_program == "WezTerm";
    let is_apple_terminal = term_program == "Apple_Terminal";

    if is_macos && (is_kitty || is_iterm || is_wezterm || is_apple_terminal) {
        return 600.0;
    }

    if is_kitty || is_wezterm {
        return 400.0;
    }

    300.0
}

fn build_world(source: Source) -> MathWorld {
    let entries: Vec<_> = typst_kit::fonts::embedded().collect();
    let infos: Vec<_> = entries.iter().map(|(_, info)| info.clone()).collect();
    let mut fonts = FontStore::new();
    fonts.extend(entries);

    MathWorld {
        library: LazyHash::new(Library::default()),
        book: LazyHash::new(FontBook::from_infos(infos)),
        fonts,
        main: source.id(),
        source,
    }
}

struct MathWorld {
    library: LazyHash<Library>,
    book: LazyHash<FontBook>,
    fonts: FontStore,
    main: FileId,
    source: Source,
}

impl World for MathWorld {
    fn library(&self) -> &LazyHash<Library> {
        &self.library
    }

    fn book(&self) -> &LazyHash<FontBook> {
        &self.book
    }

    fn main(&self) -> FileId {
        self.main
    }

    fn source(&self, id: FileId) -> Result<Source, FileError> {
        if id == self.main {
            Ok(self.source.clone())
        } else {
            Err(FileError::NotFound(PathBuf::new()))
        }
    }

    fn file(&self, id: FileId) -> Result<Bytes, FileError> {
        if id == self.main {
            Ok(Bytes::from_string(self.source.text().to_string()))
        } else {
            Err(FileError::NotFound(PathBuf::new()))
        }
    }

    fn font(&self, index: usize) -> Option<Font> {
        self.fonts.font(index)
    }

    fn today(&self, _offset: Option<Duration>) -> Option<Datetime> {
        None
    }
}

fn format_diagnostics(errors: &[typst::diag::SourceDiagnostic]) -> String {
    errors
        .iter()
        .map(|e| e.message.to_string())
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_typst_document() {
        let doc = build_typst_document("alpha + beta", 12.0);
        assert!(doc.contains("$ alpha + beta $"));
        assert!(doc.contains("12.0pt"));
        assert!(doc.contains("numbering: none"));
    }

    #[test]
    fn renders_matrix_to_png_via_typst() {
        let latex = r"\widetilde{G}^{-1}(\omega) = \frac{1}{\pi} \begin{bmatrix} \pi - \omega & -i \\ \omega & i \end{bmatrix}";
        let json = render_math_to_png(latex.to_string(), 14);
        assert!(
            json.contains("\"error\":\"\""),
            "expected success, got: {json}"
        );
        assert!(json.contains("\"width\":"), "expected width, got: {json}");
        assert!(
            json.contains("\"height\":"),
            "expected height, got: {json}"
        );
        assert!(
            json.contains("\"b64\":\"iVBOR"),
            "expected png b64 prefix, got: {json}"
        );
    }
}
