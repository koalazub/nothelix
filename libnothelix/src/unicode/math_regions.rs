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
pub(super) fn find_math_regions(text: &str) -> Vec<(usize, usize)> {
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
            // \(...\)
            let start = i + 2;
            let mut j = start;
            while j + 1 < len {
                if bytes[j] == b'\\' && bytes[j + 1] == b')' {
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
            i += 1;
        }
    }

    regions
}
