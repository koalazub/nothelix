#![allow(clippy::needless_pass_by_value)]

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};

use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use typst::diag::FileError;
use typst::foundations::{Bytes, Datetime, Duration};
use typst::syntax::{FileId, RootedPath, Source, VirtualPath, VirtualRoot};
use typst::text::{Font, FontBook};
use typst::utils::LazyHash;
use typst::{Library, LibraryExt, World};
use typst_layout::PagedDocument;
use typst_svg::{SvgOptions, svg};

use typst_kit::fonts::FontStore;
#[cfg(feature = "native")]
use typst_pdf::{PdfOptions, pdf};

use crate::typst_export::latex_to_typst_math;

type MathImageCacheKey = (String, isize, String);
type MathImageCacheEntry = (String, u32, u32);

static MATH_IMAGE_CACHE: Mutex<Option<HashMap<MathImageCacheKey, MathImageCacheEntry>>> =
    Mutex::new(None);

pub(crate) fn math_json(b64: &str, width: u32, height: u32, error: &str) -> String {
    format!("{{\"b64\":\"{b64}\",\"width\":{width},\"height\":{height},\"error\":\"{error}\"}}")
}

pub fn render_math_to_svg(latex: String, font_size_pt: isize, text_color: String) -> String {
    let pt = font_size_pt.clamp(8, 96) as f64;
    let color = sanitize_hex_color(&text_color);

    if let Ok(result) = get_cached(&latex, font_size_pt, &color) {
        return math_json(&result.0, result.1, result.2, "");
    }

    match render_math_to_svg_impl(&latex, pt, &color) {
        Ok((b64, width, height)) => {
            cache_result(latex, font_size_pt, color, (&b64, width, height));
            math_json(&b64, width, height, "")
        }
        Err(e) => math_json("", 0, 0, &e),
    }
}

/// Record Separator: never appears in LaTeX/Typst or base64.
pub(crate) const BATCH_SEP: char = '\u{1e}';

/// Compile a `BATCH_SEP`-joined batch in parallel via `render_one`, returning
/// the JSON results joined the same way, in order. Shared by the synchronous
/// `render_math_batch` (used in tests) and the async `spawn_batch`.
#[cfg(feature = "native")]
fn run_batch(
    blocks: String,
    font_size_pt: isize,
    text_color: String,
    render_one: fn(String, isize, String) -> String,
) -> String {
    use rayon::prelude::*;

    let results: Vec<String> = blocks
        .split(BATCH_SEP)
        .collect::<Vec<_>>()
        .par_iter()
        .map(|b| render_one((*b).to_string(), font_size_pt, text_color.clone()))
        .collect();
    results.join(&BATCH_SEP.to_string())
}

/// Render `BATCH_SEP`-joined LaTeX blocks in parallel; returns their JSON
/// results joined the same way, in order.
#[cfg(all(test, feature = "native"))]
fn render_math_batch(blocks: String, font_size_pt: isize, text_color: String) -> String {
    run_batch(blocks, font_size_pt, text_color, render_math_to_svg)
}

/// State of an in-flight `start_render_batch` job. The `Instant` is the job's
/// creation time, used to evict abandoned entries — a poll chain that is
/// superseded by a newer render (generation guard) or times out simply stops
/// polling, so without a sweep the `Done` payload (the full base64 SVG batch)
/// would leak for the life of the process.
#[cfg(feature = "native")]
enum RenderJob {
    Pending(std::time::Instant),
    Done(std::time::Instant, String),
}

#[cfg(feature = "native")]
impl RenderJob {
    fn started(&self) -> std::time::Instant {
        match self {
            RenderJob::Pending(t) | RenderJob::Done(t, _) => *t,
        }
    }
}

#[cfg(feature = "native")]
fn render_jobs() -> &'static Mutex<HashMap<u64, RenderJob>> {
    static JOBS: OnceLock<Mutex<HashMap<u64, RenderJob>>> = OnceLock::new();
    JOBS.get_or_init(|| Mutex::new(HashMap::new()))
}

/// How long a job may sit unclaimed before the next `spawn_batch` evicts it.
/// Comfortably longer than the plugin's poll ceiling (~24s) so a live poll
/// never races the sweep, but bounded so abandoned jobs can't accumulate.
#[cfg(feature = "native")]
const RENDER_JOB_TTL: std::time::Duration = std::time::Duration::from_secs(60);

