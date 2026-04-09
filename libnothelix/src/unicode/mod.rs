//! LaTeX-to-Unicode symbol table and overlay rendering for Helix.
//!
//! The public entry points are re-exported from this module:
//!
//!   - [`unicode_lookup`] / [`unicode_completions_for_prefix`] — LaTeX name
//!     → Unicode char table used by the `\alpha<Tab>` completion.
//!   - [`latex_overlays`] — given the text inside a single math region,
//!     return a JSON array of conceal overlays for that region.
//!   - [`compute_conceal_overlays`] / [`compute_conceal_overlays_for_comments`]
//!     — given a whole document, find math regions and return conceal
//!     overlays with document-relative char offsets.
//!
//! Internal module layout:
//!
//!   - [`symbol_table`] — the 2544-entry Julia stdlib symbol table.
//!   - [`sub_super`] — sub/superscript maps + `\mathbf` font helper.
//!   - [`fence`] — environment fence glyph lookup (⎧⎨⎩, ⎛⎜⎞, …).
//!   - [`math_regions`] — finds `$...$` / `\(...\)` boundaries.
//!   - [`overlay`] — the `Overlay` record the scanner emits.
//!   - [`scanner`] — the byte-offset LaTeX scanner itself, decomposed
//!     into one method per case via the [`scanner::Scanner`] struct.
//!   - [`conceal`] — document-level entry points that wrap the scanner.

mod conceal;
mod fence;
mod math_regions;
mod overlay;
mod scanner;
mod sub_super;
mod symbol_table;

pub use conceal::{compute_conceal_overlays, compute_conceal_overlays_for_comments};
pub use scanner::latex_overlays;
pub use symbol_table::{unicode_completions_for_prefix, unicode_lookup};

#[cfg(test)]
mod tests {
    use super::*;
    use super::symbol_table::SYMBOLS;

    #[test]
    fn lookup_alpha() {
        assert_eq!(unicode_lookup("alpha".into()), "α");
    }

    #[test]
    fn lookup_in() {
        assert_eq!(unicode_lookup("in".into()), "∈");
    }

    #[test]
    fn lookup_pi() {
        assert_eq!(unicode_lookup("pi".into()), "π");
    }

    #[test]
    fn lookup_missing() {
        assert_eq!(unicode_lookup("notareal symbol".into()), "");
    }

    #[test]
    fn completions_prefix() {
        let result = unicode_completions_for_prefix("alp".into());
        let v: serde_json::Value = serde_json::from_str(&result).unwrap();
        let arr = v.as_array().unwrap();
        assert!(!arr.is_empty());
        assert!(arr.iter().any(|e| e["name"] == "alpha"));
    }

    #[test]
    fn completions_empty_prefix_capped() {
        let result = unicode_completions_for_prefix("".into());
        let v: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert!(v.as_array().unwrap().len() <= 50);
    }

    #[test]
    fn latex_overlays_simple_command() {
        let result = latex_overlays(r"\alpha + \beta".into());
        let v: serde_json::Value = serde_json::from_str(&result).unwrap();
        let arr = v.as_array().unwrap();
        // \alpha is 6 chars: overlay at 0 with α, hide 1-5
        assert!(!arr.is_empty());
        assert_eq!(arr[0]["offset"], 0);
        assert_eq!(arr[0]["replacement"], "α");
        assert_eq!(arr[1]["replacement"], "");
    }

    #[test]
    fn latex_overlays_superscript() {
        let result = latex_overlays("10^{-6}".into());
        let v: serde_json::Value = serde_json::from_str(&result).unwrap();
        let arr = v.as_array().unwrap();
        // Should produce overlays for ^, {, -, 6, }
        assert!(!arr.is_empty());
        // Check that - becomes ⁻ and 6 becomes ⁶
        let replacements: Vec<&str> = arr
            .iter()
            .map(|o| o["replacement"].as_str().unwrap())
            .collect();
        assert!(replacements.contains(&"⁻"));
        assert!(replacements.contains(&"⁶"));
    }

