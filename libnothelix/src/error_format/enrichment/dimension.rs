use std::fmt::Write;

use super::extract_var_name;

const BINARY_OPERATORS: [&str; 8] = [
    " .* ", " ./ ", " .\\ ", " * ", " / ", " \\ ", " .+ ", " .- ",
];

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

fn scan_dimension_pairs(msg: &str) -> Vec<String> {
    let mut pairs = Vec::new();
    let bytes = msg.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] != b'(' {
            i += 1;
            continue;
        }
        let start = i;
        i += 1;
        while i < bytes.len() && bytes[i] != b')' {
            i += 1;
        }
        if i < bytes.len() {
            if is_numeric_tuple(&msg[start + 1..i]) {
                pairs.push(msg[start..=i].to_string());
            }
            i += 1;
        }
    }
    pairs
}

fn is_numeric_tuple(content: &str) -> bool {
    content
        .split(',')
        .all(|part| part.trim().chars().all(|c| c.is_ascii_digit()))
}

fn scan_binary_operands(source: &str) -> Vec<String> {
    let expr = strip_assignment(source);
    for op in BINARY_OPERATORS {
        let Some(idx) = expr.find(op) else {
            continue;
        };
        let lhs = extract_var_name(expr[..idx].trim());
        let rhs = extract_var_name(expr[idx + op.len()..].trim());
        if !lhs.is_empty() && !rhs.is_empty() {
            return vec![lhs, rhs];
        }
    }
    Vec::new()
}

fn strip_assignment(source: &str) -> &str {
    let Some(eq) = source.find('=') else {
        return source.trim();
    };
    let bytes = source.as_bytes();
    let before = eq.checked_sub(1).map(|i| bytes[i]);
    let after = bytes.get(eq + 1).copied();
    if matches!(before, Some(b'!' | b'<' | b'>')) || after == Some(b'=') {
        return source.trim();
    }
    source[eq + 1..].trim()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn enrich_pins_operand_names_to_dimensions() {
        let msg = "DimensionMismatch: a has size (8, 8), b has size (5, 5), mismatch at dim 1";
        let out = enrich(msg, "result = S_hat * K").expect("both sides parse");
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
        assert_eq!(scan_binary_operands("x == y"), Vec::<String>::new());
    }
}
