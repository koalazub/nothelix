//! Image-batch extraction: every display block's LaTeX, `BATCH_SEP`-joined in
//! document order for the async renderer. Each block's trailing renderer-owned
//! blank run is excluded, so a block's LaTeX — and its measured natural height —
//! is invariant to how much reservation padding it currently carries.

use super::blocks::{BlockKind, block_content_range, enumerate_blocks, single_line_block_body};
use super::comment::comment_body;
use super::reflow::format_math;
use crate::math_image::BATCH_SEP;

/// The LaTeX of every display-math block, `BATCH_SEP`-joined in document order;
/// `""` when the document has no display math.
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

#[cfg(test)]
mod tests {
    use super::*;

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
