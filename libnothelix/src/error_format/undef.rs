//! Cross-cell guidance for `UndefVarError`.
//!
//! When a cell references a symbol the kernel says is undefined, the
//! useful question is never "is it spelled right" — it's "where does
//! this live and what do I have to run". This module answers that by a
//! purely static scan of the notebook's other cells:
//!
//!   * the missing symbol(s) are parsed out of the Julia error message
//!     (token walk, no regex),
//!   * a package hint ("… also exists in `LinearAlgebra`") is parsed the
//!     same way,
//!   * each symbol is looked up across every sibling cell's source for a
//!     binding (assignment, `const`, `function`, short-form def,
//!     `struct`, `macro`, or an explicit `using … : name` import),
//!   * failing that, the package hint routes the user to the cell that
//!     imports the package — or tells them to add the import.
//!
//! The output is one guidance line per undefined symbol, in the order
//! the symbols appear in the error.

use super::scanners::{find_matching_paren, is_identifier};
use crate::notebook::ScanCell;

/// The result of the cross-cell scan: the symbols the error named and a
/// human-readable guidance line for each, aligned by index.
pub(super) struct UndefGuidance {
    pub symbols: Vec<String>,
    pub lines: Vec<String>,
}

/// Build guidance for every undefined symbol in `message`, scanning
/// `cells` (all sibling cells, code only) for where each lives.
/// `error_cell` is the index of the cell that raised the error, so a
/// binding in that same cell is not offered as the fix.
pub(super) fn build(cells: &[ScanCell], error_cell: i64, message: &str) -> UndefGuidance {
    let symbols = parse_undef_symbols(message);
    let pkg_hint = parse_package_hint(message);

    let lines = symbols
        .iter()
        .map(|sym| guidance_for(cells, error_cell, sym, pkg_hint.as_deref()))
        .collect();

    UndefGuidance { symbols, lines }
}

fn guidance_for(cells: &[ScanCell], error_cell: i64, sym: &str, pkg_hint: Option<&str>) -> String {
    if let Some(cell) = lowest_defining_cell(cells, error_cell, sym) {
        let label = label_suffix(&cell.label);
        return if cell.index > error_cell {
            format!(
                "`{sym}` is defined below in @cell {}{label} — move it above @cell {error_cell} and run it, or run @cell {} first",
                cell.index, cell.index
            )
        } else {
            format!(
                "`{sym}` is defined in @cell {}{label} — run @cell {} first, or run every cell above this one",
                cell.index, cell.index
            )
        };
    }

    if let Some(pkg) = pkg_hint {
        if let Some(cell) = lowest_importing_cell(cells, pkg) {
            return format!(
                "`{sym}` comes from `{pkg}` — run @cell {} (`using {pkg}`) first",
                cell.index
            );
        }
        return format!(
            "`{sym}` comes from `{pkg}` — add `using {pkg}` to an earlier cell and run it"
        );
    }

    format!("`{sym}` isn't defined in any cell — check the spelling, or define it before using it")
}

// ─── Error-message parsing (token walk, no regex) ─────────────────────────────

/// Pull every undefined symbol out of a Julia `UndefVarError` message.
/// Matches the `<ident> not defined` shape in token space, so both the
/// legacy `UndefVarError: x not defined` and the current
/// `UndefVarError: \`x\` not defined in \`Main\`` forms resolve to `x`,
/// and a message naming several symbols yields all of them in order.
pub(super) fn parse_undef_symbols(message: &str) -> Vec<String> {
    let toks: Vec<&str> = message.split_whitespace().collect();
    let mut out: Vec<String> = Vec::new();
    for w in toks.windows(3) {
        if w[1] == "not" && clean_token(w[2]) == "defined" {
            let cand = clean_token(w[0]);
            if is_identifier(cand) && !out.iter().any(|s| s == cand) {
                out.push(cand.to_string());
            }
        }
    }
    out
}

/// Parse the "a global variable of this name also exists in `PKG`" hint
/// Julia appends when the missing name shadows a stdlib/package export.
pub(super) fn parse_package_hint(message: &str) -> Option<String> {
    let toks: Vec<&str> = message.split_whitespace().collect();
    for w in toks.windows(3) {
        if w[0] == "exists" && w[1] == "in" {
            let cand = clean_token(w[2]);
            if is_identifier(cand) {
                return Some(cand.to_string());
            }
        }
    }
    None
}

/// Strip the punctuation Julia wraps identifiers in — backticks, quotes,
/// and trailing sentence punctuation — leaving the bare token.
fn clean_token(tok: &str) -> &str {
    tok.trim_matches(|c: char| {
        c == '`' || c == '\'' || c == '"' || c == '.' || c == ',' || c == ':' || c == ';'
    })
}

// ─── Cross-cell definition + import scanning ──────────────────────────────────

