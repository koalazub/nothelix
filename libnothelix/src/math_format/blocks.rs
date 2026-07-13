//! Display-math block detection over Julia comment lines: single-line
//! `# $$ … $$`, multi-line `# $$` … `# $$`, and one shared enumeration every
//! downstream pass walks so the k-th rendered result, spec entry, and reserved
//! block all line up. `single_line_block_body` mirrors the Steel twin
//! `single-line-block-body` so Rust and the plugin agree on block boundaries.

use super::comment::{comment_body, is_blank_comment_line};

#[derive(Clone, Copy)]
pub(super) enum BlockKind {
    Multi(usize),
    Single,
}

pub(super) fn single_line_block_body(line: &str) -> Option<String> {
    let body = comment_body(line)?;
    let body = body.trim_end_matches(['#', ' ', '\t']);
    if body.len() > 4 && body.starts_with("$$") && body.ends_with("$$") {
        Some(body[2..body.len() - 2].trim().to_string())
    } else {
        None
    }
}

pub(super) fn find_dollar_block(lines: &[&str], start: usize) -> Option<usize> {
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

pub(super) fn enumerate_blocks(lines: &[&str]) -> Vec<(usize, BlockKind)> {
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

pub(super) fn block_content_range(
    lines: &[&str],
    open: usize,
    close: usize,
) -> std::ops::Range<usize> {
    let mut last = close;
    while last > open + 1 && is_blank_comment_line(lines[last - 1]) {
        last -= 1;
    }
    (open + 1)..last
}
