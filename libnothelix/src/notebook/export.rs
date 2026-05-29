//! Markdown / Typst export of a parsed `.jl` notebook.
//!
//! Both formats render the notebook as a sequence of fenced code
//! blocks (for code cells) interleaved with the markdown body (for
//! markdown cells). The Typst output additionally wraps each section
//! in `#code-cell[…]` / `#markdown-cell[…]` macros that the bundled
//! template understands.

// Steel's `register_fn` marshals values from the Steel VM and requires
// the registered fn's signature to take owned types (`String`), not
// borrows.
#![allow(clippy::needless_pass_by_value)]

use std::fs;

use super::cells::{jl_sibling_path, parse_jl_file, CellKind};

pub fn export_to_markdown(jl_path: String) -> String {
    let (cells, _) = match parse_jl_file(&jl_path) {
        Err(e) => return format!("ERROR: {e}"),
        Ok(v) => v,
    };

    let mut out = String::new();

    for cell in &cells {
        match cell.kind {
            // Raw cells pass through verbatim — nbformat defines them
            // as already being in the target format.
            CellKind::Markdown | CellKind::Typst | CellKind::Raw => {
                for line in cell.code.lines() {
                    out.push_str(line.strip_prefix("# ").unwrap_or(line));
                    out.push('\n');
                }
                out.push('\n');
            }
            CellKind::Code => {
                if cell.code.trim().is_empty() {
                    continue;
                }
                out.push_str("```julia\n");
                out.push_str(&cell.code);
                if !cell.code.ends_with('\n') {
                    out.push('\n');
                }
                out.push_str("```\n\n");
            }
        }
    }

    let out_path = jl_sibling_path(&jl_path, ".md");
    match fs::write(&out_path, &out) {
        Ok(_) => format!("Exported to {out_path}"),
        Err(e) => format!("ERROR: Cannot write {out_path}: {e}"),
    }
}

/// Export a `.jl` notebook to Typst (`.typ`).
pub fn export_to_typst(jl_path: String) -> String {
    let (cells, _) = match parse_jl_file(&jl_path) {
        Err(e) => return format!("ERROR: {e}"),
        Ok(v) => v,
    };

    let mut out = String::new();

    for cell in &cells {
        match cell.kind {
            CellKind::Markdown | CellKind::Typst => {
                let stripped: String = cell
                    .code
                    .lines()
                    .map(|l| l.strip_prefix("# ").unwrap_or(l))
                    .collect::<Vec<_>>()
                    .join("\n");
                out.push_str(&crate::typst_export::md_to_typst(&stripped));
                out.push('\n');
            }
            // Raw cells pass through without markdown→typst rewriting —
            // nbformat defines them as already being in the target format.
            CellKind::Raw => {
                for line in cell.code.lines() {
                    out.push_str(line.strip_prefix("# ").unwrap_or(line));
                    out.push('\n');
                }
                out.push('\n');
            }
            CellKind::Code => {
                if cell.code.trim().is_empty() {
                    continue;
                }
                out.push_str("```julia\n");
                out.push_str(&cell.code);
                if !cell.code.ends_with('\n') {
                    out.push('\n');
                }
                out.push_str("```\n\n");
            }
        }
    }

    let out_path = jl_sibling_path(&jl_path, ".typ");
    match fs::write(&out_path, &out) {
        Ok(_) => format!("Exported to {out_path}"),
        Err(e) => format!("ERROR: Cannot write {out_path}: {e}"),
    }
}
