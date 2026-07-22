//! LaTeX reflow of Julia math comments. Two shapes: single-line block envs are
//! expanded into a multi-line `$$` block (`\begin`/rows/`\end` each on their
//! own line), and crammed lines already inside a `$$` block are split at math
//! boundaries (`\text{…}` annotations, `\\` row separators, `\begin`/`\end`
//! wrappers). Cases/matrix rows whose cells wrap onto continuation lines are
//! rejoined first so each logical row stays one physical line.

use super::blocks::find_dollar_block;
use super::comment::cr_suffix;
use crate::unicode::math_regions::find_math_regions;

const BLOCK_ENVS: &[&str] = &[
    "cases", "pmatrix", "bmatrix", "vmatrix", "Vmatrix", "matrix", "aligned", "align", "split",
    "gathered", "gather",
];

/// Canonicalise a document's math comments: expand single-line block envs into
/// multi-line `$$` blocks and split crammed `$$`-block lines. Idempotent.
pub fn format_math(text: String) -> String {
    let mut out = String::new();
    let lines: Vec<&str> = text.split('\n').collect();
    let line_count = lines.len();
    let mut idx = 0;
    while idx < line_count {
        let line = lines[idx];
        if let Some(close_idx) = find_dollar_block(&lines, idx) {
            out.push_str(line);
            out.push('\n');
            for inner in join_cases_continuations(&lines[idx + 1..close_idx]) {
                emit_reformatted_block_line(&inner, &mut out);
            }
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

fn join_cases_continuations(lines: &[&str]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for raw in lines {
        let body = raw.trim_end_matches('\r');
        let Some(content) = body.strip_prefix("# ") else {
            out.push((*raw).to_string());
            continue;
        };
        let prev_is_continuation = out
            .last()
            .and_then(|p| p.strip_prefix("# "))
            .is_some_and(|prev| prev.trim_end().ends_with('&'));
        if prev_is_continuation {
            let prev = out.pop().expect("last() was Some");
            out.push(format!("{} {}", prev.trim_end(), content.trim_start()));
        } else {
            out.push((*raw).to_string());
        }
    }
    out
}

fn emit_reformatted_block_line(line: &str, out: &mut String) {
    let body = line.trim_end_matches('\r');
    let cr = cr_suffix(line);
    let Some(content) = body.strip_prefix("# ") else {
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
        if bytes[i] == b'\\' {
            if content[i..].starts_with("\\text{") {
                let preceding = content[cursor..i].trim_end();
                if preceding.ends_with('&') {
                    i = match_brace_after(bytes, i + 6);
                    continue;
                }
                push(&mut pieces, &content[cursor..i]);
                let end = match_brace_after(bytes, i + 6);
                push(&mut pieces, &content[i..end]);
                cursor = end;
                i = end;
                continue;
            }
            if i + 1 < bytes.len() && bytes[i + 1] == b'\\' {
                push(&mut pieces, &content[cursor..i + 2]);
                cursor = i + 2;
                i += 2;
                continue;
            }
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

fn rewrite_line(line: &str) -> Option<String> {
    let cr = cr_suffix(line);
    let line_body = line.strip_suffix('\r').unwrap_or(line);
    let content = line_body.strip_prefix("# ")?;

    let regions = find_math_regions(content);
    if regions.is_empty() {
        return None;
    }

    for (region_start, region_end) in regions {
        let math = &content[region_start..region_end];
        if let Some(block) = find_single_line_block_env(math) {
            return Some(emit_multiline_form(
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
    begin_start: usize,
    begin_end: usize,
    end_start: usize,
    end_end: usize,
}

fn find_single_line_block_env(math: &str) -> Option<BlockEnv<'_>> {
    let bytes = math.as_bytes();
    let mut i = 0;
    while i + 6 < bytes.len() {
        if bytes[i] != b'\\' || !math[i..].starts_with("\\begin{") {
            i += 1;
            continue;
        }
        let begin_start = i;
        let name_start = i + 7;
        let Some(p) = math[name_start..].find('}') else {
            break;
        };
        let name_end = name_start + p;
        let env_name = &math[name_start..name_end];
        let begin_end = name_end + 1;

        if !BLOCK_ENVS.contains(&env_name) {
            i = begin_end;
            continue;
        }

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

fn emit_multiline_form(
    content: &str,
    region_start: usize,
    region_end: usize,
    math: &str,
    block: &BlockEnv<'_>,
    cr: &str,
) -> String {
    let region_delim_span = classify_region_delim(content, region_start, region_end);
    let prose_before = content[..region_delim_span.outer_start].trim_end();
    let prose_after = content[region_delim_span.outer_end..].trim_start();

    let math_prefix = math[..block.begin_start].trim();
    let math_suffix = math[block.end_end..].trim();
    let env_body = &math[block.begin_end..block.end_start];

    let mut out = String::new();
    let mut push_line = |s: &str| {
        out.push_str("# ");
        out.push_str(s);
        out.push_str(cr);
        out.push('\n');
    };

    if !prose_before.is_empty() {
        push_line(prose_before);
    }
    push_line("$$");
    if !math_prefix.is_empty() {
        push_line(math_prefix);
    }
    push_line(&format!("\\begin{{{}}}", block.name));
    for row in split_rows(env_body) {
        let trimmed = row.trim();
        if !trimmed.is_empty() {
            push_line(trimmed);
        }
    }
    push_line(&format!("\\end{{{}}}", block.name));
    if !math_suffix.is_empty() {
        push_line(math_suffix);
    }
    push_line("$$");
    if !prose_after.is_empty() {
        push_line(prose_after);
    }

    if out.ends_with('\n') {
        out.pop();
        if out.ends_with('\r') {
            out.pop();
        }
    }
    out
}

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rewrites_single_line_cases_in_double_dollar() {
        let input = "# $$X(\\omega) = \\begin{cases} 1 - x & a \\\\ 0 & b \\end{cases}$$";
        let out = format_math(input.to_string());
        let lines: Vec<&str> = out.lines().collect();
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
            lines.last().is_some_and(|l| l.trim() == "# after"),
            "out:\n{out}"
        );
    }

    #[test]
    fn round_trips_rows_with_backslash_backslash() {
        let input = "# $$\\begin{cases} a \\\\ b \\\\ c \\end{cases}$$";
        let out = format_math(input.to_string());
        assert!(out.contains("# a \\\\\n"), "out:\n{out}");
        assert!(out.contains("# b \\\\\n"), "out:\n{out}");
        assert!(out.contains("# c\n"), "out:\n{out}");
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
        assert!(out.contains("\r\n"), "out:\n{out}");
    }
}
