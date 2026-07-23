//! Canonical cell-source hashing: fold both mechanical buffer transforms
//! (reflow via `format_math`, height reservation via `reserve_math_lines`) to a
//! single fixed point before hashing, so a cell whose source the render
//! pipeline rewrote in place still hashes to the value it had at its last run.
//! The canonical form is `format_math` applied, then each display block reduced
//! to its content — reservation padding stripped and single-line blocks expanded
//! to the multi-line shape `reserve_math_lines` would produce — hashed with the
//! same djb2 the plain source hash uses.

use super::blocks::{BlockKind, block_content_range, enumerate_blocks, single_line_block_body};
use super::comment::cr_suffix;
use super::reflow::format_math;

pub fn canonical_cell_hash(code: String) -> isize {
    crate::slm::djb2_hash(&canonical_form(code)) as isize
}

fn canonical_form(text: String) -> String {
    let formatted = format_math(text);
    let lines: Vec<&str> = formatted.split('\n').collect();
    let blocks = enumerate_blocks(&lines);

    let mut out: Vec<String> = Vec::with_capacity(lines.len());
    let mut bi = 0;
    let mut idx = 0;
    while idx < lines.len() {
        if bi < blocks.len() && blocks[bi].0 == idx {
            let (open, kind) = blocks[bi];
            match kind {
                BlockKind::Multi(close) => {
                    out.push(lines[open].to_string());
                    for k in block_content_range(&lines, open, close) {
                        out.push(lines[k].to_string());
                    }
                    out.push(lines[close].to_string());
                    idx = close + 1;
                }
                BlockKind::Single => {
                    let cr = cr_suffix(lines[open]);
                    let inner = single_line_block_body(lines[open]).unwrap_or_default();
                    out.push(format!("# $${cr}"));
                    if !inner.is_empty() {
                        out.push(format!("# {inner}{cr}"));
                    }
                    out.push(format!("# $${cr}"));
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::math_format::reserve_math_lines;

    const REFLOW_FIXTURES: &[&str] = &[
        "# $$X(\\omega) = \\begin{cases} 1 - x & a \\\\ 0 & b \\end{cases}$$",
        "# $\\begin{cases} a \\\\ b \\end{cases}$",
        "# $$\n# \\begin{cases}\n# a \\\\\n# b\n# \\end{cases}\n# $$",
        "# just a comment with $x = 1$ inline math\n# and another line",
        "# before $$\\begin{cases} a \\\\ b \\end{cases}$$ after",
        "# $$\\begin{cases} a \\\\ b \\\\ c \\end{cases}$$",
        "# $$ x $$ some real words",
        "# $$\n# X_k = \\frac{C_k+C^*_{N-k}}{2} \\text{ and } Y_k = \\frac{C_k-C^*_{N-k}}{2i}.\n# $$",
        "# $$\n# X_k = \\frac{a}{b}\n# \\text{ and }\n# Y_k = \\frac{c}{d}\n# $$",
        "# $$\n# \\begin{aligned} x = 1 \\\\ y = 2 \\end{aligned}\n# $$",
        "# $$\n# x = 5 ≤ y \\text{ and } ω ∈ ℝ\n# $$",
        "# $$\\begin{cases} a \\\\ b \\end{cases}$$\r\n",
        "# $$ E=mc^2 $$",
        "# $$ \\int_0^1 x = 1 $$",
        "# $$\n# E=mc^2\n# $$",
        "# $$\n# a\n# \n# b\n# $$",
        "x = 1\ny = 2",
        "# a plain comment\nprint(x)",
    ];

    fn block_count(x: &str) -> usize {
        let formatted = format_math(x.to_string());
        let lines: Vec<&str> = formatted.split('\n').collect();
        enumerate_blocks(&lines).len()
    }

    fn pad_spec(x: &str) -> String {
        vec!["8"; block_count(x).max(1)].join(",")
    }

    #[test]
    fn canonical_is_fixpoint_over_reserve() {
        for x in REFLOW_FIXTURES {
            let reserved = reserve_math_lines(x.to_string(), pad_spec(x));
            assert_eq!(
                canonical_form(reserved),
                canonical_form(x.to_string()),
                "reserve must be invisible to the canonical form:\n{x}"
            );
        }
    }

    #[test]
    fn canonical_is_fixpoint_over_format_math() {
        for x in REFLOW_FIXTURES {
            let reflowed = format_math(x.to_string());
            assert_eq!(
                canonical_form(reflowed),
                canonical_form(x.to_string()),
                "format_math must be invisible to the canonical form:\n{x}"
            );
        }
    }

    #[test]
    fn canonical_hash_equals_raw_djb2_for_non_math_cells() {
        for x in [
            "x = 1\ny = 2",
            "# a plain comment\nprint(x)",
            "function f(x)\n    x + 1\nend",
            "",
            "# inline $a+b$ only, no display block",
        ] {
            assert_eq!(
                canonical_cell_hash(x.to_string()),
                crate::slm::djb2_hash_ffi(x.to_string()),
                "no-math cell must hash identically to the plain source hash:\n{x}"
            );
        }
    }

    #[test]
    fn canonical_hash_is_padding_invariant() {
        let bare = canonical_cell_hash("# $$\n# E=mc^2\n# $$".to_string());
        let padded = canonical_cell_hash("# $$\n# E=mc^2\n# \n# \n# \n# $$".to_string());
        assert_eq!(bare, padded);
    }

    #[test]
    fn canonical_hash_still_changes_on_real_content_edit() {
        let before = canonical_cell_hash("# $$\n# E=mc^2\n# $$".to_string());
        let after = canonical_cell_hash("# $$\n# E=mc^3\n# $$".to_string());
        assert_ne!(before, after);
    }
}
