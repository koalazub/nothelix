// Steel's `register_fn` marshals values from the Steel VM and requires
// the registered fn's signature to take owned types (`String`), not
// borrows. The owned type is load-bearing for the FFI dispatcher.
#![allow(clippy::needless_pass_by_value)]

//! Multi-line formatter for math environments in Julia notebook comments.
//!
//! When LaTeX block environments (`\begin{cases}...\end{cases}`, `pmatrix`,
//! `aligned`, etc.) appear on a single source line, the conceal layer can't
//! render them across multiple visual lines — overlays substitute graphemes
//! in place, they can't insert line breaks. This module rewrites the actual
//! source so the env spans the lines it semantically should, at which point
//! the existing scanner in `unicode/scanner.rs` renders it cleanly.
//!
//! The rewrite is idempotent: running it twice changes nothing, because any
//! env that's already multi-line is left alone. Only single-line envs get
//! expanded.
//!
//! Julia comment prefix (`# `) is preserved on every emitted line so the
//! output is still a valid Julia notebook comment block.

use crate::math_image::BATCH_SEP;
use crate::unicode::math_regions::find_math_regions;

/// Hard ceiling on reservation lines per block, a backstop against a runaway
/// row count; the effective cap is already the renderer's `*math-image-target-rows*`.
const MAX_RESERVE_LINES: usize = 64;

/// Environments whose rows should live on their own lines.
const BLOCK_ENVS: &[&str] = &[
    "cases", "pmatrix", "bmatrix", "vmatrix", "Vmatrix", "matrix", "aligned", "align", "split",
    "gathered", "gather",
];

/// Format the given document text. Two passes:
///
///   1. For comment lines whose math region contains a single-line block
///      env (`\begin{cases}...\end{cases}` etc.), rewrite the line into a
///      multi-line `$$` block (`rewrite_line`).
///   2. For comment lines *inside* an existing multi-line `$$` block, split
///      the content at natural math boundaries — `\text{...}` annotations,
///      `\\` row separators, and `\begin{env}` / `\end{env}` wrappers —
///      so a single crammed line like `X = \frac{...}{2} \text{ and } Y =
///      \frac{...}{2i}` becomes three comment lines.
///
/// Both passes preserve the Julia `# ` comment prefix and are idempotent.
pub fn format_math(text: String) -> String {
    let mut out = String::new();
    let lines: Vec<&str> = text.split('\n').collect();
    let line_count = lines.len();
    let mut idx = 0;
    while idx < line_count {
        let line = lines[idx];
        // Walk of the document notices "# $$" opener lines and processes
        // the whole block in one shot so each content line can be split.
        if let Some(close_idx) = find_dollar_block(&lines, idx) {
            out.push_str(line);
            out.push('\n');
            // Merge cases/matrix row continuations first so each logical
            // row lives on one physical line before we emit. Without this,
            // `# 0 &` + `# \text{otherwise}` stays on two lines and the
            // overlay scanner draws "otherwise" below the cases fence
            // instead of beside its value — exactly the layout bug that
            // made `X(ω) = { 0, otherwise }` render as three stranded
            // lines instead of a two-row cases cell.
            for inner in join_cases_continuations(&lines[idx + 1..close_idx]) {
                emit_reformatted_block_line(&inner, &mut out);
            }
            // Closing "# $$" line.
            out.push_str(lines[close_idx]);
            if close_idx + 1 < line_count {
                out.push('\n');
            }
            idx = close_idx + 1;
            continue;
        }

        if let Some(rewritten) = rewrite_line(line) {
            out.push_str(&rewritten);
        } else {
            out.push_str(line);
        }
        if idx + 1 < line_count {
            out.push('\n');
        }
        idx += 1;
    }
    out
}

/// The Julia-comment body of a line, with any trailing CR removed. A bare
/// `#` carries an empty body; a `# ` prefix yields the rest. Non-comment
/// lines yield `None`. Block detection routes through this so a bare `#`
/// reservation line (e.g. a save-time trailing-space trim) does not silently
/// terminate a `$$` block the way a literal `"# "` prefix check would.
fn comment_body(line: &str) -> Option<&str> {
    let body = line.trim_end_matches('\r');
    if body == "#" {
        Some("")
    } else {
        body.strip_prefix("# ")
    }
}

/// A comment line whose body is empty or whitespace — renderer-owned
/// reservation space.
fn is_blank_comment_line(line: &str) -> bool {
    matches!(comment_body(line), Some(b) if b.trim().is_empty())
}

/// `"\r"` when the line carries a trailing CR, so emitted lines preserve a
/// document's CRLF endings; `""` otherwise.
fn cr_suffix(line: &str) -> &'static str {
    if line.ends_with('\r') { "\r" } else { "" }
}

