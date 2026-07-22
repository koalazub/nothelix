use std::fs;

use super::ipynb::sibling_path;
use super::marker::{CellKind, Marker};
use crate::error::{Error, Result};

const HEADER_PREFIX: &str = "# ═══ Nothelix Notebook: ";
const HEADER_SUFFIX: &str = " ═══";
const OUTPUT_OPEN: &str = "# ─── Output";
const OUTPUT_CLOSE: &str = "# ─────────────";
const IMAGE_MARKER: &str = "# @image ";
const PREAMBLE_INDEX: isize = -1;
const RETIRED_MACRO_PACKAGE: &str = "NothelixMacros";

pub(super) struct JlCell {
    pub index: isize,
    pub kind: CellKind,
    pub code: String,
    pub start_line: usize,
    pub marker_comment: String,
    pub images: Vec<String>,
}

impl JlCell {
    fn opened_by(marker: Marker, start_line: usize) -> Self {
        Self {
            index: marker.index,
            kind: marker.kind,
            code: String::new(),
            start_line,
            marker_comment: marker.comment,
            images: Vec::new(),
        }
    }
}

pub(super) fn parse_jl_file(jl_path: &str) -> Result<(Vec<JlCell>, String)> {
    let content = fs::read_to_string(jl_path).map_err(|source| Error::reading(jl_path, source))?;
    let lines: Vec<&str> = content.lines().collect();
    let source_path =
        declared_source_path(&lines).unwrap_or_else(|| sibling_path(jl_path, ".ipynb"));

    let mut cells: Vec<JlCell> = lines
        .iter()
        .enumerate()
        .filter_map(|(at, line)| Marker::parse(line).map(|marker| JlCell::opened_by(marker, at)))
        .collect();

    let first_marker = cells.first().map_or(lines.len(), |cell| cell.start_line);
    if let Some(preamble) = preamble_cell(&lines[..first_marker]) {
        cells.insert(0, preamble);
    }

    for at in 0..cells.len() {
        let from = cells[at].start_line + 1;
        let to = cells
            .get(at + 1)
            .map_or(lines.len(), |cell| cell.start_line);
        let body = CellBody::read(&lines[from..to]);
        cells[at].code = body.code(cells[at].kind, &cells[at].marker_comment);
        cells[at].images = body.images;
    }

    Ok((cells, source_path))
}

fn declared_source_path(lines: &[&str]) -> Option<String> {
    lines
        .iter()
        .find_map(|line| {
            let declared = line.strip_prefix(HEADER_PREFIX)?;
            Some(declared.trim_end_matches(HEADER_SUFFIX).trim().to_string())
        })
        .filter(|path| !path.is_empty())
}

fn preamble_cell(before_first_marker: &[&str]) -> Option<JlCell> {
    let code = before_first_marker
        .iter()
        .copied()
        .filter(|line| is_executable_preamble(line))
        .collect::<Vec<&str>>()
        .join("\n");
    (!code.trim().is_empty()).then(|| JlCell {
        index: PREAMBLE_INDEX,
        kind: CellKind::Code,
        code,
        start_line: 0,
        marker_comment: String::new(),
        images: Vec::new(),
    })
}

fn is_executable_preamble(line: &str) -> bool {
    let trimmed = line.trim();
    !trimmed.is_empty() && !trimmed.starts_with('#') && !loads_retired_macro_package(line)
}

fn loads_retired_macro_package(line: &str) -> bool {
    let code = match line.split_once('#') {
        Some((before_comment, _)) => before_comment,
        None => line,
    }
    .trim();
    ["using ", "import "].into_iter().any(|keyword| {
        code.strip_prefix(keyword)
            .is_some_and(|package| package.trim_end_matches(';').trim() == RETIRED_MACRO_PACKAGE)
    })
}

#[derive(Default)]
struct CellBody<'a> {
    code_lines: Vec<&'a str>,
    images: Vec<String>,
}

