//! `DimensionMismatch` enricher.
//!
//! For a message like "a has size (8, 8), b has size (5, 5), mismatch
//! at dim 1" plus source line "result = S_hat * K", emits:
//!
//!     = note: `S_hat` is (8, 8), `K` is (5, 5) — need size(S_hat, 2) == size(K, 1)
//!     = help: check with: size(S_hat), size(K)
//!
//! Pulls the variable names from the source instead of using the
//! placeholders Julia emits.

use std::fmt::Write;

use super::extract_var_name;

pub(super) fn enrich(message: &str, source: &str) -> Option<String> {
    let dims = scan_dimension_pairs(message);
    let operands = scan_binary_operands(source);

    if dims.len() < 2 || operands.is_empty() {
        return None;
    }

    let lhs = &operands[0];
    let rhs = operands.get(1)?;

    let mut out = String::new();
    let _ = write!(
        out,
        "   = note: `{lhs}` is {}, `{rhs}` is {}",
        dims[0], dims[1]
    );
    if message.contains("mismatch at dim") {
        let _ = write!(out, " — need size({lhs}, 2) == size({rhs}, 1)");
    }
    out.push('\n');
    let _ = writeln!(out, "   = help: check with: size({lhs}), size({rhs})");
    Some(out)
}

/// Scan for "(M, N)" dimension pairs in an error message.
/// Returns strings like "(8, 8)", "(5, 5)".
fn scan_dimension_pairs(msg: &str) -> Vec<String> {
    let mut pairs = Vec::new();
    let bytes = msg.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'(' {
            let start = i;
            i += 1;
            while i < bytes.len() && bytes[i] != b')' {
                i += 1;
            }
            if i < bytes.len() {
                let content = &msg[start + 1..i];
                if content
                    .split(',')
                    .all(|p| p.trim().chars().all(|c| c.is_ascii_digit()))
                {
                    pairs.push(msg[start..=i].to_string());
                }
                i += 1;
            }
        } else {
            i += 1;
        }
    }
    pairs
}

/// Extract the two operands of a binary operator from a source line.
/// "result = `S_hat` * K" → ["`S_hat`", "K"]
/// "A \ b" → ["A", "b"]
/// "x .+ y" → ["x", "y"]
fn scan_binary_operands(source: &str) -> Vec<String> {
    // Strip assignment: "result = S_hat * K" → "S_hat * K", but only
    // when the `=` is the assignment kind (not `==`, `!=`, `<=`, `>=`).
    let expr = if let Some(eq) = source.find('=') {
        let before_eq = if eq > 0 { source.as_bytes()[eq - 1] } else { 0 };
        let after_eq = source.as_bytes().get(eq + 1).copied().unwrap_or(0);
        if before_eq != b'!' && before_eq != b'<' && before_eq != b'>' && after_eq != b'=' {
            source[eq + 1..].trim()
        } else {
            source.trim()
        }
    } else {
        source.trim()
    };

    // Try common binary operators in length-descending order so that
    // `.\` is matched as a unit rather than being treated as `.` + `\`.
    for op in &[
        " .* ", " ./ ", " .\\ ", " * ", " / ", " \\ ", " .+ ", " .- ",
    ] {
        if let Some(idx) = expr.find(op) {
            let lhs = expr[..idx].trim();
            let rhs = expr[idx + op.len()..].trim();
            let lhs_name = extract_var_name(lhs);
            let rhs_name = extract_var_name(rhs);
            if !lhs_name.is_empty() && !rhs_name.is_empty() {
                return vec![lhs_name, rhs_name];
            }
        }
    }
    Vec::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn enrich_pins_operand_names_to_dimensions() {
        let msg = "DimensionMismatch: a has size (8, 8), b has size (5, 5), mismatch at dim 1";
        let src = "result = S_hat * K";
        let out = enrich(msg, src).unwrap();
        assert!(out.contains("`S_hat` is (8, 8)"), "got:\n{out}");
        assert!(out.contains("`K` is (5, 5)"), "got:\n{out}");
        assert!(out.contains("size(S_hat, 2) == size(K, 1)"), "got:\n{out}");
        assert!(
            out.contains("check with: size(S_hat), size(K)"),
            "got:\n{out}"
        );
    }

    #[test]
    fn scan_dimension_pairs_extracts_sizes() {
        assert_eq!(
            scan_dimension_pairs("(8, 8), (5, 5)"),
            vec!["(8, 8)", "(5, 5)"]
        );
        assert_eq!(
            scan_dimension_pairs("nope (text) here"),
            Vec::<String>::new()
        );
    }

    #[test]
    fn scan_binary_operands_handles_multiplication() {
        assert_eq!(scan_binary_operands("S_hat * K"), vec!["S_hat", "K"]);
        assert_eq!(
            scan_binary_operands("result = S_hat * K"),
            vec!["S_hat", "K"]
        );
    }

    #[test]
    fn scan_binary_operands_handles_backslash_solve() {
        assert_eq!(scan_binary_operands("A \\ b"), vec!["A", "b"]);
    }

    #[test]
    fn scan_binary_operands_ignores_comparison_eq() {
        // `x == y` is NOT an assignment; we leave the expression intact and
        // never find a binary-op pattern in `x == y` because == is not in
        // our operator list. Verify it returns empty rather than wrongly
        // splitting on `=`.
        assert_eq!(scan_binary_operands("x == y"), Vec::<String>::new());
    }
}
