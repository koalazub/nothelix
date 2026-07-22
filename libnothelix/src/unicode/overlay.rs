use std::borrow::Cow;

use serde::Serialize;

use super::char_offsets::CharOffsets;
use crate::error::Result;

#[derive(Debug, Clone, Serialize)]
pub(crate) struct Overlay {
    pub offset: usize,
    pub replacement: Cow<'static, str>,
}

impl Overlay {
    #[inline]
    pub fn hide(offset: usize) -> Self {
        Overlay {
            offset,
            replacement: Cow::Borrowed(""),
        }
    }

    #[inline]
    pub fn at(offset: usize, replacement: impl Into<Cow<'static, str>>) -> Self {
        Overlay {
            offset,
            replacement: replacement.into(),
        }
    }
}

pub(super) struct CharOffsetTsv<'a> {
    offsets: &'a CharOffsets,
    rows: String,
}

impl<'a> CharOffsetTsv<'a> {
    pub fn new(offsets: &'a CharOffsets) -> Self {
        Self {
            offsets,
            rows: String::new(),
        }
    }

    pub fn push(&mut self, byte: usize, replacement: &str) -> Result<()> {
        if let Some(char_offset) = self.offsets.visible(byte)? {
            self.rows.push_str(&char_offset.to_string());
            self.rows.push('\t');
            self.rows.push_str(replacement);
            self.rows.push('\n');
        }
        Ok(())
    }

    pub fn hide(&mut self, byte: usize) -> Result<()> {
        self.push(byte, "")
    }

    pub fn into_rows(self) -> String {
        self.rows
    }
}