#[cfg(feature = "native")]
static RENDER_JOB_COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

/// Kick off a batch Typst compile on a plain Rust thread and return a job id
/// immediately. `render_one` compiles a single `BATCH_SEP`-delimited block
/// (`render_math_to_svg` for equations, `render_table_to_svg` for tables).
///
/// The work runs on a `std::thread` (NOT a Steel VM thread), so it is invisible
/// to Steel's GC safepoint machinery and never freezes the editor — unlike
/// calling the synchronous batch inside Steel's `spawn-native-thread`, where the
/// cloned VM stuck in this FFI makes the main-thread garbage collector busy-spin
/// until the compile finishes.
#[cfg(feature = "native")]
pub(crate) fn spawn_batch(
    blocks: String,
    font_size_pt: isize,
    text_color: String,
    render_one: fn(String, isize, String) -> String,
) -> String {
    let job_id = RENDER_JOB_COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let now = std::time::Instant::now();
    if let Ok(mut g) = render_jobs().lock() {
        // Sweep entries abandoned by a superseded or timed-out poll chain.
        g.retain(|_, job| now.duration_since(job.started()) < RENDER_JOB_TTL);
        g.insert(job_id, RenderJob::Pending(now));
    }
    std::thread::spawn(move || {
        let joined = run_batch(blocks, font_size_pt, text_color, render_one);
        if let Ok(mut g) = render_jobs().lock() {
            g.insert(job_id, RenderJob::Done(now, joined));
        }
    });
    job_id.to_string()
}

#[cfg(feature = "native")]
pub fn start_render_batch(blocks: String, font_size_pt: isize, text_color: String) -> String {
    spawn_batch(blocks, font_size_pt, text_color, render_math_to_svg)
}

/// Poll a job started by `start_render_batch`. Returns `"PENDING"` while the
/// compile is still running, `"ERROR:<reason>"` on a bad/expired id, or the
/// `BATCH_SEP`-joined JSON results (consuming the job) once complete. JSON
/// results always start with `{`, so they can never collide with the sentinels.
#[cfg(feature = "native")]
pub fn poll_render_batch(job_id: String) -> String {
    let Ok(id) = job_id.trim().parse::<u64>() else {
        return "ERROR:bad-job-id".to_string();
    };
    let Ok(mut g) = render_jobs().lock() else {
        return "ERROR:lock-poisoned".to_string();
    };
    match g.remove(&id) {
        Some(RenderJob::Done(_, result)) => result,
        Some(pending @ RenderJob::Pending(_)) => {
            g.insert(id, pending);
            "PENDING".to_string()
        }
        None => "ERROR:expired".to_string(),
    }
}

fn get_cached(latex: &str, font_size_pt: isize, color: &str) -> Result<(String, u32, u32), ()> {
    let mut guard = MATH_IMAGE_CACHE.lock().map_err(|_| ())?;
    let cache = guard.get_or_insert_with(HashMap::new);
    cache
        .get(&(latex.to_string(), font_size_pt, color.to_string()))
        .cloned()
        .ok_or(())
}

fn cache_result(latex: String, font_size_pt: isize, color: String, entry: (&str, u32, u32)) {
    if let Ok(mut guard) = MATH_IMAGE_CACHE.lock() {
        let cache = guard.get_or_insert_with(HashMap::new);
        cache.insert(
            (latex, font_size_pt, color),
            (entry.0.to_string(), entry.1, entry.2),
        );
    }
}

fn render_math_to_svg_impl(
    latex: &str,
    font_size_pt: f64,
    text_color: &str,
) -> Result<(String, u32, u32), String> {
    let typst_math = latex_to_typst_math(latex)?;
    let doc_source = build_typst_document(&typst_math, font_size_pt, text_color);
    compile_typst_to_svg(doc_source)
}

/// Normalise a caller-supplied colour to a 6-digit hex string Typst's
/// `rgb(..)` accepts. Strips a leading `#`, expands 3-digit shorthand, and
/// falls back to a light grey (legible on dark themes) for anything invalid.
fn sanitize_hex_color(input: &str) -> String {
    let hex = input.trim().trim_start_matches('#');
    let expanded = match hex.len() {
        3 => hex.chars().flat_map(|c| [c, c]).collect::<String>(),
        6 => hex.to_string(),
        _ => return "e8e8e8".to_string(),
    };
    if expanded.bytes().all(|b| b.is_ascii_hexdigit()) {
        expanded.to_ascii_lowercase()
    } else {
        "e8e8e8".to_string()
    }
}

