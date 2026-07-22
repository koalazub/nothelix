mod bindings;
mod message;

use crate::notebook::ScanCell;

use bindings::{defining_cell, importing_cell};
use message::{shadowed_package, undefined_symbols};

pub(super) struct UndefGuidance {
    pub symbols: Vec<String>,
    pub lines: Vec<String>,
}

pub(super) fn build(cells: &[ScanCell], error_cell: i64, message: &str) -> UndefGuidance {
    let symbols = undefined_symbols(message);
    let package = shadowed_package(message);
    let lines = symbols
        .iter()
        .map(|symbol| guidance_for(cells, error_cell, symbol, package.as_deref()))
        .collect();
    UndefGuidance { symbols, lines }
}

fn guidance_for(
    cells: &[ScanCell],
    error_cell: i64,
    symbol: &str,
    package: Option<&str>,
) -> String {
    if let Some(cell) = defining_cell(cells, error_cell, symbol) {
        let label = label_suffix(&cell.label);
        let index = cell.index;
        return if index > error_cell {
            format!(
                "`{symbol}` is defined below in @cell {index}{label} — move it above @cell {error_cell} and run it, or run @cell {index} first"
            )
        } else {
            format!(
                "`{symbol}` is defined in @cell {index}{label} — run @cell {index} first, or run every cell above this one"
            )
        };
    }

    let Some(package) = package else {
        return format!(
            "`{symbol}` isn't defined in any cell — check the spelling, or define it before using it"
        );
    };

    match importing_cell(cells, package) {
        Some(cell) => format!(
            "`{symbol}` comes from `{package}` — run @cell {} (`using {package}`) first",
            cell.index
        ),
        None => format!(
            "`{symbol}` comes from `{package}` — add `using {package}` to an earlier cell and run it"
        ),
    }
}

fn label_suffix(label: &str) -> String {
    let trimmed = label.trim_start_matches('#').trim();
    if trimmed.is_empty() {
        String::new()
    } else {
        format!(" ({trimmed})")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cell(index: i64, code: &str) -> ScanCell {
        ScanCell {
            index,
            code: code.to_string(),
            label: String::new(),
        }
    }

    #[test]
    fn defined_by_assignment_points_at_lowest_cell() {
        let cells = vec![cell(1, "A = rand(3, 3)"), cell(2, "B = A * A")];
        let guidance = build(&cells, 70, "UndefVarError: `A` not defined");
        assert_eq!(guidance.lines.len(), 1);
        assert!(
            guidance.lines[0].contains("@cell 1 first"),
            "{}",
            guidance.lines[0]
        );
    }

    #[test]
    fn various_definition_forms_are_detected() {
        for code in [
            "const K = 3",
            "function K(x)\n    x\nend",
            "K(x) = x + 1",
            "struct K end",
            "mutable struct K\n    v\nend",
            "macro K(x) end",
            "using LinearAlgebra: K",
        ] {
            let cells = vec![cell(5, code)];
            let guidance = build(&cells, 9, "UndefVarError: `K` not defined");
            assert!(
                guidance.lines[0].contains("@cell 5"),
                "form did not resolve: {code:?} -> {}",
                guidance.lines[0]
            );
        }
    }

    #[test]
    fn package_symbol_with_import_cell_points_at_it() {
        let cells = vec![cell(3, "using LinearAlgebra"), cell(4, "x = 1")];
        let msg = "UndefVarError: `eigen` not defined. also exists in LinearAlgebra";
        let guidance = build(&cells, 70, msg);
        assert!(
            guidance.lines[0].contains("comes from `LinearAlgebra`")
                && guidance.lines[0].contains("@cell 3"),
            "{}",
            guidance.lines[0]
        );
    }

    #[test]
    fn package_symbol_without_import_suggests_adding_it() {
        let cells = vec![cell(1, "x = 1")];
        let msg = "UndefVarError: `eigen` not defined. also exists in LinearAlgebra";
        let guidance = build(&cells, 70, msg);
        assert!(
            guidance.lines[0].contains("add `using LinearAlgebra`"),
            "{}",
            guidance.lines[0]
        );
    }

    #[test]
    fn two_symbols_yield_two_lines_in_order() {
        let cells = vec![cell(2, "using LinearAlgebra"), cell(65, "A = rand(3, 3)")];
        let msg = "UndefVarError: `eigen` not defined. `A` not defined. \
                   a global variable of this name also exists in LinearAlgebra";
        let guidance = build(&cells, 70, msg);
        assert_eq!(guidance.symbols, vec!["eigen", "A"]);
        assert!(
            guidance.lines[0].contains("comes from `LinearAlgebra`"),
            "{}",
            guidance.lines[0]
        );
        assert!(
            guidance.lines[1].contains("`A` is defined in @cell 65"),
            "{}",
            guidance.lines[1]
        );
    }

    #[test]
    fn symbol_found_nowhere_falls_back_gracefully() {
        let cells = vec![cell(1, "y = 2")];
        let guidance = build(&cells, 3, "UndefVarError: `ghost` not defined");
        assert_eq!(guidance.lines.len(), 1);
        assert!(
            guidance.lines[0].contains("isn't defined in any cell"),
            "{}",
            guidance.lines[0]
        );
    }

    #[test]
    fn definition_in_error_cell_is_not_offered() {
        let cells = vec![cell(3, "A = 1")];
        let guidance = build(&cells, 3, "UndefVarError: `A` not defined");
        assert!(
            guidance.lines[0].contains("isn't defined in any cell"),
            "{}",
            guidance.lines[0]
        );
    }

    #[test]
    fn later_cell_definition_says_move_up() {
        let cells = vec![cell(80, "A = 1")];
        let guidance = build(&cells, 70, "UndefVarError: `A` not defined");
        assert!(
            guidance.lines[0].contains("defined below in @cell 80")
                && guidance.lines[0].contains("move it above"),
            "{}",
            guidance.lines[0]
        );
    }

    #[test]
    fn marker_label_is_shown() {
        let cells = vec![ScanCell {
            index: 12,
            code: "A = 1".into(),
            label: "# Setup".into(),
        }];
        let guidance = build(&cells, 70, "UndefVarError: `A` not defined");
        assert!(
            guidance.lines[0].contains("@cell 12 (Setup)"),
            "{}",
            guidance.lines[0]
        );
    }

    #[test]
    fn equality_is_not_mistaken_for_binding() {
        let cells = vec![cell(1, "if A == 3\nend")];
        let guidance = build(&cells, 3, "UndefVarError: `A` not defined");
        assert!(
            guidance.lines[0].contains("isn't defined in any cell"),
            "{}",
            guidance.lines[0]
        );
    }
}
