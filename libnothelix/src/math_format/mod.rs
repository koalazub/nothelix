//! Multi-line formatter for math environments in Julia notebook comments.
//!
//! Single-line LaTeX block envs (`\begin{cases}…\end{cases}`, `pmatrix`, …)
//! cannot render across visual lines through the conceal overlay layer, so the
//! source is rewritten to span the lines it semantically should and each block
//! reserves real blank lines matching its rendered image height. Every pass
//! preserves the Julia `# ` prefix and is idempotent.

mod batch;
mod blocks;
mod comment;
mod reflow;
mod reserve;

pub use batch::math_block_latex_batch;
pub use reflow::format_math;
pub use reserve::reserve_math_lines;