/// The inner LaTeX of a single-line block `# $$ … $$`, or `None`. Mirrors the
/// Steel `single-line-block-body` so Rust and the plugin enumerate the same
/// blocks in the same order.
fn single_line_block_body(line: &str) -> Option<String> {
    let body = comment_body(line)?;
    let body = body.trim_end_matches(['#', ' ', '\t']);
    if body.len() > 4 && body.starts_with("$$") && body.ends_with("$$") {
        Some(body[2..body.len() - 2].trim().to_string())
    } else {
        None
    }
}

/// When `lines[start]` is a `# $$` opener, return the index of the matching
/// `# $$` closer. Returns `None` if this isn't an opener or no closer is
/// found inside the contiguous Julia comment block.
fn find_dollar_block(lines: &[&str], start: usize) -> Option<usize> {
    if comment_body(lines.get(start)?)?.trim() != "$$" {
        return None;
    }
    for (j, candidate) in lines.iter().enumerate().skip(start + 1) {
        match comment_body(candidate) {
            Some(b) if b.trim() == "$$" => return Some(j),
            Some(_) => {}
            None => break,
        }
    }
    None
}

/// A display-math block keyed by its opening line: a multi-line `# $$ … # $$`
/// block (carrying its closer line index) or a single-line `# $$ … $$`.
#[derive(Clone, Copy)]
enum BlockKind {
    Multi(usize),
    Single,
}

/// Enumerate every display-math block in `lines` in document order. The render
/// blob (`math_block_latex_batch`) and the reservation rewrite
/// (`reserve_math_lines`) both walk this list, so the k-th rendered result, the
/// k-th spec entry, and the k-th reserved block are guaranteed to line up.
fn enumerate_blocks(lines: &[&str]) -> Vec<(usize, BlockKind)> {
    let mut blocks = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        if let Some(close) = find_dollar_block(lines, i) {
            blocks.push((i, BlockKind::Multi(close)));
            i = close + 1;
        } else if single_line_block_body(lines[i]).is_some() {
            blocks.push((i, BlockKind::Single));
            i += 1;
        } else {
            i += 1;
        }
    }
    blocks
}

/// Interior content lines of a multi-line block with the trailing run of
/// renderer-owned blank lines removed, leaving user content (including any
/// blank line a user placed *between* equations) intact.
fn block_content_range(lines: &[&str], open: usize, close: usize) -> std::ops::Range<usize> {
    let mut last = close;
    while last > open + 1 && is_blank_comment_line(lines[last - 1]) {
        last -= 1;
    }
    (open + 1)..last
}

/// Merge cases/matrix row continuations so each logical row is on one
/// physical line. A line ending in `&` (a cell separator) is the start of
/// a row whose remaining cells live on the next line(s). Split rows break
/// the overlay scanner, which renders per physical line — a row split
/// across lines puts "otherwise" on its own line below the cases fence
/// with no alignment to the value column beside it.
fn join_cases_continuations(lines: &[&str]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for raw in lines {
        let body = raw.trim_end_matches('\r');
        let content = if let Some(c) = body.strip_prefix("# ") {
            c
        } else {
            out.push((*raw).to_string());
            continue;
        };
        let prev_is_continuation = out
            .last()
            .and_then(|p| p.strip_prefix("# "))
            .map(|prev| prev.trim_end().ends_with('&'))
            .unwrap_or(false);
        if prev_is_continuation {
            let prev = out.pop().expect("last() was Some");
            out.push(format!("{} {}", prev.trim_end(), content.trim_start()));
        } else {
            out.push((*raw).to_string());
        }
    }
    out
}

/// Reformat one content line of a `$$` block and push the result into `out`.
/// Non-comment lines are preserved verbatim.
fn emit_reformatted_block_line(line: &str, out: &mut String) {
    let body = line.trim_end_matches('\r');
    let cr = if line.len() > body.len() { "\r" } else { "" };
    let content = if let Some(c) = body.strip_prefix("# ") {
        c
    } else {
        out.push_str(line);
        out.push('\n');
        return;
    };
    if content.trim().is_empty() {
        out.push_str(line);
        out.push('\n');
        return;
    }
    for piece in split_block_content(content) {
        out.push_str("# ");
        out.push_str(&piece);
        out.push_str(cr);
        out.push('\n');
    }
}

