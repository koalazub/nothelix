//! Identify `$...$`, `$$...$$` and `\(...\)` math regions in a document.
//!
//! The scanner itself runs on one math region at a time. This module is
//! what decides where those regions start and end. The returned ranges
//! are byte offsets for the region CONTENTS, not including the delimiters.

/// Scan `text` and return `(region_start, region_end)` byte-offset pairs
/// for every math region found. The ranges cover the region's contents
/// (excluding the `$`, `$$`, or `\(`/`\)` delimiters themselves).
///
/// Regions are non-overlapping and in source order. Nested math is not
/// supported — the first closing delimiter wins.
pub(crate) fn find_math_regions(text: &str) -> Vec<(usize, usize)> {
    let bytes = text.as_bytes();
    let len = bytes.len();
    let mut regions = Vec::new();
    let mut i = 0;

    while i < len {
        if bytes[i] == b'$' {
            if i + 1 < len && bytes[i + 1] == b'$' {
                // $$...$$
                let start = i + 2;
                let mut j = start;
                while j + 1 < len {
                    if bytes[j] == b'$' && bytes[j + 1] == b'$' {
                        regions.push((start, j));
                        i = j + 2;
                        break;
                    }
                    j += 1;
                }
                if j + 1 >= len {
                    break;
                }
            } else {
                // $...$
                let start = i + 1;
                let mut j = start;
                while j < len {
                    if bytes[j] == b'$' {
                        regions.push((start, j));
                        i = j + 1;
                        break;
                    }
                    j += 1;
                }
                if j >= len {
                    break;
                }
            }
        } else if bytes[i] == b'\\' && i + 1 < len && bytes[i + 1] == b'(' {
            // \(...\) — but reject markdown-escaped parens like \(a\), \(b\)
            let start = i + 2;
            let mut j = start;
            while j + 1 < len {
                if bytes[j] == b'\\' && bytes[j + 1] == b')' {
                    let content = &text[start..j];
                    if looks_like_math(content) {
                        regions.push((start, j));
                    }
                    i = j + 2;
                    break;
                }
                j += 1;
            }
            if j + 1 >= len {
                break;
            }
        } else {
            i += 1;
        }
    }

    regions
}

/// Heuristic: does the content between `\(` and `\)` look like actual
/// LaTeX math, or is it just a markdown-escaped parenthetical like `\(a\)`?
///
/// Real math contains backslash commands, superscripts, subscripts, or is
/// longer than a few characters. A single letter/digit is almost certainly
/// a list marker: `\(a\)`, `\(b\)`, `\(i\)`, `\(1\)`.
fn looks_like_math(content: &str) -> bool {
    // Single char or empty → not math (it's \(a\), \(b\), etc.)
    if content.len() <= 2 {
        return false;
    }
    // Contains LaTeX indicators → definitely math
    let bytes = content.as_bytes();
    for &b in bytes {
        if b == b'\\' || b == b'^' || b == b'_' || b == b'{' {
            return true;
        }
    }
    // Longer than a short word and contains math-like chars
    content.len() > 4 || content.contains('+') || content.contains('=') || content.contains('-')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_markdown_escaped_parens() {
        // \(a\) and \(b\) are markdown list markers, not math
        let text = r"\(a\) First item \(b\) Second item";
        let regions = find_math_regions(text);
        assert!(regions.is_empty(), "should reject \\(a\\) as not math, got: {regions:?}");
    }

    #[test]
    fn accepts_real_inline_math() {
        let text = r"\(\alpha + \beta\)";
        let regions = find_math_regions(text);
        assert_eq!(regions.len(), 1);
        assert_eq!(&text[regions[0].0..regions[0].1], r"\alpha + \beta");
    }

    #[test]
    fn inline_dollar_still_works() {
        let text = r"$H(e^{j\omega})$";
        let regions = find_math_regions(text);
        assert_eq!(regions.len(), 1);
    }

    #[test]
    fn mixed_escaped_parens_and_real_math() {
        let text = r"\(b\) Compute $H(e^{j\omega})$. For $\omega \in [-\pi, \pi]$";
        let regions = find_math_regions(text);
        // Should find the two $...$ regions but NOT the \(b\)
        assert_eq!(regions.len(), 2, "got: {regions:?}");
    }

    #[test]
    fn exact_user_line_bandwidth() {
        let text = r"\(a\) What is the bandwidth of $x$ (in Hz)? What is the Nyquist rate? \[1 mark\]";
        let regions = find_math_regions(text);
        assert_eq!(regions.len(), 1, "should find $x$, got: {regions:?}");
        assert_eq!(&text[regions[0].0..regions[0].1], "x");
    }
}