pub(crate) fn compile_typst_to_svg(doc_source: String) -> Result<(String, u32, u32), String> {
    let world = build_world(Source::detached(doc_source));

    let warned = typst::compile::<PagedDocument>(&world);
    let document = warned
        .output
        .map_err(|errors| format_diagnostics(&errors))?;

    let page = document.pages().first().ok_or("no pages rendered")?;
    let size = page.frame.size();
    let width = size.x.to_pt().round().max(1.0) as u32;
    let height = size.y.to_pt().round().max(1.0) as u32;

    let svg_data = svg(
        page,
        &SvgOptions {
            render_bleed: false,
            pretty: false,
        },
    );
    let b64 = BASE64.encode(svg_data.as_bytes());

    Ok((b64, width, height))
}

#[cfg(feature = "native")]
pub fn render_typst_to_pdf(typst_source: String, out_path: String) -> String {
    match compile_typst_to_pdf(&typst_source) {
        Ok(bytes) => match std::fs::write(&out_path, &bytes) {
            Ok(()) => String::new(),
            Err(e) => format!("ERROR:{e}"),
        },
        Err(e) => format!("ERROR:{e}"),
    }
}

#[cfg(feature = "native")]
pub(crate) fn compile_typst_to_pdf(typst_source: &str) -> Result<Vec<u8>, String> {
    let world = build_world(Source::detached(typst_source.to_string()));

    let warned = typst::compile::<PagedDocument>(&world);
    let document = warned
        .output
        .map_err(|errors| format_diagnostics(&errors))?;

    pdf(&document, &PdfOptions::default()).map_err(|errors| format_diagnostics(&errors))
}

fn build_typst_document(typst_math: &str, font_size_pt: f64, text_color: &str) -> String {
    let escaped = typst_math
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace(['\n', '\r'], " ");
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

fn mitex_scope_sources() -> &'static [(FileId, Source)] {
    static SOURCES: OnceLock<Vec<(FileId, Source)>> = OnceLock::new();
    SOURCES.get_or_init(|| {
        [
            ("/mitex/mod.typ", include_str!("mitex/mod.typ")),
            ("/mitex/prelude.typ", include_str!("mitex/prelude.typ")),
            (
                "/mitex/latex/standard.typ",
                include_str!("mitex/latex/standard.typ"),
            ),
            ("/mitex/compat.typ", include_str!("mitex/compat.typ")),
        ]
        .into_iter()
        .map(|(path, text)| {
            let vpath = VirtualPath::new(path).expect("static mitex vpath");
            let id = FileId::new(RootedPath::new(VirtualRoot::Project, vpath));
            (id, Source::new(id, text.to_string()))
        })
        .collect()
    })
}

/// Typst fonts + library are identical for every render, but rebuilding
/// them (collecting the embedded font book, parsing each font's metadata)
/// dominated the per-render cost. Build once, share across all renders —
/// the difference between a notebook with many equations freezing the UI
/// and rendering smoothly.
struct TypstAssets {
    library: LazyHash<Library>,
    book: LazyHash<FontBook>,
    fonts: FontStore,
}

fn typst_assets() -> &'static TypstAssets {
    static ASSETS: OnceLock<TypstAssets> = OnceLock::new();
    ASSETS.get_or_init(|| {
        let entries: Vec<_> = typst_kit::fonts::embedded().collect();
        let infos: Vec<_> = entries.iter().map(|(_, info)| info.clone()).collect();
        let mut fonts = FontStore::new();
        fonts.extend(entries);
        TypstAssets {
            library: LazyHash::new(Library::default()),
            book: LazyHash::new(FontBook::from_infos(infos)),
            fonts,
        }
    })
}

fn build_world(source: Source) -> MathWorld {
    MathWorld {
        assets: typst_assets(),
        main: source.id(),
        source,
    }
}

struct MathWorld {
    assets: &'static TypstAssets,
    main: FileId,
    source: Source,
}

impl World for MathWorld {
    fn library(&self) -> &LazyHash<Library> {
        &self.assets.library
    }

    fn book(&self) -> &LazyHash<FontBook> {
        &self.assets.book
    }

    fn main(&self) -> FileId {
        self.main
    }