fn lowest_defining_cell<'a>(
    cells: &'a [ScanCell],
    error_cell: i64,
    sym: &str,
) -> Option<&'a ScanCell> {
    cells
        .iter()
        .filter(|c| c.index != error_cell && cell_defines(&c.code, sym))
        .min_by_key(|c| c.index)
}

fn lowest_importing_cell<'a>(cells: &'a [ScanCell], pkg: &str) -> Option<&'a ScanCell> {
    cells
        .iter()
        .filter(|c| cell_imports(&c.code, pkg))
        .min_by_key(|c| c.index)
}

/// Does any line in `code` bind `sym`? Recognises assignment, compound
/// assignment, short-form + long-form function defs, `const`, `struct`
/// / `mutable struct`, `macro`, and an explicit `using/import … : sym`.
fn cell_defines(code: &str, sym: &str) -> bool {
    code.lines().any(|raw| {
        let line = strip_comment(raw);
        line_defines(line, sym)
    })
}

fn line_defines(line: &str, sym: &str) -> bool {
    let words: Vec<&str> = line.split_whitespace().collect();
    match words.as_slice() {
        ["const", name, ..] if ident_head(name) == sym => return true,
        ["global", name, ..] if ident_head(name) == sym => return true,
        ["local", name, ..] if ident_head(name) == sym => return true,
        ["function", name, ..] if ident_head(name) == sym => return true,
        ["macro", name, ..] if ident_head(name) == sym => return true,
        ["struct", name, ..] if ident_head(name) == sym => return true,
        ["abstract", "type", name, ..] if ident_head(name) == sym => return true,
        ["primitive", "type", name, ..] if ident_head(name) == sym => return true,
        ["mutable", "struct", name, ..] if ident_head(name) == sym => return true,
        ["using", ..] | ["import", ..] => return import_list_binds(line, sym),
        _ => {}
    }
    line_binds(line, sym)
}

/// Leading-token assignment / short-function-def check. `sym` must be
/// the first token on the line, followed (past an optional argument
/// paren group) by a single `=` or a compound `⊕=`, never `==`.
fn line_binds(line: &str, sym: &str) -> bool {
    let b = line.as_bytes();
    let s = sym.as_bytes();
    let start = b.iter().take_while(|c| c.is_ascii_whitespace()).count();
    if start + s.len() > b.len() || &b[start..start + s.len()] != s {
        return false;
    }
    let mut j = start + s.len();
    if j < b.len() && is_ident_byte(b[j]) {
        return false;
    }
    while j < b.len() && (b[j] == b' ' || b[j] == b'\t') {
        j += 1;
    }
    if j < b.len() && b[j] == b'(' {
        match find_matching_paren(b, j) {
            Some(close) => j = close + 1,
            None => return false,
        }
        while j < b.len() && (b[j] == b' ' || b[j] == b'\t') {
            j += 1;
        }
    }
    if j >= b.len() {
        return false;
    }
    if b[j] == b'=' {
        return b.get(j + 1) != Some(&b'=');
    }
    matches!(b[j], b'+' | b'-' | b'*' | b'/' | b'^' | b'%') && b.get(j + 1) == Some(&b'=')
}

/// True when `line` is a `using`/`import` whose explicit name list
/// (after the `:`) contains `sym` — e.g. `using LinearAlgebra: eigen`.
fn import_list_binds(line: &str, sym: &str) -> bool {
    let Some(colon) = line.find(':') else {
        return false;
    };
    line[colon + 1..].split(',').any(|item| {
        let name = item.split_whitespace().next().unwrap_or("");
        ident_head(name) == sym
    })
}

/// Does `code` load `pkg` via `using pkg` / `import pkg` (in any position
/// of a comma list, and regardless of a trailing `: names` selector)?
fn cell_imports(code: &str, pkg: &str) -> bool {
    code.lines().any(|raw| {
        let line = strip_comment(raw);
        let mut words = line.split_whitespace();
        let Some(kw) = words.next() else {
            return false;
        };
        if kw != "using" && kw != "import" {
            return false;
        }
        let rest = line[kw.len()..].trim();
        let modules = rest.split(':').next().unwrap_or("");
        modules.split(',').any(|m| {
            let head = m.trim().split('.').next().unwrap_or("").trim();
            head == pkg
        })
    })
}

// ─── Small shared helpers ─────────────────────────────────────────────────────

/// Leading identifier of a token: `foo(x,` → `foo`, `Bar{T}` → `Bar`.
fn ident_head(tok: &str) -> &str {
    let end = tok.bytes().take_while(|&c| is_ident_byte(c)).count();
    &tok[..end]
}

fn is_ident_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_' || b == b'!'
}

/// Strip an inline `# comment` tail (best-effort — cuts at the first
/// `#`, matching the rest of the static scanners).
fn strip_comment(line: &str) -> &str {
    match line.find('#') {
        Some(pos) => &line[..pos],
        None => line,
    }
}