impl<'a> CellBody<'a> {
    fn read(lines: &[&'a str]) -> Self {
        let mut body = Self::default();
        let mut inside_output = false;
        for line in lines {
            if line.contains(OUTPUT_OPEN) {
                inside_output = true;
                continue;
            }
            if inside_output {
                inside_output = !line.contains(OUTPUT_CLOSE);
                continue;
            }
            if Marker::parse(line).is_some() || loads_retired_macro_package(line) {
                continue;
            }
            if let Some(declared) = line.strip_prefix(IMAGE_MARKER) {
                let path = declared.trim_end_matches('\r').trim();
                if !path.is_empty() {
                    body.images.push(path.to_string());
                }
                continue;
            }
            body.code_lines.push(line);
        }
        while body
            .code_lines
            .last()
            .is_some_and(|line| line.trim().is_empty())
        {
            body.code_lines.pop();
        }
        body
    }

    fn code(&self, kind: CellKind, marker_comment: &str) -> String {
        let code = self.code_lines.join("\n");
        if marker_comment.is_empty() {
            return code;
        }
        let heading = if kind.is_prose() {
            format!("# {marker_comment}")
        } else {
            marker_comment.to_string()
        };
        if code.is_empty() {
            heading
        } else {
            format!("{heading}\n{code}")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::notebook::fixture;

    fn parse_source(src: &str) -> Vec<JlCell> {
        let dir = tempfile::TempDir::new().unwrap();
        let jl_path = dir.path().join("nb.jl");
        std::fs::write(&jl_path, src).unwrap();
        parse_jl_file(&jl_path.to_string_lossy()).unwrap().0
    }

    #[test]
    fn every_marker_opens_a_cell_carrying_its_own_body() {
        let (cells, source_path) = parse_jl_file(&fixture::path("simple.jl")).unwrap();
        assert_eq!(cells.len(), 4);
        assert!(source_path.ends_with("simple.ipynb"));

        assert_eq!(cells[0].index, 0);
        assert_eq!(cells[0].kind, CellKind::Code);
        assert_eq!(cells[0].code, "using Plots");

        assert_eq!(cells[1].index, 1);
        assert_eq!(cells[1].code, "x = 1:10\ny = x.^2");

        assert_eq!(cells[2].index, 2);
        assert_eq!(cells[2].kind, CellKind::Markdown);

        assert_eq!(cells[3].index, 3);
        assert_eq!(cells[3].code, "plot(x, y)");
    }

    #[test]
    fn preamble_filter_drops_nothelix_macros_pragma() {
        let cells = parse_source(
            "using NothelixMacros  # cell markers for static checking\n\n\
             # ═══ Nothelix Notebook: example.ipynb ═══\n# Cells: 1\n\n\
             @cell 0 :julia\nx = 1\n",
        );
        assert_eq!(
            cells.len(),
            1,
            "should not emit preamble cell for pragma-only preamble, got: {}",
            cells.len()
        );
        assert_eq!(cells[0].index, 0);
        assert_eq!(cells[0].code, "x = 1");
    }

    #[test]
    fn cell_body_drops_nothelix_macros_load() {
        let cells = parse_source(
            "@cell 0 :julia\n\
             import Pkg\n\
             using NothelixMacros\n\
             import NothelixMacros;\n\
             x = 1\n",
        );
        let body = &cells.iter().find(|c| c.index == 0).unwrap().code;
        assert!(
            !body.contains("NothelixMacros"),
            "NothelixMacros load lines must be stripped from the cell body, got:\n{body}"
        );
        assert!(
            body.contains("import Pkg"),
            "real code dropped, got:\n{body}"
        );
        assert!(body.contains("x = 1"), "real code dropped, got:\n{body}");
    }

    #[test]
    fn preamble_filter_keeps_real_user_preamble() {
        let cells = parse_source(
            "using NothelixMacros\nconst MY_CONST = 42\nusing LinearAlgebra\n\n\
             @cell 0 :julia\nA = I\n",
        );
        assert_eq!(
            cells.len(),
            2,
            "expected preamble cell + @cell 0, got {} cells",
            cells.len()
        );
        assert_eq!(cells[0].index, PREAMBLE_INDEX);
        assert!(cells[0].code.contains("const MY_CONST = 42"));
        assert!(cells[0].code.contains("using LinearAlgebra"));
        assert!(
            !cells[0].code.contains("NothelixMacros"),
            "pragma must not leak into preamble cell"
        );
    }

    #[test]
    fn a_bare_cell_marker_is_a_boundary_and_never_reaches_the_kernel() {
        let cells = parse_source(
            "@cell 0:julia\n\nusing DSP\n\n# building a matrix\n\n\
             @cell\n\nA = zeros(8, 8)\n\ndisplay(A)\n",
        );
        assert_eq!(cells.len(), 2, "bare `@cell` must split into its own cell");

        for (i, cell) in cells.iter().enumerate() {
            assert!(
                !cell.code.contains("@cell"),
                "cell {i} still contains @cell: {:?}",
                cell.code
            );
            assert!(
                !cell.code.contains("@markdown"),
                "cell {i} still contains @markdown: {:?}",
                cell.code
            );
        }

        assert!(cells[0].code.contains("using DSP"));
        assert!(cells[1].code.contains("A = zeros(8, 8)"));
        assert!(cells[1].code.contains("display(A)"));
    }
}