    fn source(&self, id: FileId) -> Result<Source, FileError> {
        if id == self.main {
            return Ok(self.source.clone());
        }
        mitex_scope_sources()
            .iter()
            .find(|(fid, _)| *fid == id)
            .map(|(_, src)| src.clone())
            .ok_or_else(|| FileError::NotFound(PathBuf::new()))
    }

    fn file(&self, id: FileId) -> Result<Bytes, FileError> {
        self.source(id)
            .map(|src| Bytes::from_string(src.text().to_string()))
    }

    fn font(&self, index: usize) -> Option<Font> {
        self.assets.fonts.font(index)
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

/// Terminal `(rows, cols)` grid for a display-math image, from the SVG's
/// intrinsic size in Typst points.
///
/// `rows` scales with the equation's true height — a tall stacked
/// fraction gets more rows than a one-line equation — clamped to
/// `[2, max_rows]`. `pt_per_row` is how many points of equation height
/// map to one terminal row. `cols` is then chosen so the on-screen
/// aspect ratio matches the SVG: a `rows × cols` block of cells whose
/// height/width ratio is `cell_aspect` has visual aspect
/// `(rows / cols) * cell_aspect`, which equals `height / width` exactly
/// when `cols = rows * (width / height) * cell_aspect`. So the equation
/// is never squashed or stretched regardless of shape — the bug this
/// replaces forced every image to a fixed row count, distorting both
/// tall and short equations.
///
/// Degenerate input (non-positive dims) returns the minimum readable
/// grid rather than dividing by zero.
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

const MIN_ROWS: u32 = 2;
const MIN_COLS: u32 = 10;

/// FFI wrapper: returns `"rows,cols"`. The on-screen sizing config
/// (`max_rows`, `cell_aspect`, `pt_per_row`) lives in the Scheme layer as
/// runtime-tunable boxes and is passed through here so the deterministic
/// computation stays in one tested place.
///
/// `cell_aspect`/`pt_per_row` arrive as strings because the Steel dylib
/// FFI has no `f64` argument marshaller (only integer/string/bool); they
/// are parsed here, falling back to sane defaults on garbage input.
#[allow(clippy::needless_pass_by_value)]
pub fn math_image_grid_ffi(
    width: isize,
    height: isize,
    max_rows: isize,
    cell_aspect: String,
    pt_per_row: String,
) -> String {
    let aspect = cell_aspect.trim().parse::<f64>().unwrap_or(2.0);
    let ppr = pt_per_row.trim().parse::<f64>().unwrap_or(11.0);
    let (rows, cols) = math_image_grid(
        width as f64,
        height as f64,
        max_rows.max(0) as u32,
        aspect,
        ppr,
    );
    format!("{rows},{cols}")
}

#[cfg(test)]
mod tests {
    use super::*;

    // On-screen visual aspect of a rows×cols block of cells whose
    // height/width ratio is `cell_aspect`.
    fn visual_aspect(rows: u32, cols: u32, cell_aspect: f64) -> f64 {
        (f64::from(rows) / f64::from(cols)) * cell_aspect
    }

    #[test]
    fn async_batch_round_trips() {
        // The freeze fix runs the batch on a plain std::thread and hands results
        // back via poll. Start a 2-block batch and poll to completion; both
        // results must come back as JSON in order, with no PENDING leaking
        // through and no error sentinel.
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
        // The job is consumed once Done — a second poll must report expiry,
        // never a stale duplicate.
        assert_eq!(poll_render_batch(job), "ERROR:expired");
    }

    #[test]
    fn grid_preserves_aspect_ratio() {
        // A wide one-liner and a tall stacked block, same cell aspect.
        let wide = math_image_grid(300.0, 40.0, 12, 2.0, 11.0);
        let tall = math_image_grid(120.0, 200.0, 12, 2.0, 11.0);
        // Visual aspect should track intrinsic height/width within the
        // rounding granularity of a whole-cell grid.
        let wide_err = (visual_aspect(wide.0, wide.1, 2.0) - 40.0 / 300.0).abs();
        let tall_err = (visual_aspect(tall.0, tall.1, 2.0) - 200.0 / 120.0).abs();
        assert!(wide_err < 0.15, "wide aspect off: {wide:?} err={wide_err}");
        assert!(tall_err < 0.20, "tall aspect off: {tall:?} err={tall_err}");
    }

    #[test]
    fn grid_rows_scale_with_height() {
        // The regression the old code had: rows must NOT be constant.
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
        // Huge height caps at max_rows; tiny clamps up to MIN_ROWS.
        assert_eq!(math_image_grid(100.0, 5000.0, 8, 2.0, 11.0).0, 8);
        assert_eq!(math_image_grid(100.0, 1.0, 12, 2.0, 11.0).0, MIN_ROWS);
        // Cols never below the readable minimum.
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
        // 160x80 (the Scheme mock): height 80 / 11 ≈ 7 rows; cols keeps aspect.
        let s = math_image_grid_ffi(160, 80, 12, "2.0".into(), "11.0".into());
        let (rows, cols) = s.split_once(',').unwrap();
        assert_eq!(rows, "7");
        assert!(cols.parse::<u32>().unwrap() >= MIN_COLS);
    }

    #[test]
    fn grid_ffi_tolerates_garbage_config() {
        // Unparseable aspect/ppr fall back to defaults rather than panicking.
        let s = math_image_grid_ffi(160, 80, 12, "".into(), "nan-ish".into());
        assert!(s.split_once(',').is_some(), "expected rows,cols, got {s}");
    }

    #[test]
    fn builds_typst_document() {
        let doc = build_typst_document("alpha + beta", 12.0, "e8e8e8");
        assert!(doc.contains("$ alpha + beta $"));
        assert!(doc.contains("12.0pt"));
        assert!(doc.contains("numbering: none"));
    }

    #[test]
    fn transparent_page_and_coloured_text() {
        // The image embeds into a dark editor, so the page must be
        // transparent (no white box) and the glyphs must take the caller's
        // colour rather than default black.
        let doc = build_typst_document("alpha", 14.0, "ddccbb");
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
    fn sanitize_hex_color_normalises_and_defaults() {
        assert_eq!(sanitize_hex_color("#FFFFFF"), "ffffff");
        assert_eq!(sanitize_hex_color("e8e8e8"), "e8e8e8");
        assert_eq!(sanitize_hex_color("fff"), "ffffff");
        assert_eq!(sanitize_hex_color("not-a-colour"), "e8e8e8");
        assert_eq!(sanitize_hex_color(""), "e8e8e8");
    }

    #[test]
    fn display_math_is_framed_for_emphasis() {
        // A `$$` block is the author emphasising something — the rendered
        // SVG must carry the breathing-room-plus-rule frame so it reads as
        // special, not as ordinary inline math.
        let doc = build_typst_document("alpha + beta", 14.0, "e8e8e8");
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

    #[test]
    fn framed_display_math_still_renders() {
        // The block wrapper must not break Typst compilation — a real
        // render of framed math has to succeed end to end.
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
        let bytes =
            compile_typst_to_pdf("= Hello\n\nSome $x^2$ math.").expect("pdf compile succeeds");
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
        // Braceless args (`\tfrac13` = `\tfrac{1}{3}`), `\quad` spacing and
        // primes on subscripted symbols must all survive conversion + compile.
        let latex = r"p_0(\tfrac13)=p_1(\tfrac13),\quad p_0'(\tfrac13)=p_1'(\tfrac13),\quad p_1(\tfrac23)=p_2(\tfrac23),\quad p_1'(\tfrac23)=p_2'(\tfrac23).";
        let json = render_math_to_svg(latex.to_string(), 14, "e8e8e8".to_string());
        assert!(
            json.contains("\"error\":\"\""),
            "braceless tfrac render failed: {json}"
        );
    }

    #[test]
    fn cases_with_math_condition_renders() {
        // Math conditions kept as math (with `&` alignment) must still
        // compile in Typst — regression guard for the cases conversion.
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

    fn width_of(json: &str) -> u32 {
        let key = "\"width\":";
        let start = json.find(key).expect("width key") + key.len();
        let rest = &json[start..];
        let end = rest.find(',').expect("width end");
        rest[..end].parse().expect("width number")
    }

    #[test]
    fn batch_renders_blocks_in_input_order() {
        let sep = '\u{1e}';
        let small = "x";
        let wide = r"\widetilde{G}^{-1}(\omega) = \frac{1}{\pi} \begin{bmatrix} \pi - \omega & -i \\ \omega & i \end{bmatrix}";
        let input = format!("{small}{sep}{wide}");
        let out = render_math_batch(input, 14, "e8e8e8".to_string());
        let parts: Vec<&str> = out.split(sep).collect();
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
            !out.contains('\u{1e}'),
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
