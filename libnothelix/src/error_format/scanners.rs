//! Tokenizing primitives for the enrichment pipeline.
//!
//! Every scanner in the enrichment pipeline ultimately needs the same
//! handful of low-level operations: track a byte position, balance a
//! bracket pair, recognise an identifier, walk past a string literal,
//! split a string on comma boundaries that respect nesting. This module
//! collects those primitives behind one tested surface so each
//! enrichment scanner becomes a short composition rather than a 50-
//! line nested-while-loop that re-derives the bracket-balance state
//! machine.
//!
//! Designed so each primitive is independently usable. The `Scanner`
//! struct is a cursor wrapper; the free functions take `&str` directly
//! and don't require a `Scanner` to be constructed. Compose as needed.

/// Cursor over a `&str` viewed as bytes, tracked by byte position.
/// Equivalent in spirit to `Peekable<Chars>`, but works on bytes so
/// callers can do bracket-matching, escape-aware string skipping, and
/// other operations that need byte-level precision.
pub(super) struct Scanner<'a> {
    bytes: &'a [u8],
    pos: usize,
}

#[allow(dead_code)] // Scanner::pos / is_at_end are public methods retained
                    // for callers that need to inspect cursor state. New
                    // enrichment scanners may grow into them.
impl<'a> Scanner<'a> {
    pub fn new(s: &'a str) -> Self {
        Self { bytes: s.as_bytes(), pos: 0 }
    }

    pub fn pos(&self) -> usize {
        self.pos
    }

    pub fn is_at_end(&self) -> bool {
        self.pos >= self.bytes.len()
    }

    pub fn peek(&self) -> Option<u8> {
        self.bytes.get(self.pos).copied()
    }

    pub fn advance(&mut self) -> Option<u8> {
        let b = self.peek()?;
        self.pos += 1;
        Some(b)
    }

    /// Move forward `n` bytes (saturating at end).
    pub fn skip(&mut self, n: usize) {
        self.pos = self.pos.saturating_add(n).min(self.bytes.len());
    }

    /// Scan an ASCII Julia identifier starting at the current position
    /// (`[a-zA-Z_][a-zA-Z0-9_!]*`). Returns the matched slice or `None`
    /// when not currently at an identifier-start byte. Position is
    /// advanced to just past the identifier on success.
    pub fn scan_identifier(&mut self) -> Option<&'a str> {
        let start = self.pos;
        let first = self.peek()?;
        if !(first.is_ascii_alphabetic() || first == b'_') {
            return None;
        }
        while let Some(b) = self.peek() {
            if b.is_ascii_alphanumeric() || b == b'_' || b == b'!' {
                self.pos += 1;
            } else {
                break;
            }
        }
        // self.bytes is the byte view of a valid &str; any contiguous
        // range that respects char boundaries (which `[A-Za-z0-9_!]*`
        // does, since those are 1-byte ASCII) is valid UTF-8.
        std::str::from_utf8(&self.bytes[start..self.pos]).ok()
    }

    /// Walk past a double-quoted string literal starting at the current
    /// position. Caller should check `peek() == Some(b'"')` first;
    /// otherwise this is a no-op. Consumes the closing quote and
    /// honours `\"`/`\\` escapes. Returns `true` if the string was
    /// closed before EOF.
    pub fn skip_string_literal(&mut self) -> bool {
        if self.peek() != Some(b'"') {
            return false;
        }
        self.pos += 1; // opening quote
        while let Some(b) = self.peek() {
            self.pos += 1;
            match b {
                b'\\' => self.skip(1), // skip next byte (the escape target)
                b'"' => return true,
                _ => {}
            }
        }
        false
    }
}

// ─── Bracket primitives ──────────────────────────────────────────────────────

/// Find the index of the byte matching `open` at `open_idx` to its
/// closing counterpart `close`, tracking nested pairs of the same
/// shape. Returns `None` when the input doesn't open with `open` at
/// `open_idx` or when the close is never reached.
pub(super) fn find_matching_close(
    bytes: &[u8],
    open_idx: usize,
    open: u8,
    close: u8,
) -> Option<usize> {
    if bytes.get(open_idx).copied() != Some(open) {
        return None;
    }
    let mut depth: i32 = 1;
    let mut i = open_idx + 1;
    while i < bytes.len() {
        let b = bytes[i];
        if b == open {
            depth += 1;
        } else if b == close {
            depth -= 1;
            if depth == 0 {
                return Some(i);
            }
        }
        i += 1;
    }
    None
}

