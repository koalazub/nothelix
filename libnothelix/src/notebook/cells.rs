//! `.jl` cell parser + the `JlCell` type it produces.
//!
//! Notebook `.jl` files use a comment-based marker syntax that this
//! module knows how to walk. Each section is delimited by `@cell`,
//! `@markdown`, `@raw` or `@typst` markers; everything between markers
//! is the cell body. We also strip `# @image <path>` lines from bodies
//! and stash them on the cell so the converter can lift them into
//! `display_data` outputs / `attachments`.

use std::fs;

use serde_json::Value;
// ─── Cell types ───────────────────────────────────────────────────────────────

#[derive(Debug, PartialEq)]
pub enum CellKind {
    Code,
    Markdown,
    Typst,
    /// nbformat `raw` cell — content passes through conversion verbatim
    /// (never executed, never rendered as markdown). Stored in the .jl
    /// with `# `-prefixed lines exactly like markdown bodies.
    Raw,
}

pub struct JlCell {
    pub index: isize,
    pub kind: CellKind,
    pub code: String,
    pub start_line: usize,
    /// Trailing comment from the marker line, e.g. "# Q1" from "@markdown 3 # Q1".
    /// Prepended to cell code during export so it appears in the ipynb.
    pub marker_comment: String,
    /// Paths from `# @image <path>` markers inside this cell's body.
    /// Stripped from `code` (the kernel doesn't need them as literal
    /// comments, and in markdown they'd render as `@image foo.png`
    /// prose), but preserved here so `convert_to_ipynb` can lift them
    /// into portable forms — `display_data` outputs on code cells or
    /// base64 `attachments` on markdown cells.
    pub images: Vec<String>,
}

// ─── Helpers ──────────────────────────────────────────────────────────────────

/// Extract the trailing comment from a marker line's "rest" portion.
/// Input: "3 # Q1" or "5 :julia # answer" or "0" → "# Q1", "# answer", ""
pub(super) fn extract_marker_comment(rest: &str) -> String {
    if let Some(hash_pos) = rest.find(" #") {
        // Everything from " #" onward, trimmed
        let comment = rest[hash_pos + 1..].trim();
        if comment.starts_with('#') {
            return comment.to_string();
        }
    }
    String::new()
}

/// Derive a sibling path by swapping a trailing `.jl` for `new_ext`
/// (`".ipynb"`, `".md"`, …). Only the suffix is swapped — a plain
/// `.replace(".jl", …)` would also rewrite `.jl` occurrences mid-path
/// (`my.jl.backup.jl`, `proj.jl/nb.jl`) and corrupt the destination.
/// Paths without the suffix get the extension appended.
pub(super) fn jl_sibling_path(jl_path: &str, new_ext: &str) -> String {
    match jl_path.strip_suffix(".jl") {
        Some(stem) => format!("{stem}{new_ext}"),
        None => format!("{jl_path}{new_ext}"),
    }
}

/// Read and parse an `.ipynb` file.
pub fn read_notebook(path: &str) -> Result<Value, String> {
    let content = fs::read_to_string(path).map_err(|e| format!("Cannot read {path}: {e}"))?;
    serde_json::from_str(&content).map_err(|e| format!("Invalid JSON in {path}: {e}"))
}

/// Join notebook cell `source` lines into a single `String`.
pub fn source_to_string(source: &Value) -> String {
    match source {
        Value::Array(lines) => {
            let parts = lines.iter().map(|l| l.as_str().unwrap_or(""));
            let mut out = String::with_capacity(parts.clone().map(str::len).sum());
            for part in parts {
                out.push_str(part);
            }
            out
        }
        Value::String(s) => s.clone(),
        _ => String::new(),
    }
}