/// Split a content line from a `$$` block at natural math boundaries.
/// Keeps `\text{...}` groups on their own line (they're prose annotations
/// between equations), isolates `\begin{env}` / `\end{env}` wrappers, and
/// ends each row of a matrix/cases-style env at its `\\` separator.
fn split_block_content(content: &str) -> Vec<String> {
    let bytes = content.as_bytes();
    let mut pieces = Vec::new();
    let mut cursor = 0;
    let mut i = 0;

    let push = |pieces: &mut Vec<String>, s: &str| {
        let trimmed = s.trim();
        if !trimmed.is_empty() {
            pieces.push(trimmed.to_string());
        }
    };

    while i < bytes.len() {
        // All patterns of interest start with an ASCII `\`. Gating on the
        // byte first keeps us from ever calling `content[i..]` inside a
        // multi-byte codepoint (the content can contain UTF-8 like `≤`,
        // `ω`, `π` directly — a naive slice mid-char panics).
        if bytes[i] == b'\\' {
            // \text{...} — isolate prose annotations on their own line.
            // EXCEPT when it's the right-hand cell of a cases/matrix row
            // (preceded by `&`): there `\text{otherwise}` is the row's
            // condition, semantically part of the same row as its value.
            // Splitting it off would reproduce the cases bug where the
            // condition renders as an orphan line below the fence.
            if content[i..].starts_with("\\text{") {
                let preceding = content[cursor..i].trim_end();
                if preceding.ends_with('&') {
                    // Skip past the \text{...} without emitting a split.
                    let end = match_brace_after(bytes, i + 6);
                    i = end;
                    continue;
                }
                push(&mut pieces, &content[cursor..i]);
                let end = match_brace_after(bytes, i + 6);
                push(&mut pieces, &content[i..end]);
                cursor = end;
                i = end;
                continue;
            }
            // \\ — keep the separator with the preceding row.
            if i + 1 < bytes.len() && bytes[i + 1] == b'\\' {
                push(&mut pieces, &content[cursor..i + 2]);
                cursor = i + 2;
                i += 2;
                continue;
            }
            // \begin{env} / \end{env} — put the wrapper on its own line.
            let is_begin = content[i..].starts_with("\\begin{");
            let is_end = content[i..].starts_with("\\end{");
            if is_begin || is_end {
                push(&mut pieces, &content[cursor..i]);
                let brace_open = i + if is_begin { 7 } else { 5 };
                let close = match_brace_after(bytes, brace_open);
                push(&mut pieces, &content[i..close]);
                cursor = close;
                i = close;
                continue;
            }
        }
        i += 1;
    }
    push(&mut pieces, &content[cursor..]);

    if pieces.is_empty() {
        pieces.push(content.trim().to_string());
    }
    pieces
}

/// Given a position `j` that points at the byte JUST past an opening `{`,
/// return the byte position one past the matching closing `}`. Clamps to
/// the end of input when the braces are unbalanced.
fn match_brace_after(bytes: &[u8], mut j: usize) -> usize {
    let mut depth = 1i32;
    while j < bytes.len() && depth > 0 {
        match bytes[j] {
            b'{' => depth += 1,
            b'}' => depth -= 1,
            _ => {}
        }
        j += 1;
    }
    j
}

/// Attempt to rewrite a single line. Returns `Some(new_text)` when the line
/// contains a single-line block env worth expanding, `None` otherwise.
fn rewrite_line(line: &str) -> Option<String> {
    // Keep the trailing-CR detection out of the match so DOS line endings
    // round-trip unchanged.
    let (line_body, cr) = match line.strip_suffix('\r') {
        Some(body) => (body, "\r"),
        None => (line, ""),
    };

    let comment_prefix = "# ";
    let content = line_body.strip_prefix(comment_prefix)?;

    // Skip if the line already ends in a dangling `\begin{X}` (multi-line
    // env already in progress) — only single-line envs need rewriting.
    let regions = find_math_regions(content);
    if regions.is_empty() {
        return None;
    }

    // Find the first region that contains a block env fully on this line.
    for (region_start, region_end) in regions {
        let math = &content[region_start..region_end];
        if let Some(block) = find_single_line_block_env(math) {
            return Some(emit_multiline_form(
                comment_prefix,
                content,
                region_start,
                region_end,
                math,
                &block,
                cr,
            ));
        }
    }
    None
}

struct BlockEnv<'a> {
    name: &'a str,
    /// Byte range of `\begin{name}` within the math region text.
    begin_start: usize,
    begin_end: usize,
    /// Byte range of `\end{name}` within the math region text.
    end_start: usize,
    end_end: usize,
}

/// Scan a math-region text for a `\begin{X}...\end{X}` pair where `X` is a
/// block env and both delimiters live on this same line (no embedded `\n`
/// between them). Returns the first match in source order.
fn find_single_line_block_env(math: &str) -> Option<BlockEnv<'_>> {
    let bytes = math.as_bytes();
    let mut i = 0;
    while i + 6 < bytes.len() {
        // Gate the slice on the ASCII backslash byte first — `math` can
        // contain multi-byte UTF-8 (≤, ω, π …) and slicing mid-codepoint
        // panics. `\` being ASCII guarantees a char boundary.
        if bytes[i] != b'\\' || !math[i..].starts_with("\\begin{") {
            i += 1;
            continue;
        }
        let begin_start = i;
        let name_start = i + 7; // past "\begin{"
        let name_end = match math[name_start..].find('}') {
            Some(p) => name_start + p,
            None => break,
        };
        let env_name = &math[name_start..name_end];
        let begin_end = name_end + 1;

        if !BLOCK_ENVS.contains(&env_name) {
            i = begin_end;
            continue;
        }

        // Find matching `\end{name}` on the same line (math region is
        // already one source line, so any match is on-line by construction).
        let end_tag = format!("\\end{{{env_name}}}");
        if let Some(p) = math[begin_end..].find(&end_tag) {
            let end_start = begin_end + p;
            let end_end = end_start + end_tag.len();
            return Some(BlockEnv {
                name: env_name,
                begin_start,
                begin_end,
                end_start,
                end_end,
            });
        }
        i = begin_end;
    }
    None
}