/// Parenthesis specialisation of [`find_matching_close`].
pub(super) fn find_matching_paren(bytes: &[u8], open_idx: usize) -> Option<usize> {
    find_matching_close(bytes, open_idx, b'(', b')')
}

// ─── Comma splitting ─────────────────────────────────────────────────────────

/// Split `s` on commas that are at bracket-depth 0 (top-level only).
/// `f(a, b)` and `[a, b]` are NOT split open. Each returned arg has
/// surrounding whitespace trimmed; empty fragments are dropped.
pub(super) fn split_top_level_commas(s: &str) -> Vec<String> {
    let mut args = Vec::new();
    let mut current = String::new();
    let mut depth = 0i32;
    for ch in s.chars() {
        match ch {
            '(' | '[' | '{' => {
                depth += 1;
                current.push(ch);
            }
            ')' | ']' | '}' => {
                depth -= 1;
                current.push(ch);
            }
            ',' if depth == 0 => {
                let trimmed = current.trim();
                if !trimmed.is_empty() {
                    args.push(trimmed.to_string());
                }
                current.clear();
            }
            _ => current.push(ch),
        }
    }
    let trimmed = current.trim();
    if !trimmed.is_empty() {
        args.push(trimmed.to_string());
    }
    args
}

// ─── Identifier predicates ───────────────────────────────────────────────────

/// Is `s` a valid Julia identifier (`[a-zA-Z_][a-zA-Z0-9_!]*`)?
pub(super) fn is_identifier(s: &str) -> bool {
    let mut chars = s.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    if !(first.is_ascii_alphabetic() || first == '_') {
        return false;
    }
    chars.all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '!')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scanner_advances_byte_by_byte() {
        let mut s = Scanner::new("abc");
        assert_eq!(s.peek(), Some(b'a'));
        assert_eq!(s.advance(), Some(b'a'));
        assert_eq!(s.advance(), Some(b'b'));
        assert_eq!(s.advance(), Some(b'c'));
        assert_eq!(s.advance(), None);
        assert!(s.is_at_end());
    }

    #[test]
    fn scan_identifier_handles_julia_syntax() {
        // Bang-suffix names like `push!` are valid Julia identifiers.
        let mut s = Scanner::new("push!(x)");
        assert_eq!(s.scan_identifier(), Some("push!"));
        assert_eq!(s.pos(), 5);
        assert_eq!(s.peek(), Some(b'('));
    }

    #[test]
    fn scan_identifier_rejects_non_letter_starts() {
        let mut s = Scanner::new("9foo");
        assert_eq!(s.scan_identifier(), None);
        // Cursor must NOT advance on a non-match.
        assert_eq!(s.pos(), 0);
    }

    #[test]
    fn skip_string_literal_honours_escapes() {
        // The escaped quote must NOT close the string.
        let mut s = Scanner::new(r#""a\"b" after"#);
        assert!(s.skip_string_literal());
        // After consuming the closing quote, we should be at ` after`.
        assert_eq!(s.peek(), Some(b' '));
    }

    #[test]
    fn find_matching_paren_handles_nesting() {
        let bytes = b"f(g(x), y)";
        assert_eq!(find_matching_paren(bytes, 1), Some(9));
    }

    #[test]
    fn find_matching_close_requires_open_byte_at_idx() {
        // Wrong byte at open_idx -> None.
        assert_eq!(find_matching_close(b"abc", 0, b'(', b')'), None);
    }

    #[test]
    fn split_top_level_commas_respects_nesting() {
        let parts = split_top_level_commas("a, f(b, c), [d, e]");
        assert_eq!(parts, vec!["a", "f(b, c)", "[d, e]"]);
    }

    #[test]
    fn split_top_level_commas_drops_empty_fragments() {
        let parts = split_top_level_commas(",a,, b,");
        assert_eq!(parts, vec!["a", "b"]);
    }

    #[test]
    fn is_identifier_accepts_standard_forms() {
        assert!(is_identifier("foo"));
        assert!(is_identifier("foo_bar"));
        assert!(is_identifier("foo!"));
        assert!(is_identifier("_private"));
        assert!(is_identifier("a1b2"));
    }

    #[test]
    fn is_identifier_rejects_invalid_forms() {
        assert!(!is_identifier(""));
        assert!(!is_identifier("9foo"));
        assert!(!is_identifier("foo bar"));
        assert!(!is_identifier("foo-bar"));
    }
}