    #[test]
    fn latex_overlays_mathbf() {
        let result = latex_overlays(r"\mathbf{b}".into());
        let v: serde_json::Value = serde_json::from_str(&result).unwrap();
        let arr = v.as_array().unwrap();
        assert!(!arr.is_empty());
        // First overlay should replace with 𝐛
        assert_eq!(arr[0]["replacement"], "𝐛");
    }

    #[test]
    fn latex_overlays_empty() {
        let result = latex_overlays("x + y = z".into());
        let v: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert!(v.as_array().unwrap().is_empty());
    }

    #[test]
    fn table_is_sorted() {
        for i in 1..SYMBOLS.len() {
            assert!(
                SYMBOLS[i - 1].0 < SYMBOLS[i].0,
                "Table not sorted at index {i}: {:?} >= {:?}",
                SYMBOLS[i - 1].0,
                SYMBOLS[i].0
            );
        }
    }

    #[test]
    fn latex_overlays_text_command() {
        let result = latex_overlays(r"\text{otherwise}".into());
        let v: serde_json::Value = serde_json::from_str(&result).unwrap();
        let arr = v.as_array().unwrap();
        // \text{ should be hidden, } should be hidden, content preserved
        let hidden: Vec<(i64, &str)> = arr
            .iter()
            .filter_map(|o| {
                let off = o["offset"].as_i64().unwrap();
                let rep = o["replacement"].as_str().unwrap();
                if rep.is_empty() {
                    Some((off, rep))
                } else {
                    None
                }
            })
            .collect();
        // The backslash, "text", and "{" should be hidden
        assert!(hidden.iter().any(|(off, _)| *off == 0));
        // The closing } should be hidden
        assert!(hidden
            .iter()
            .any(|(off, _)| *off as usize == r"\text{otherwise}".len() - 1));
    }

    #[test]
    fn latex_overlays_cases_env() {
        let input = r"\begin{cases} 1 & 0 \leq n \leq 2 \\ 0 & \text{otherwise} \end{cases}";
        let result = latex_overlays(input.to_string());
        let v: serde_json::Value = serde_json::from_str(&result).unwrap();
        let arr = v.as_array().unwrap();
        let replacements: Vec<&str> = arr
            .iter()
            .map(|o| o["replacement"].as_str().unwrap())
            .collect();
        // Should have ⎧ (open fence) and ⎩ (close fence) and ⎨ (mid fence)
        assert!(
            replacements.contains(&"⎧"),
            "Expected ⎧ in {:?}",
            replacements
        );
        assert!(
            replacements.contains(&"⎩"),
            "Expected ⎩ in {:?}",
            replacements
        );
        assert!(
            replacements.contains(&"⎨"),
            "Expected ⎨ in {:?}",
            replacements
        );
        // Should have ≤ from \leq
        assert!(
            replacements.contains(&"≤"),
            "Expected ≤ from \\leq in {:?}",
            replacements
        );
        // & should become space
        assert!(
            replacements.contains(&" "),
            "Expected space from & in {:?}",
            replacements
        );
    }

    #[test]
    fn latex_overlays_pmatrix_env() {
        let input = r"\begin{pmatrix} 1 & 0 \\ 0 & 1 \end{pmatrix}";
        let result = latex_overlays(input.to_string());
        let v: serde_json::Value = serde_json::from_str(&result).unwrap();
        let arr = v.as_array().unwrap();
        let replacements: Vec<&str> = arr
            .iter()
            .map(|o| o["replacement"].as_str().unwrap())
            .collect();
        assert!(replacements.contains(&"⎛"));
        assert!(replacements.contains(&"⎜"));
        assert!(replacements.contains(&"⎞"));
    }

    #[test]
    fn latex_overlays_subscript() {
        let result = latex_overlays("h_{n}".into());
        let v: serde_json::Value = serde_json::from_str(&result).unwrap();
        let arr = v.as_array().unwrap();
        let replacements: Vec<&str> = arr
            .iter()
            .map(|o| o["replacement"].as_str().unwrap())
            .collect();
        assert!(
            replacements.contains(&"ₙ"),
            "Expected ₙ subscript in {:?}",
            replacements
        );
    }

