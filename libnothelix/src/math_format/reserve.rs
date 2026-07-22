//! Height reservation: rewrite each display block to hold real blank `# ` lines
//! equal to its rendered image height, per a comma-separated natural-row spec
//! (one entry per block, `0` = leave untouched). The buffer is canonicalised
//! first and trailing blank runs are stripped before repadding, so emit and
//! strip share one blank predicate and the transform is its own fixed point.

use super::blocks::{BlockKind, block_content_range, enumerate_blocks, single_line_block_body};
use super::comment::cr_suffix;
use super::reflow::format_math;

const MAX_RESERVE_LINES: usize = 64;

/// Rewrite each display block to reserve `spec`-many natural rows of blank
/// lines; `spec` is one row count per block in enumeration order.
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
    use crate::math_format::math_block_latex_batch;

    fn count_lines(out: &str, pred: impl Fn(&str) -> bool) -> usize {
        out.split('\n').filter(|l| pred(l)).count()
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
}
