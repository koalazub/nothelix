use std::fs;

use super::cells::parse_jl_file;
use super::ipynb::sibling_path;
use super::marker::CellKind;
use crate::error::{Error, Result, ffi};

#[derive(Clone, Copy)]
enum Format {
    Markdown,
    Typst,
}

impl Format {
    const fn extension(self) -> &'static str {
        match self {
            Self::Markdown => ".md",
            Self::Typst => ".typ",
        }
    }

    fn render_prose(self, body: &str) -> Result<String> {
        match self {
            Self::Markdown => Ok(verbatim(body)),
            Self::Typst => {
                let typst = crate::typst_export::md_to_typst(&uncommented_block(body)).map_err(
                    |source| Error::Malformed {
                        subject: "markdown cell",
                        detail: source.to_string(),
                    },
                )?;
                Ok(format!("{typst}\n"))
            }
        }
    }
}

pub fn export_to_markdown(jl_path: String) -> String {
    ffi(export(&jl_path, Format::Markdown))
}

pub fn export_to_typst(jl_path: String) -> String {
    ffi(export(&jl_path, Format::Typst))
}

fn export(jl_path: &str, format: Format) -> Result<String> {
    let (cells, _) = parse_jl_file(jl_path)?;
    let mut out = String::new();
    for cell in &cells {
        match cell.kind {
            CellKind::Markdown | CellKind::Typst => {
                out.push_str(&format.render_prose(&cell.code)?);
            }
            CellKind::Raw => out.push_str(&verbatim(&cell.code)),
            CellKind::Code => {
                if !cell.code.trim().is_empty() {
                    out.push_str(&fenced_julia(&cell.code));
                }
            }
        }
    }

    let out_path = sibling_path(jl_path, format.extension());
    fs::write(&out_path, &out).map_err(|source| Error::writing(&out_path, source))?;
    Ok(format!("Exported to {out_path}"))
}

fn uncomment(line: &str) -> &str {
    line.strip_prefix("# ").unwrap_or(line)
}

fn uncommented_block(code: &str) -> String {
    code.lines().map(uncomment).collect::<Vec<_>>().join("\n")
}

fn verbatim(code: &str) -> String {
    let mut out: String = code
        .lines()
        .flat_map(|line| [uncomment(line), "\n"])
        .collect();
    out.push('\n');
    out
}

fn fenced_julia(code: &str) -> String {
    let newline = if code.ends_with('\n') { "" } else { "\n" };
    format!("```julia\n{code}{newline}```\n\n")
}

#[cfg(test)]
mod tests {
    use super::{export_to_markdown, export_to_typst};

    #[test]
    fn a_directory_named_like_a_notebook_is_not_rewritten() {
        let tmp_dir = tempfile::TempDir::new().unwrap();
        let dir = tmp_dir.path().join("proj.jl");
        std::fs::create_dir(&dir).unwrap();
        let jl_path = dir.join("nb.jl");
        let src = "# ═══ Nothelix Notebook: nb.ipynb ═══\n# Cells: 1\n\n@cell 0 :julia\nx = 1\n";
        std::fs::write(&jl_path, src).unwrap();

        let md = export_to_markdown(jl_path.to_string_lossy().into());
        assert_eq!(md, format!("Exported to {}", dir.join("nb.md").display()));
        assert!(dir.join("nb.md").exists());

        let typ = export_to_typst(jl_path.to_string_lossy().into());
        assert_eq!(typ, format!("Exported to {}", dir.join("nb.typ").display()));
        assert!(dir.join("nb.typ").exists());
    }
}
