//! Unicode fence characters for LaTeX matrix-style environments.
//!
//! "Fence" characters are the large curly/bracket decorations that wrap
//! piecewise functions, matrices, and other multi-row environments. The
//! scanner emits these at `\begin{env}` / `\end{env}` / `\\` boundaries.

/// Opening fence character for the given environment, or empty if none.
pub(super) fn open_fence(env: &str, _total_rows: usize) -> &'static str {
    match env {
        "cases" => "⎧",
        "pmatrix" => "⎛",
        "bmatrix" => "⎡",
        "vmatrix" => "│",
        "aligned" | "gathered" | "split" | "gather" | "align" | "matrix" => "",
        _ => "",
    }
}

/// Closing fence character for the given environment, or empty if none.
pub(super) fn close_fence(env: &str, _total_rows: usize) -> &'static str {
    match env {
        "cases" => "⎩",
        "pmatrix" => "⎞",
        "bmatrix" => "⎤",
        "vmatrix" => "│",
        "aligned" | "gathered" | "split" | "gather" | "align" | "matrix" => "",
        _ => "",
    }
}

/// Row-boundary fence character for the given environment, or empty if none.
pub(super) fn mid_fence(env: &str, _row: usize, _total_rows: usize) -> &'static str {
    match env {
        "cases" => "⎨",
        "pmatrix" => "⎜",
        "bmatrix" => "⎢",
        "vmatrix" => "│",
        "aligned" | "gathered" | "split" | "gather" | "align" | "matrix" => "",
        _ => "",
    }
}