    #[test]
    fn latex_overlays_norm_delimiter() {
        let result = latex_overlays(r"\|B - B_1\|".into());
        let v: serde_json::Value = serde_json::from_str(&result).unwrap();
        let arr = v.as_array().unwrap();
        let replacements: Vec<&str> = arr
            .iter()
            .map(|o| o["replacement"].as_str().unwrap())
            .collect();
        assert!(
            replacements.contains(&"‖"),
            "Expected ‖ from \\| in {:?}",
            replacements
        );
    }

    #[test]
    fn latex_overlays_user_example() {
        let input = r"h_n = \begin{cases} 1 & 0 \leq n \leq 2 \\ 0 & \text{otherwise} \end{cases}";
        let result = latex_overlays(input.to_string());
        let v: serde_json::Value = serde_json::from_str(&result).unwrap();
        let arr = v.as_array().unwrap();
        let replacements: Vec<&str> = arr
            .iter()
            .map(|o| o["replacement"].as_str().unwrap())
            .collect();
        assert!(
            replacements.contains(&"ₙ"),
            "Expected ₙ in {:?}",
            replacements
        );
        assert!(
            replacements.contains(&"⎧"),
            "Expected ⎧ in {:?}",
            replacements
        );
        assert!(
            replacements.contains(&"⎨"),
            "Expected ⎨ in {:?}",
            replacements
        );
        assert!(
            replacements.contains(&"⎩"),
            "Expected ⎩ in {:?}",
            replacements
        );
        assert!(
            replacements.contains(&"≤"),
            "Expected ≤ in {:?}",
            replacements
        );
    }

    #[test]
    fn latex_overlays_frac() {
        let result = latex_overlays(r"\frac{a}{b}".into());
        let v: serde_json::Value = serde_json::from_str(&result).unwrap();
        let arr = v.as_array().unwrap();
        let replacements: Vec<&str> = arr
            .iter()
            .map(|o| o["replacement"].as_str().unwrap())
            .collect();
        assert!(
            replacements.contains(&"⁄"),
            "Expected ⁄ fraction slash in {:?}",
            replacements
        );
        let hidden_offsets: Vec<i64> = arr
            .iter()
            .filter(|o| o["replacement"].as_str().unwrap() == "")
            .map(|o| o["offset"].as_i64().unwrap())
            .collect();
        assert!(
            hidden_offsets.contains(&0),
            "\\frac at offset 0 should be hidden"
        );
    }

    #[test]
    fn latex_overlays_frac_nested() {
        let result = latex_overlays(r"\frac{1}{\frac{a}{b}}".into());
        let v: serde_json::Value = serde_json::from_str(&result).unwrap();
        let arr = v.as_array().unwrap();
        let replacements: Vec<&str> = arr
            .iter()
            .map(|o| o["replacement"].as_str().unwrap())
            .collect();
        let frac_count = replacements.iter().filter(|&&r| r == "⁄").count();
        assert!(
            frac_count >= 2,
            "Expected at least 2 fraction slashes in nested frac, got {}",
            frac_count
        );
    }

    #[test]
    fn compute_conceal_overlays_with_math_region() {
        let input = r#"some text $\alpha + \beta$ more text"#;
        let result = compute_conceal_overlays(input.to_string());
        let v: serde_json::Value = serde_json::from_str(&result).unwrap();
        let arr = v.as_array().unwrap();
        assert!(!arr.is_empty(), "Expected overlays for math region");
        let offsets: Vec<usize> = arr
            .iter()
            .map(|o| o["offset"].as_u64().unwrap() as usize)
            .collect();
        let alpha_offset = "some text $".len();
        assert!(
            offsets.contains(&alpha_offset),
            "Expected alpha at offset {} in: {:?}",
            alpha_offset,
            offsets
        );
    }