/// Emit the multi-line form of a line whose math region contains a block env.
fn emit_multiline_form(
    comment_prefix: &str,
    content: &str,
    region_start: usize,
    region_end: usize,
    math: &str,
    block: &BlockEnv<'_>,
    cr: &str,
) -> String {
    // Prose around the math region (on the original line).
    let region_delim_span = classify_region_delim(content, region_start, region_end);
    let prose_before = content[..region_delim_span.outer_start].trim_end();
    let prose_after = content[region_delim_span.outer_end..].trim_start();

    // Math pieces.
    let math_prefix = math[..block.begin_start].trim();
    let math_suffix = math[block.end_end..].trim();
    let env_body = &math[block.begin_end..block.end_start];

    let mut out = String::new();
    let mut push_line = |s: &str| {
        out.push_str(comment_prefix);
        out.push_str(s);
        out.push_str(cr);
        out.push('\n');
    };

    if !prose_before.is_empty() {
        push_line(prose_before);
    }
    // Always use `$$` for the block form so the existing multi-line
    // concealer in `conceal.rs::compute_conceal_overlays_for_comments_with_options`
    // picks it up.
    push_line("$$");
    if !math_prefix.is_empty() {
        push_line(math_prefix);
    }
    push_line(&format!("\\begin{{{}}}", block.name));
    for row in split_rows(env_body) {
        let trimmed = row.trim();
        if trimmed.is_empty() {
            continue;
        }
        push_line(trimmed);
    }
    push_line(&format!("\\end{{{}}}", block.name));
    if !math_suffix.is_empty() {
        push_line(math_suffix);
    }
    push_line("$$");
    if !prose_after.is_empty() {
        push_line(prose_after);
    }

    // Strip the final newline we appended via `push_line` on the last
    // emission — `format_math` adds the separator between lines itself.
    if out.ends_with('\n') {
        out.pop();
        if out.ends_with('\r') {
            out.pop();
        }
    }
    out
}

/// Split an env body into rows on `\\` delimiters. Keeps the `\\` suffix on
/// each row except the last so the rendered output matches canonical LaTeX
/// conventions and round-trips cleanly if rewritten a second time.
fn split_rows(body: &str) -> Vec<String> {
    let mut rows = Vec::new();
    let mut cursor = 0;
    let bytes = body.as_bytes();
    let mut i = 0;
    while i + 1 < bytes.len() {
        if bytes[i] == b'\\' && bytes[i + 1] == b'\\' {
            rows.push(format!("{} \\\\", body[cursor..i].trim()));
            i += 2;
            cursor = i;
            continue;
        }
        i += 1;
    }
    rows.push(body[cursor..].trim().to_string());
    rows
}

struct RegionDelimSpan {
    outer_start: usize,
    outer_end: usize,
}

/// Given a region's inner byte range (content only), return the outer byte
/// range including the opening/closing delimiters (`$`, `$$`, `\(`/`\)`,
/// `\[`/`\]`). Conservative: assumes the delimiter bytes are right next to
/// the content, which is what `find_math_regions` guarantees.
fn classify_region_delim(content: &str, region_start: usize, region_end: usize) -> RegionDelimSpan {
    let bytes = content.as_bytes();
    let outer_start = if region_start >= 2
        && (&bytes[region_start - 2..region_start] == b"\\("
            || &bytes[region_start - 2..region_start] == b"$$")
    {
        region_start - 2
    } else if region_start >= 1 && bytes[region_start - 1] == b'$' {
        region_start - 1
    } else {
        region_start
    };

    let outer_end = if region_end + 1 < bytes.len()
        && (&bytes[region_end..region_end + 2] == b"\\)"
            || &bytes[region_end..region_end + 2] == b"$$")
    {
        region_end + 2
    } else if region_end < bytes.len() && bytes[region_end] == b'$' {
        region_end + 1
    } else {
        region_end
    };

    RegionDelimSpan {
        outer_start,
        outer_end,
    }
}

