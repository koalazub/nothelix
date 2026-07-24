use super::super::hints::ErrorHint;
use super::super::matching::{expand_template, find_hint};
use super::super::text::{clean_message, first_line, truncate};
use super::super::tokenize::tokenize_error;
use super::super::types::StructuredError;
use super::super::{call_chain, enrichment};
use super::report::{Headline, Report};
use super::undefined::format_undefined;
use super::var_context::write_var_context;

const HEADLINE_WIDTH: usize = 80;
const QUOTED_WIDTH: usize = 120;

pub fn format_structured(
    err: &StructuredError,
    hints: &[ErrorHint],
    kernel_dir: Option<&str>,
) -> String {
    if err.error_type == "UndefVarError" && !err.undef_guidance.is_empty() {
        return format_undefined(err);
    }

    let tokens = tokenize_error(&err.error_type, clean_message(&err.message));
    let matched = find_hint(hints, &tokens);
    let mut report = Report::default();

    match matched {
        Some(hint) => report.headline(&Headline::Coded {
            code: &hint.id,
            title: &expand_template(&hint.title, &tokens),
        }),
        None => report.headline(&Headline::Coded {
            code: &err.error_type,
            title: &truncate(first_line(&err.message), HEADLINE_WIDTH),
        }),
    }
    report.cell_arrow(err.cell_index, err.cell_line);

    report.gutter();
    report.source_frame(&err.source_line, err.cell_line);
    report.quoted(&truncate(first_line(&err.message), QUOTED_WIDTH));
    report.gutter();

    if let Some(hint) = matched {
        if !hint.help.is_empty() {
            report.wrapped_help(&expand_template(&hint.help, &tokens));
        }
        if !hint.example.is_empty() {
            report.example(&expand_template(&hint.example, &tokens));
        }
        if hint.note_kernel_dir
            && let Some(dir) = kernel_dir
        {
            report.note(&format!("the kernel looked in {dir}"));
        }
    }

    if let Some(enriched) = enrichment::source_context(err) {
        report.gutter();
        report.block(&enriched);
    }

    if !err.cell_context.is_empty() {
        report.gutter();
        for (var, ctx) in &err.cell_context {
            write_var_context(&mut report, var, ctx, err.cell_index);
        }
    }

    if !err.unexecuted_deps.is_empty() {
        report.gutter();
        let cells: Vec<String> = err
            .unexecuted_deps
            .iter()
            .map(|cell| format!("@cell {cell}"))
            .collect();
        report.note(&format!(
            "this cell depends on {} which haven't been executed",
            cells.join(", ")
        ));
    }

    let chain = call_chain::build(&err.frames);
    if !chain.is_empty() {
        report.gutter();
        report.call_chain(&chain);
    }

    report.finish()
}