    #[test]
    fn compute_conceal_overlays_no_math() {
        let result = compute_conceal_overlays("plain text no math".to_string());
        assert_eq!(result, "[]");
    }

    // ── Comment-line conceal (TSV format) tests ──────────────────────────

    /// Parse TSV output into (char_offset, replacement) pairs for testing.
    fn parse_tsv(tsv: &str) -> Vec<(usize, String)> {
        tsv.lines()
            .filter(|l| !l.is_empty())
            .map(|line| {
                let mut parts = line.splitn(2, '\t');
                let offset: usize = parts.next().unwrap().parse().unwrap();
                let replacement = parts.next().unwrap_or("").to_string();
                (offset, replacement)
            })
            .collect()
    }

    #[test]
    fn comment_conceal_simple_alpha() {
        let input = "# $\\alpha$\n";
        let result = compute_conceal_overlays_for_comments(input.to_string());
        let overlays = parse_tsv(&result);
        assert!(!overlays.is_empty(), "Expected overlays, got none");
        // Should have α replacement
        assert!(
            overlays.iter().any(|(_, r)| r == "α"),
            "Expected α in {:?}",
            overlays
        );
        // $ delimiters should be hidden (empty replacement)
        let hidden: Vec<usize> = overlays
            .iter()
            .filter(|(_, r)| r.is_empty())
            .map(|(o, _)| *o)
            .collect();
        assert!(
            hidden.len() >= 2,
            "Expected at least 2 hidden delimiters, got {:?}",
            hidden
        );
    }

    #[test]
    fn comment_conceal_no_math_lines_skipped() {
        let input = "x = rand(10)\nprintln(\"value: $x\")\n";
        let result = compute_conceal_overlays_for_comments(input.to_string());
        assert!(result.is_empty(), "Non-comment lines should produce no overlays");
    }

    #[test]
    fn comment_conceal_mixed_code_and_comments() {
        let input = "x = 1\n# The value $\\beta$ is cool\ny = 2\n";
        let result = compute_conceal_overlays_for_comments(input.to_string());
        let overlays = parse_tsv(&result);
        assert!(
            overlays.iter().any(|(_, r)| r == "β"),
            "Expected β from comment line, got {:?}",
            overlays
        );
    }

    #[test]
    fn comment_conceal_dollar_in_code_ignored() {
        // Julia string interpolation — should NOT match across lines
        let input = "# cost is $5\n# price is $10\n";
        let result = compute_conceal_overlays_for_comments(input.to_string());
        let overlays = parse_tsv(&result);
        // $5 on one line should NOT match $10 on another
        // Each line has only one $, so no complete $...$ region
        assert!(
            overlays.is_empty(),
            "Single $ per line should not create regions, got {:?}",
            overlays
        );
    }

    #[test]
    fn comment_conceal_backslash_parens() {
        let input = "# \\(a\\) Find the eigenvalues\n";
        let result = compute_conceal_overlays_for_comments(input.to_string());
        let overlays = parse_tsv(&result);
        // \( and \) delimiters should be hidden
        let hidden_count = overlays.iter().filter(|(_, r)| r.is_empty()).count();
        assert!(
            hidden_count >= 4,
            "Expected \\( (2 chars) and \\) (2 chars) hidden, got {} hidden in {:?}",
            hidden_count,
            overlays
        );
    }

    #[test]
    fn comment_conceal_multiple_regions_one_line() {
        let input = "# $\\alpha$ and $\\beta$\n";
        let result = compute_conceal_overlays_for_comments(input.to_string());
        let overlays = parse_tsv(&result);
        let symbols: Vec<&str> = overlays.iter().map(|(_, r)| r.as_str()).collect();
        assert!(symbols.contains(&"α"), "Expected α in {:?}", symbols);
        assert!(symbols.contains(&"β"), "Expected β in {:?}", symbols);
    }

