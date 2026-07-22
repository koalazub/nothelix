mod bounds;
mod dimension;
mod method_error;
mod not_callable;
mod parse_error;
mod var_name;

use super::FormatContext;
use super::types::StructuredError;

pub(super) use parse_error::scan_error_location;
use var_name::extract_var_name;

trait Enricher: Sync {
    fn enrich(&self, err: &mut StructuredError, ctx: &FormatContext<'_>);
}

#[cfg(feature = "native")]
static ENRICHERS: &[&dyn Enricher] = &[&CrossCellUndefinedScan];

#[cfg(not(feature = "native"))]
static ENRICHERS: &[&dyn Enricher] = &[];

pub(super) fn apply(err: &mut StructuredError, ctx: &FormatContext<'_>) {
    for enricher in ENRICHERS {
        enricher.enrich(err, ctx);
    }
}

pub(super) fn source_context(err: &StructuredError) -> Option<String> {
    let source = err.source_line.trim();
    if source.is_empty() {
        return None;
    }
    match err.error_type.as_str() {
        "DimensionMismatch" => dimension::enrich(&err.message, source),
        "BoundsError" => bounds::enrich(&err.message, source),
        "MethodError" => method_error::enrich(&err.message, source, err),
        "ParseError" | "Meta.ParseError" => parse_error::enrich(&err.message, source),
        _ => None,
    }
}

#[cfg(feature = "native")]
struct CrossCellUndefinedScan;

#[cfg(feature = "native")]
impl Enricher for CrossCellUndefinedScan {
    fn enrich(&self, err: &mut StructuredError, ctx: &FormatContext<'_>) {
        if err.error_type != "UndefVarError" || !err.cell_context.is_empty() {
            return;
        }
        let Some(path) = ctx.notebook_path.filter(|path| !path.is_empty()) else {
            return;
        };
        let cells = crate::notebook::scan_code_cells(path);
        let guidance = super::undef::build(&cells, err.cell_index, &err.message);
        if guidance.lines.is_empty() {
            return;
        }
        err.undef_symbols = guidance.symbols;
        err.undef_guidance = guidance.lines;
    }
}
