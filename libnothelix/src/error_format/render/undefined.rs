use super::super::types::StructuredError;
use super::report::{Headline, Report};

pub fn format_undefined(err: &StructuredError) -> String {
    let mut report = Report::default();
    report.headline(&Headline::Coded {
        code: "E004",
        title: &format!("{} {}", named_symbols(err), agreement(err)),
    });
    report.cell_arrow(err.cell_index, err.cell_line);

    if !err.source_line.is_empty() && err.cell_line > 0 {
        report.gutter();
        report.source_frame(&err.source_line, err.cell_line);
    }

    report.gutter();
    for line in &err.undef_guidance {
        report.guidance(line);
    }

    if let Some(package) = &err.exporting_package
        && let Some(symbol) = err.undef_symbols.first()
    {
        report.gutter();
        report.guidance(&format!(
            "`{symbol}` is exported by `{package}` — add `using {package}` to an earlier cell and run it"
        ));
    }

    report.finish()
}

fn named_symbols(err: &StructuredError) -> String {
    if err.undef_symbols.is_empty() {
        return "a variable".to_string();
    }
    err.undef_symbols
        .iter()
        .map(|symbol| format!("`{symbol}`"))
        .collect::<Vec<_>>()
        .join(", ")
}

fn agreement(err: &StructuredError) -> &'static str {
    if err.undef_symbols.len() > 1 {
        "are not defined"
    } else {
        "is not defined"
    }
}