/// Parse a `.jl` file into a list of cells and the originating `.ipynb` path.
pub fn parse_jl_file(jl_path: &str) -> Result<(Vec<JlCell>, String), String> {
    let content = fs::read_to_string(jl_path).map_err(|e| format!("Cannot read {jl_path}: {e}"))?;
    let lines: Vec<&str> = content.lines().collect();

    // Extract source .ipynb path from header.
    let mut source_path = String::new();
    for line in &lines {
        if let Some(rest) = line.strip_prefix("# ═══ Nothelix Notebook: ") {
            source_path = rest.trim_end_matches(" ═══").trim().to_string();
            break;
        }
    }
    if source_path.is_empty() {
        source_path = jl_sibling_path(jl_path, ".ipynb");
    }

    // Locate cell markers.
    //
    // Match three shapes on each line:
    //   `@cell N :lang`      (full marker, stamp N, record lang)
    //   `@cell :lang`        (nothelix autofill produced it without
    //                         an index yet — accept, assign index 0
    //                         as a placeholder, the renumber pass
    //                         later fixes it)
    //   `@cell`              (bare — user typed it and the autofill
    //                         hasn't fired yet; still a boundary)
    //
    // …and equivalent shapes for `@markdown`. Before we only matched
    // `@cell ` (with trailing space), so bare `@cell` lines slipped
    // through and got shipped into the Julia kernel where they
    // detonated with `MethodError: no method matching var"@cell"`.
    let mut cells: Vec<JlCell> = Vec::new();
    for (i, line) in lines.iter().enumerate() {
        if line.trim_end() == "@cell" {
            cells.push(JlCell {
                index: 0,
                kind: CellKind::Code,
                code: String::new(),
                start_line: i,
                marker_comment: String::new(),
                images: Vec::new(),
            });
        } else if let Some(rest) = line.strip_prefix("@cell ") {
            let rest = rest.trim();
            let first = rest.split_whitespace().next().unwrap_or("");
            let idx: isize = first.parse().unwrap_or(0);
            cells.push(JlCell {
                index: idx,
                kind: CellKind::Code,
                code: String::new(),
                start_line: i,
                marker_comment: extract_marker_comment(rest),
                images: Vec::new(),
            });
        } else if line.trim_end() == "@markdown" {
            cells.push(JlCell {
                index: 0,
                kind: CellKind::Markdown,
                code: String::new(),
                start_line: i,
                marker_comment: String::new(),
                images: Vec::new(),
            });
        } else if let Some(rest) = line.strip_prefix("@markdown ") {
            let first = rest.split_whitespace().next().unwrap_or("");
            let idx: isize = first.parse().unwrap_or(0);
            cells.push(JlCell {
                index: idx,
                kind: CellKind::Markdown,
                code: String::new(),
                start_line: i,
                marker_comment: extract_marker_comment(rest),
                images: Vec::new(),
            });
        } else if line.trim_end() == "@raw" {
            cells.push(JlCell {
                index: 0,
                kind: CellKind::Raw,
                code: String::new(),
                start_line: i,
                marker_comment: String::new(),
                images: Vec::new(),
            });
        } else if let Some(rest) = line.strip_prefix("@raw ") {
            let first = rest.split_whitespace().next().unwrap_or("");
            let idx: isize = first.parse().unwrap_or(0);
            cells.push(JlCell {
                index: idx,
                kind: CellKind::Raw,
                code: String::new(),
                start_line: i,
                marker_comment: extract_marker_comment(rest),
                images: Vec::new(),
            });
        } else if line.trim_end() == "@typst" {
            cells.push(JlCell {
                index: 0,
                kind: CellKind::Typst,
                code: String::new(),
                start_line: i,
                marker_comment: String::new(),
                images: Vec::new(),
            });
        } else if let Some(rest) = line.strip_prefix("@typst ") {
            let first = rest.split_whitespace().next().unwrap_or("");
            let idx: isize = first.parse().unwrap_or(0);
            cells.push(JlCell {
                index: idx,
                kind: CellKind::Typst,
                code: String::new(),
                start_line: i,
                marker_comment: extract_marker_comment(rest),
                images: Vec::new(),
            });
        }
    }

    // If there's non-empty code before the first cell marker, insert
    // an implicit preamble cell at index -1. This handles `using X`
    // lines at the top of the file that need to execute before any cell.
    //
    // `using NothelixMacros` is special-cased out: the converter injects
    // it so Julia's LanguageServer resolves @cell/@markdown macros
    // without false "Missing reference" squiggles, but it's not user
    // code. Letting it become a preamble cell pollutes .ipynb round-
    // trips — the package only exists in nothelix's bootstrap env, so
    // running that cell in stock Julia fails with "Package
    // NothelixMacros not found in current path".
    let first_marker_line = cells.first().map(|c| c.start_line).unwrap_or(lines.len());
    if first_marker_line > 0 {
        let preamble: String = lines[..first_marker_line]
            .iter()
            .copied()
            .filter(|l| {
                let t = l.trim();
                if t.is_empty() || t.starts_with('#') {
                    return false;
                }
                if t == "using NothelixMacros" || t.starts_with("using NothelixMacros ") {
                    return false;
                }
                true
            })
            .collect::<Vec<&str>>()
            .join("\n");
        if !preamble.trim().is_empty() {
            cells.insert(
                0,
                JlCell {
                    index: -1,
                    kind: CellKind::Code,
                    code: preamble,
                    start_line: 0,
                    marker_comment: String::new(),
                    images: Vec::new(),
                },
            );
        }
    }

    // Collect code for each cell (strip output sections *and* any
    // stray marker-shaped lines that slipped into the cell body).
    // The stray-marker strip is a defense against users typing a new
    // `@cell` inside an existing cell without triggering the autofill
    // expansion — without it those lines would be forwarded to the
    // Julia kernel, which would then choke on `@cell` as a malformed
    // macro invocation.
    // Collect boundaries first so we can mutate cells below.
    let boundaries: Vec<(usize, usize)> = cells
        .iter()
        .enumerate()
        .map(|(ci, cell)| {
            let code_start = cell.start_line + 1;
            let code_end = cells.get(ci + 1).map_or(lines.len(), |c| c.start_line);
            (code_start, code_end)
        })
        .collect();

    for (ci, (code_start, code_end)) in boundaries.into_iter().enumerate() {
        let is_marker_line = |line: &str| -> bool {
            let t = line.trim_end();
            t == "@cell"
                || t == "@markdown"
                || t == "@raw"
                || t == "@typst"
                || line.starts_with("@cell ")
                || line.starts_with("@markdown ")
                || line.starts_with("@raw ")
                || line.starts_with("@typst ")
        };

        let mut filtered: Vec<&str> = Vec::new();
        let mut images: Vec<String> = Vec::new();
        let mut in_output = false;
        for line in &lines[code_start..code_end] {
            if line.contains("# ─── Output") {
                in_output = true;
                continue;
            }
            if in_output {
                if line.contains("# ─────────────") {
                    in_output = false;
                }
                continue;
            }
            if is_marker_line(line) {
                continue;
            }
            if let Some(rest) = line.strip_prefix("# @image ") {
                let path = rest.trim_end_matches('\r').trim();
                if !path.is_empty() {
                    images.push(path.to_string());
                }
                continue;
            }
            filtered.push(line);
        }
        cells[ci].images = images;

        // Trim trailing blank lines.
        while filtered
            .last()
            .map(|l: &&str| l.trim().is_empty())
            .unwrap_or(false)
        {
            filtered.pop();
        }

        let mut code = filtered.join("\n");

        // Prepend marker-line comment as the first line of cell content.
        // For "@markdown 3 # Q1", this makes "# Q1" appear as "# # Q1"
        // in the cell body (a markdown heading when the # prefix is stripped).
        if !cells[ci].marker_comment.is_empty() {
            let comment = &cells[ci].marker_comment;
            let prefix_line = if matches!(
                cells[ci].kind,
                CellKind::Markdown | CellKind::Typst | CellKind::Raw
            ) {
                format!("# {comment}")
            } else {
                comment.to_string()
            };
            if code.is_empty() {
                code = prefix_line;
            } else {
                code = format!("{prefix_line}\n{code}");
            }
        }

        cells[ci].code = code;
    }

    Ok((cells, source_path))
}