/// Render a cell's marker label as a parenthesised suffix, or nothing.
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
    fn parse_symbols_modern_form() {
        let msg = "UndefVarError: `eigen` not defined in `Main`";
        assert_eq!(parse_undef_symbols(msg), vec!["eigen"]);
    }

    #[test]
    fn parse_symbols_legacy_form() {
        assert_eq!(
            parse_undef_symbols("UndefVarError: myvar not defined"),
            vec!["myvar"]
        );
    }

    #[test]
    fn parse_symbols_multiple_in_order() {
        let msg = "`eigen` not defined. also `A` not defined";
        assert_eq!(parse_undef_symbols(msg), vec!["eigen", "A"]);
    }

    #[test]
    fn parse_package_hint_extracts_pkg() {
        let msg = "UndefVarError: `eigen` not defined in `Main`. Hint: a global \
                   variable of this name also exists in LinearAlgebra.";
        assert_eq!(parse_package_hint(msg).as_deref(), Some("LinearAlgebra"));
    }

    #[test]
    fn defined_by_assignment_points_at_lowest_cell() {
        let cells = vec![cell(1, "A = rand(3, 3)"), cell(2, "B = A * A")];
        let g = build(&cells, 70, "UndefVarError: `A` not defined");
        assert_eq!(g.lines.len(), 1);
        assert!(g.lines[0].contains("@cell 1 first"), "{}", g.lines[0]);
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
            let g = build(&cells, 9, "UndefVarError: `K` not defined");
            assert!(
                g.lines[0].contains("@cell 5"),
                "form did not resolve: {code:?} -> {}",
                g.lines[0]
            );
        }
    }

    #[test]
    fn package_symbol_with_import_cell_points_at_it() {
        let cells = vec![cell(3, "using LinearAlgebra"), cell(4, "x = 1")];
        let msg = "UndefVarError: `eigen` not defined. also exists in LinearAlgebra";
        let g = build(&cells, 70, msg);
        assert!(
            g.lines[0].contains("comes from `LinearAlgebra`") && g.lines[0].contains("@cell 3"),
            "{}",
            g.lines[0]
        );
    }

    #[test]
    fn package_symbol_without_import_suggests_adding_it() {
        let cells = vec![cell(1, "x = 1")];
        let msg = "UndefVarError: `eigen` not defined. also exists in LinearAlgebra";
        let g = build(&cells, 70, msg);
        assert!(
            g.lines[0].contains("add `using LinearAlgebra`"),
            "{}",
            g.lines[0]
        );
    }

    #[test]
    fn two_symbols_yield_two_lines_in_order() {
        let cells = vec![cell(2, "using LinearAlgebra"), cell(65, "A = rand(3, 3)")];
        let msg = "UndefVarError: `eigen` not defined. `A` not defined. \
                   a global variable of this name also exists in LinearAlgebra";
        let g = build(&cells, 70, msg);
        assert_eq!(g.symbols, vec!["eigen", "A"]);
        assert!(
            g.lines[0].contains("comes from `LinearAlgebra`"),
            "{}",
            g.lines[0]
        );
        assert!(
            g.lines[1].contains("`A` is defined in @cell 65"),
            "{}",
            g.lines[1]
        );
    }

    #[test]
    fn symbol_found_nowhere_falls_back_gracefully() {
        let cells = vec![cell(1, "y = 2")];
        let g = build(&cells, 3, "UndefVarError: `ghost` not defined");
        assert_eq!(g.lines.len(), 1);
        assert!(
            g.lines[0].contains("isn't defined in any cell"),
            "{}",
            g.lines[0]
        );
    }

    #[test]
    fn definition_in_error_cell_is_not_offered() {
        let cells = vec![cell(3, "A = 1")];
        let g = build(&cells, 3, "UndefVarError: `A` not defined");
        assert!(
            g.lines[0].contains("isn't defined in any cell"),
            "{}",
            g.lines[0]
        );
    }

    #[test]
    fn later_cell_definition_says_move_up() {
        let cells = vec![cell(80, "A = 1")];
        let g = build(&cells, 70, "UndefVarError: `A` not defined");
        assert!(
            g.lines[0].contains("defined below in @cell 80")
                && g.lines[0].contains("move it above"),
            "{}",
            g.lines[0]
        );
    }

    #[test]
    fn marker_label_is_shown() {
        let cells = vec![ScanCell {
            index: 12,
            code: "A = 1".into(),
            label: "# Setup".into(),
        }];
        let g = build(&cells, 70, "UndefVarError: `A` not defined");
        assert!(g.lines[0].contains("@cell 12 (Setup)"), "{}", g.lines[0]);
    }

    #[test]
    fn equality_is_not_mistaken_for_binding() {
        let cells = vec![cell(1, "if A == 3\nend")];
        let g = build(&cells, 3, "UndefVarError: `A` not defined");
        assert!(
            g.lines[0].contains("isn't defined in any cell"),
            "{}",
            g.lines[0]
        );
    }
}