    #[test]
    fn comment_conceal_mathbf() {
        let input = "# $\\mathbf{v}$\n";
        let result = compute_conceal_overlays_for_comments(input.to_string());
        let overlays = parse_tsv(&result);
        assert!(
            overlays.iter().any(|(_, r)| r == "𝐯"),
            "Expected 𝐯 (bold v) in {:?}",
            overlays
        );
    }

    #[test]
    fn comment_conceal_lambda_equation() {
        let input = "# $A\\mathbf{v} = \\lambda\\mathbf{v}$\n";
        let result = compute_conceal_overlays_for_comments(input.to_string());
        let overlays = parse_tsv(&result);
        let symbols: Vec<&str> = overlays
            .iter()
            .filter(|(_, r)| !r.is_empty())
            .map(|(_, r)| r.as_str())
            .collect();
        assert!(symbols.contains(&"λ"), "Expected λ in {:?}", symbols);
        assert!(symbols.contains(&"𝐯"), "Expected 𝐯 in {:?}", symbols);
    }

    #[test]
    fn comment_conceal_offsets_are_char_not_byte() {
        // The ═ character is 3 bytes in UTF-8.
        // Offsets must be char positions, not byte positions.
        let input = "# ═══ separator\n# $\\alpha$\n";
        let result = compute_conceal_overlays_for_comments(input.to_string());
        let overlays = parse_tsv(&result);
        let alpha_overlay = overlays.iter().find(|(_, r)| r == "α");
        assert!(alpha_overlay.is_some(), "Expected α overlay");
        let (offset, _) = alpha_overlay.unwrap();
        // "# ═══ separator\n# $" = 2 + 3 + 1 + 10 + 1 + 2 + 1 = 20 chars (not bytes)
        // The α should be at the char position of \ in \alpha
        // Line 2: "# $\alpha$" — content starts at char offset of first char after "# "
        // on line 2. The $ is at some char offset, \alpha starts 1 char after.
        // Key check: offset should be reasonable (< 30), not inflated by multi-byte chars
        assert!(
            *offset < 30,
            "Char offset {} seems too large — possibly using byte offsets",
            offset
        );
    }

    #[test]
    fn comment_conceal_user_exercise_line_no_stray_overlays() {
        // Reproduces the exact line from the user's bug report where
        // "numerically" was showing as "numeially" and "maximum" as "maium".
        // Any overlay that lands OUTSIDE the two math regions is a bug.
        let input = "# \\(b\\) Verify the eigenvalue equation $A\\mathbf{v} = \\lambda\\mathbf{v}$ numerically for each eigenpair. What is the maximum residual $\\|A\\mathbf{v} - \\lambda\\mathbf{v}\\|$?\n";
        let result = compute_conceal_overlays_for_comments(input.to_string());
        let overlays = parse_tsv(&result);

        // Locate "numerically" and "maximum" char positions in the input.
        let doc_chars: Vec<char> = input.chars().collect();
        let find_word = |word: &str| -> (usize, usize) {
            let s: String = doc_chars.iter().collect();
            let byte_idx = s.find(word).unwrap();
            let char_idx = s[..byte_idx].chars().count();
            (char_idx, char_idx + word.chars().count())
        };
        let (num_start, num_end) = find_word("numerically");
        let (max_start, max_end) = find_word("maximum");

        for (off, rep) in &overlays {
            assert!(
                !(*off >= num_start && *off < num_end),
                "Stray overlay inside 'numerically' (chars {}..{}): ({}, {:?})",
                num_start, num_end, off, rep
            );
            assert!(
                !(*off >= max_start && *off < max_end),
                "Stray overlay inside 'maximum' (chars {}..{}): ({}, {:?})",
                max_start, max_end, off, rep
            );
        }
    }

    #[test]
    fn comment_conceal_empty_document() {
        let result = compute_conceal_overlays_for_comments("".to_string());
        assert!(result.is_empty());
    }

    #[test]
    fn comment_conceal_no_comment_lines() {
        let input = "x = 1\ny = 2\nz = x + y\n";
        let result = compute_conceal_overlays_for_comments(input.to_string());
        assert!(result.is_empty());
    }

}