/// The LaTeX of every display-math block, `BATCH_SEP`-joined in document
/// order, for the async renderer. The buffer is canonicalised with
/// `format_math` first and each block's trailing renderer-owned blank run is
/// excluded, so a block's LaTeX — and therefore its measured natural height —
/// is invariant to how much reservation padding it currently carries. That
/// invariance is what makes the reserve cycle converge. Returns `""` when the
/// document has no display math.
pub fn math_block_latex_batch(text: String) -> String {
    let formatted = format_math(text);
    let lines: Vec<&str> = formatted.split('\n').collect();
    let latex: Vec<String> = enumerate_blocks(&lines)
        .iter()
        .map(|(open, kind)| match *kind {
            BlockKind::Multi(close) => block_content_range(&lines, *open, close)
                .map(|k| comment_body(lines[k]).unwrap_or(""))
                .collect::<Vec<_>>()
                .join("\n"),
            BlockKind::Single => single_line_block_body(lines[*open]).unwrap_or_default(),
        })
        .collect();
    latex.join(&BATCH_SEP.to_string())
}

/// Rewrite the document so each display-math block reserves real blank lines
/// equal to its rendered image height. `spec` is a comma-separated list of
/// natural row counts, one per block in `enumerate_blocks` order; a `0` (render
/// error or parse failure) leaves that block untouched. The buffer is
/// canonicalised with `format_math`, every block's trailing renderer-owned
/// blank run is stripped, single-line blocks are expanded to multi-line, and
/// `nat - 2` interior lines are repadded. Emit and strip share one empty-body
/// predicate, so the transform is its own fixed point: re-running it with the
/// same spec is a no-op.
pub fn reserve_math_lines(text: String, spec: String) -> String {
    let formatted = format_math(text);
    let specs: Vec<usize> = spec
        .split(',')
        .map(|s| s.trim().parse().unwrap_or(0))
        .collect();
    let lines: Vec<&str> = formatted.split('\n').collect();
    let blocks = enumerate_blocks(&lines);

    let mut out: Vec<String> = Vec::with_capacity(lines.len() + blocks.len());
    let mut bi = 0;
    let mut idx = 0;
    while idx < lines.len() {
        if bi < blocks.len() && blocks[bi].0 == idx {
            let (open, kind) = blocks[bi];
            let nat = specs.get(bi).copied().unwrap_or(0);
            let cr = cr_suffix(lines[open]);
            match kind {
                BlockKind::Multi(close) => {
                    if nat == 0 {
                        for line in &lines[open..=close] {
                            out.push((*line).to_string());
                        }
                    } else {
                        out.push(lines[open].to_string());
                        let content = block_content_range(&lines, open, close);
                        let content_count = content.len();
                        for k in content {
                            out.push(lines[k].to_string());
                        }
                        push_reservation(&mut out, nat, content_count, cr);
                        out.push(lines[close].to_string());
                    }
                    idx = close + 1;
                }
                BlockKind::Single => {
                    if nat == 0 {
                        out.push(lines[open].to_string());
                    } else {
                        let inner = single_line_block_body(lines[open]).unwrap_or_default();
                        out.push(format!("# $${cr}"));
                        let content_count = if inner.is_empty() {
                            0
                        } else {
                            out.push(format!("# {inner}{cr}"));
                            1
                        };
                        push_reservation(&mut out, nat, content_count, cr);
                        out.push(format!("# $${cr}"));
                    }
                    idx = open + 1;
                }
            }
            bi += 1;
        } else {
            out.push(lines[idx].to_string());
            idx += 1;
        }
    }
    out.join("\n")
}

