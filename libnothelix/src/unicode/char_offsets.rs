use crate::error::{Error, Result};

pub(super) struct CharOffsets {
    by_byte: Vec<usize>,
    char_len: usize,
}

impl CharOffsets {
    pub fn of(text: &str) -> Self {
        let mut by_byte = vec![0usize; text.len() + 1];
        let mut char_len = 0;
        for (byte, ch) in text.char_indices() {
            for slot in &mut by_byte[byte..byte + ch.len_utf8()] {
                *slot = char_len;
            }
            char_len += 1;
        }
        by_byte[text.len()] = char_len;
        Self { by_byte, char_len }
    }

    pub fn visible(&self, byte: usize) -> Result<Option<usize>> {
        let char_offset = *self.by_byte.get(byte).ok_or_else(|| Error::Malformed {
            subject: "conceal overlay offset",
            detail: format!(
                "byte {byte} lies past the end of the {} byte document it was scanned from",
                self.by_byte.len() - 1
            ),
        })?;
        Ok((char_offset < self.char_len).then_some(char_offset))
    }
}
