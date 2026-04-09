//! Typed conceal overlay record used by the scanner.
//!
//! A single conceal overlay is an instruction: "at this byte offset in the
//! scanned text, replace exactly one grapheme with this string". Empty
//! replacement means "hide the grapheme". This mirrors Helix's
//! `helix_core::text_annotations::Overlay` but uses byte offsets; the Scheme
//! layer converts to char offsets before handing to Helix.
//!
//! Using `Cow<'static, str>` lets the scanner emit static glyphs ("λ", "⎧",
//! "⁰", "") without allocation, and still carry owned strings from the
//! unicode symbol table when needed.

use std::borrow::Cow;

use serde::Serialize;

/// One entry in the JSON overlay output.
#[derive(Debug, Clone, Serialize)]
pub(crate) struct Overlay {
    pub offset: usize,
    pub replacement: Cow<'static, str>,
}

impl Overlay {
    /// Overlay that hides a single byte (empty replacement, zero width).
    #[inline]
    pub fn hide(offset: usize) -> Self {
        Overlay {
            offset,
            replacement: Cow::Borrowed(""),
        }
    }

    /// Overlay that replaces a single byte with `replacement`.
    #[inline]
    pub fn at(offset: usize, replacement: impl Into<Cow<'static, str>>) -> Self {
        Overlay {
            offset,
            replacement: replacement.into(),
        }
    }

    /// Append `hide` overlays for every byte in `[start, end)`.
    #[inline]
    pub fn hide_range(overlays: &mut Vec<Overlay>, start: usize, end: usize) {
        for k in start..end {
            overlays.push(Overlay::hide(k));
        }
    }
}