/// Push `nat - 2 - content_count` blank `# ` reservation lines (clamped to
/// `[0, MAX_RESERVE_LINES]`); the `- 2` accounts for the opener and closer.
fn push_reservation(out: &mut Vec<String>, nat: usize, content_count: usize, cr: &str) {
    let pad = nat
        .saturating_sub(2)
        .saturating_sub(content_count)
        .min(MAX_RESERVE_LINES);
    for _ in 0..pad {
        out.push(format!("# {cr}"));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rewrites_single_line_cases_in_double_dollar() {
        let input = "# $$X(\\omega) = \\begin{cases} 1 - x & a \\\\ 0 & b \\end{cases}$$";
        let out = format_math(input.to_string());
        let lines: Vec<&str> = out.lines().collect();
        // Expect each structural piece on its own comment line.
        assert!(lines.iter().any(|l| l.trim() == "# $$"), "out:\n{out}");
        assert!(
            lines.iter().any(|l| l.trim() == "# X(\\omega) ="),
            "out:\n{out}"
        );
        assert!(
            lines.iter().any(|l| l.trim() == "# \\begin{cases}"),
            "out:\n{out}"
        );
        assert!(lines.iter().any(|l| l.contains("1 - x & a")), "out:\n{out}");
        assert!(
            lines.iter().any(|l| l.trim() == "# \\end{cases}"),
            "out:\n{out}"
        );
    }

    #[test]
    fn rewrites_inline_dollar_cases_by_promoting_to_double_dollar() {
        let input = "# $\\begin{cases} a \\\\ b \\end{cases}$";
        let out = format_math(input.to_string());
        // The inline `$` was promoted to `$$` so the block form is
        // consistent with the multi-line concealer's expectations.
        assert!(out.contains("# $$\n"), "out:\n{out}");
    }

    #[test]
    fn leaves_already_multiline_cases_untouched() {
        let input = "# $$\n# \\begin{cases}\n# a \\\\\n# b\n# \\end{cases}\n# $$";
        let out = format_math(input.to_string());
        assert_eq!(input, out, "expected idempotent; got:\n{out}");
    }

    #[test]
    fn leaves_plain_prose_untouched() {
        let input = "# just a comment with $x = 1$ inline math\n# and another line";
        let out = format_math(input.to_string());
        assert_eq!(input, out);
    }

    #[test]
    fn preserves_prose_before_and_after_region() {
        let input = "# before $$\\begin{cases} a \\\\ b \\end{cases}$$ after";
        let out = format_math(input.to_string());
        let lines: Vec<&str> = out.lines().collect();
        assert_eq!(lines.first().copied(), Some("# before"));
        assert!(
            lines.last().map(|l| l.trim() == "# after").unwrap_or(false),
            "out:\n{out}"
        );
    }

    #[test]
    fn round_trips_rows_with_backslash_backslash() {
        let input = "# $$\\begin{cases} a \\\\ b \\\\ c \\end{cases}$$";
        let out = format_math(input.to_string());
        // Middle rows keep their trailing `\\`.
        assert!(out.contains("# a \\\\\n"), "out:\n{out}");
        assert!(out.contains("# b \\\\\n"), "out:\n{out}");
        // Last row has no trailing `\\`.
        assert!(out.contains("# c\n"), "out:\n{out}");
    }

    #[test]
    fn single_line_block_tolerates_trailing_comment_junk() {
        let input = "# $$ P = A A^+ = A (A^\\top A)^{-1} A^\\top.$$ #";
        let batch = math_block_latex_batch(input.to_string());
        assert!(
            batch.contains("A^\\top."),
            "image batch sees the block:\n{batch}"
        );
        let out = reserve_math_lines(input.to_string(), "5".to_string());
        let lines: Vec<&str> = out.split('\n').collect();
        assert_eq!(lines.first().copied(), Some("# $$"));
        assert_eq!(
            lines.get(1).copied(),
            Some("# P = A A^+ = A (A^\\top A)^{-1} A^\\top.")
        );
        assert_eq!(lines.last().copied(), Some("# $$"));
    }

    #[test]
    fn single_line_block_with_trailing_prose_stays_strict() {
        let input = "# $$ x $$ some real words";
        let out = format_math(input.to_string());
        assert_eq!(out, input, "prose after the block must not be reflowed");
    }

    #[test]
    fn reflows_dollar_block_with_text_separator() {
        let input = "# $$\n# X_k = \\frac{C_k+C^*_{N-k}}{2} \\text{ and } Y_k = \\frac{C_k-C^*_{N-k}}{2i}.\n# $$";
        let out = format_math(input.to_string());
        let lines: Vec<&str> = out.lines().collect();
        // Expect the single content line to fan out into: X_k = …, \text{ and }, Y_k = …
        assert!(
            lines
                .iter()
                .filter(|l| l.starts_with("# ") && !l.contains("$$"))
                .count()
                >= 3,
            "expected 3+ content lines, got:\n{out}"
        );
        assert!(
            lines.iter().any(|l| l.trim() == "# \\text{ and }"),
            "out:\n{out}"
        );
        assert!(lines.iter().any(|l| l.contains("X_k")), "out:\n{out}");
        assert!(lines.iter().any(|l| l.contains("Y_k")), "out:\n{out}");
    }

    #[test]
    fn dollar_block_reflow_is_idempotent() {
        let formatted = "# $$\n# X_k = \\frac{a}{b}\n# \\text{ and }\n# Y_k = \\frac{c}{d}\n# $$";
        let out = format_math(formatted.to_string());
        assert_eq!(formatted, out, "reflow should be a no-op; got:\n{out}");
    }

    #[test]
    fn dollar_block_reflow_breaks_at_begin_end_env() {
        let input = "# $$\n# \\begin{aligned} x = 1 \\\\ y = 2 \\end{aligned}\n# $$";
        let out = format_math(input.to_string());
        // Each structural piece — \begin, each row, \end — should be its own line.
        let lines: Vec<&str> = out.lines().collect();
        assert!(
            lines.iter().any(|l| l.trim() == "# \\begin{aligned}"),
            "out:\n{out}"
        );
        assert!(
            lines.iter().any(|l| l.trim() == "# \\end{aligned}"),
            "out:\n{out}"
        );
    }

    #[test]
    fn cases_row_condition_does_not_split_off() {
        // User bug: when a cases row's condition used `\text{otherwise}`,
        // the block splitter peeled the `\text{...}` onto its own line,
        // leaving "0" and "otherwise" as separate physical lines. The
        // scanner then rendered the fence with row 2 value "0" only and
        // "otherwise" as an orphan below the fence.
        let input = "# $$\n\
                     # X(\\omega) =\n\
                     # \\begin{cases}\n\
                     # 1 - \\frac{|\\omega|}{\\omega_0} & |\\omega| \\leq \\omega_0 \\\\\n\
                     # 0 &\n\
                     # \\text{otherwise}\n\
                     # \\end{cases}\n\
                     # $$";
        let out = format_math(input.to_string());
        let lines: Vec<&str> = out.lines().collect();
        // Both rows of the cases env should be on their own line, with
        // value + condition joined by `&`. Critically, no `\text{otherwise}`
        // line that stands alone without a preceding value.
        assert!(
            lines.iter().any(|l| l.trim() == "# 0 & \\text{otherwise}"),
            "expected joined row '0 & \\text{{otherwise}}', got:\n{out}"
        );
        assert!(
            !lines.iter().any(|l| l.trim() == "# \\text{otherwise}"),
            "condition should NOT be on its own line, got:\n{out}"
        );
    }

    #[test]
    fn cases_row_join_is_idempotent() {
        // Running format_math on its own output shouldn't re-split the
        // joined row.
        let first = "# $$\n\
                     # \\begin{cases}\n\
                     # 0 &\n\
                     # \\text{otherwise}\n\
                     # \\end{cases}\n\
                     # $$";
        let once = format_math(first.to_string());
        let twice = format_math(once.clone());
        assert_eq!(
            once, twice,
            "format_math should be idempotent; second pass changed output:\nfirst:\n{once}\nsecond:\n{twice}"
        );
    }

    #[test]
    fn handles_multibyte_utf8_in_content() {
        // Regression: `≤` is 3 bytes — stepping `i += 1` lands inside it,
        // and `content[i..]` panicked on `is not a char boundary`.
        let input = "# $$X(\\omega) = \\begin{cases} 1 & |\\omega| \\leq \\omega_0 \\\\ 0 & \\text{otherwise} \\end{cases}$$";
        let out = format_math(input.to_string());
        assert!(out.contains("\\begin{cases}"), "out:\n{out}");
        assert!(out.contains("\\end{cases}"), "out:\n{out}");
        assert!(out.contains("\\text{otherwise}"), "out:\n{out}");
    }

    #[test]
    fn reflows_dollar_block_with_multibyte_content() {
        let input = "# $$\n# x = 5 ≤ y \\text{ and } ω ∈ ℝ\n# $$";
        let out = format_math(input.to_string());
        assert!(out.contains("\\text{ and }"), "out:\n{out}");
    }

    #[test]
    fn preserves_crlf_line_endings() {
        let input = "# $$\\begin{cases} a \\\\ b \\end{cases}$$\r\n";
        let out = format_math(input.to_string());
        // Output lines all end with \r\n (original had CRLF on the input line;
        // we preserve the CR on emitted lines).
        assert!(out.contains("\r\n"), "out:\n{out}");
    }

    fn count_lines(out: &str, pred: impl Fn(&str) -> bool) -> usize {
        out.split('\n').filter(|l| pred(l)).count()
    }

    #[test]
    fn pads_short_block_to_natural_rows() {
        let input = "# $$\n# E=mc^2\n# $$";
        let out = reserve_math_lines(input.to_string(), "8".to_string());
        let lines: Vec<&str> = out.split('\n').collect();
        assert_eq!(lines.len(), 8, "block should span 8 lines:\n{out}");
        assert_eq!(lines.first().copied(), Some("# $$"));
        assert_eq!(lines.get(1).copied(), Some("# E=mc^2"));
        assert_eq!(lines.last().copied(), Some("# $$"));
        assert_eq!(count_lines(&out, |l| l == "# "), 5, "5 pad lines:\n{out}");
    }

    #[test]
    fn never_shrinks_taller_content() {
        let input = "# $$\n# a\n# b\n# c\n# d\n# e\n# $$";
        let out = reserve_math_lines(input.to_string(), "4".to_string());
        for c in ["# a", "# b", "# c", "# d", "# e"] {
            assert!(out.split('\n').any(|l| l == c), "{c} preserved:\n{out}");
        }
        assert_eq!(count_lines(&out, |l| l == "# "), 0, "no pad added:\n{out}");
    }

    #[test]
    fn strip_then_repad_is_idempotent() {
        let input = "# $$\n# E=mc^2\n# $$";
        let once = reserve_math_lines(input.to_string(), "8".to_string());
        let twice = reserve_math_lines(once.clone(), "8".to_string());
        assert_eq!(once, twice, "reserve must be its own fixed point:\n{twice}");
    }

    #[test]
    fn zero_spec_leaves_block_untouched() {
        let input = "# $$\n# x\n# \n# \n# $$";
        let out = reserve_math_lines(input.to_string(), "0".to_string());
        assert_eq!(input, out, "nat=0 must not mutate a block:\n{out}");
    }

    #[test]
    fn expands_plain_single_line_then_pads() {
        let input = "# $$ \\int_0^1 x = 1 $$";
        let out = reserve_math_lines(input.to_string(), "6".to_string());
        let lines: Vec<&str> = out.split('\n').collect();
        assert_eq!(lines.len(), 6, "single-line expands to 6 lines:\n{out}");
        assert_eq!(lines.first().copied(), Some("# $$"));
        assert_eq!(lines.get(1).copied(), Some("# \\int_0^1 x = 1"));
        assert_eq!(lines.last().copied(), Some("# $$"));
        assert_eq!(count_lines(&out, |l| l == "# "), 3, "3 pad lines:\n{out}");
    }

    #[test]
    fn middle_blank_preserved_only_trailing_stripped() {
        let input = "# $$\n# a\n# \n# b\n# $$";
        let out = reserve_math_lines(input.to_string(), "8".to_string());
        let lines: Vec<&str> = out.split('\n').collect();
        assert_eq!(lines.len(), 8, "block spans 8 lines:\n{out}");
        assert_eq!(lines.first().copied(), Some("# $$"));
        assert_eq!(lines.get(1).copied(), Some("# a"));
        assert_eq!(lines.get(2).copied(), Some("# "));
        assert_eq!(lines.get(3).copied(), Some("# b"));
        let again = reserve_math_lines(out.clone(), "8".to_string());
        assert_eq!(out, again, "middle blank must be stable:\n{again}");
    }

    #[test]
    fn bare_hash_reservation_detected() {
        let input = "# $$\n# x\n#\n#\n# $$";
        let out = reserve_math_lines(input.to_string(), "8".to_string());
        let lines: Vec<&str> = out.split('\n').collect();
        assert_eq!(lines.len(), 8, "bare-# block detected and repadded:\n{out}");
        assert_eq!(lines.get(1).copied(), Some("# x"));
        assert_eq!(count_lines(&out, |l| l == "# "), 5, "5 pad lines:\n{out}");
    }

    #[test]
    fn preserves_crlf_pad() {
        let input = "# $$\r\n# x\r\n# $$\r\n";
        let out = reserve_math_lines(input.to_string(), "6".to_string());
        assert!(out.contains("# \r\n"), "pad lines keep CRLF:\n{out:?}");
        assert!(out.contains("# x\r\n"), "content keeps CRLF:\n{out:?}");
    }

    #[test]
    fn spec_fewer_than_blocks_and_garbage_is_safe() {
        let input = "# $$\n# a\n# $$\n# $$\n# b\n# $$";
        let out = reserve_math_lines(input.to_string(), "8".to_string());
        assert_eq!(
            count_lines(&out, |l| l == "# "),
            5,
            "only block 1 pads:\n{out}"
        );
        let garbage = reserve_math_lines(input.to_string(), "abc,4".to_string());
        assert!(
            garbage.contains("# a"),
            "no panic on garbage spec:\n{garbage}"
        );
    }

    #[test]
    fn reserve_no_blocks_is_format_only() {
        let input = "# just prose\n# more prose\nx = 1";
        let out = reserve_math_lines(input.to_string(), String::new());
        assert_eq!(out, format_math(input.to_string()));
        assert_eq!(out, input);
    }

    #[test]
    fn batch_latex_count_and_order() {
        let input = "# $$\n# alpha\n# $$\n# text\n# $$\n# beta\n# $$";
        let blob = math_block_latex_batch(input.to_string());
        let parts: Vec<&str> = blob.split(BATCH_SEP).collect();
        assert_eq!(
            parts,
            vec!["alpha", "beta"],
            "two blocks in order:\n{blob:?}"
        );
    }

    #[test]
    fn batch_latex_is_pad_invariant() {
        let bare = math_block_latex_batch("# $$\n# E=mc^2\n# $$".to_string());
        let padded = math_block_latex_batch("# $$\n# E=mc^2\n# \n# \n# \n# $$".to_string());
        assert_eq!(bare, padded, "pad must not change rendered latex");
    }

    #[test]
    fn batch_latex_empty_without_blocks() {
        assert_eq!(
            math_block_latex_batch("# prose only\nx = 1".to_string()),
            ""
        );
    }

    #[test]
    fn batch_latex_expands_single_line() {
        let blob = math_block_latex_batch("# $$ x = 1 $$".to_string());
        assert_eq!(blob, "x = 1");
    }
}
