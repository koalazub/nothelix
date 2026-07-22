use std::cmp::Ordering;

use super::super::types::VarContext;
use super::report::Report;

pub fn write_var_context(report: &mut Report, var: &str, ctx: &VarContext, error_cell: i64) {
    match ctx {
        VarContext::StaticSource {
            defined_in_cell,
            line_text,
            ..
        } => {
            report.note(&format!(
                "`{var}` is defined in @cell {defined_in_cell} ({}) — that cell hasn't been executed yet",
                position_of(*defined_in_cell, error_cell)
            ));
            if !line_text.is_empty() {
                report.note(&format!("look for:  {}", line_text.trim()));
            }
            if *defined_in_cell > error_cell {
                report.help(&format!(
                    "move the `{var} = …` line above @cell {error_cell}, or run @cell {defined_in_cell} first"
                ));
            } else {
                report.help(&run_first(*defined_in_cell));
            }
        }
        VarContext::Executed {
            defined_in_cell,
            status,
        } => {
            report.note(&format!(
                "`{var}` is defined in @cell {defined_in_cell} (status: {status})"
            ));
            report.note("the cell ran but the variable may have been overwritten or errored");
        }
        VarContext::PendingRegistered { defined_in_cell } => {
            report.note(&format!(
                "`{var}` is defined in @cell {defined_in_cell} — not yet executed"
            ));
            report.help(&run_first(*defined_in_cell));
        }
    }
}

fn position_of(defined_in_cell: i64, error_cell: i64) -> &'static str {
    match defined_in_cell.cmp(&error_cell) {
        Ordering::Greater => "later in the notebook",
        Ordering::Less => "earlier in the notebook",
        Ordering::Equal => "in this cell",
    }
}

fn run_first(defined_in_cell: i64) -> String {
    format!("run @cell {defined_in_cell} first, or use :execute-cells-above")
}
