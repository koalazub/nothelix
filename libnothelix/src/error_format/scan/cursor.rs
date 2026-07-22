pub struct Scanner<'a> {
    bytes: &'a [u8],
    pos: usize,
}

impl<'a> Scanner<'a> {
    pub fn new(s: &'a str) -> Self {
        Self {
            bytes: s.as_bytes(),
            pos: 0,
        }
    }

    pub fn peek(&self) -> Option<u8> {
        self.bytes.get(self.pos).copied()
    }

    pub fn advance(&mut self) -> Option<u8> {
        let b = self.peek()?;
        self.pos += 1;
        Some(b)
    }

    fn skip(&mut self, n: usize) {
        self.pos = self.pos.saturating_add(n).min(self.bytes.len());
    }

    pub fn scan_identifier(&mut self) -> Option<&'a str> {
        let start = self.pos;
        if !self.peek().is_some_and(is_identifier_start) {
            return None;
        }
        while self.peek().is_some_and(is_identifier_byte) {
            self.pos += 1;
        }
        std::str::from_utf8(&self.bytes[start..self.pos]).ok()
    }

    pub fn skip_string_literal(&mut self) -> bool {
        if self.peek() != Some(b'"') {
            return false;
        }
        self.pos += 1;
        while let Some(b) = self.peek() {
            self.pos += 1;
            match b {
                b'\\' => self.skip(1),
                b'"' => return true,
                _ => {}
            }
        }
        false
    }
}

fn is_identifier_start(b: u8) -> bool {
    b.is_ascii_alphabetic() || b == b'_'
}

fn is_identifier_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_' || b == b'!'
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
        assert_eq!(s.peek(), None);
    }

    #[test]
    fn scan_identifier_handles_julia_syntax() {
        let mut s = Scanner::new("push!(x)");
        assert_eq!(s.scan_identifier(), Some("push!"));
        assert_eq!(s.peek(), Some(b'('));
    }

    #[test]
    fn scan_identifier_rejects_non_letter_starts() {
        let mut s = Scanner::new("9foo");
        assert_eq!(s.scan_identifier(), None);
        assert_eq!(s.peek(), Some(b'9'));
    }

    #[test]
    fn skip_string_literal_honours_escapes() {
        let mut s = Scanner::new(r#""a\"b" after"#);
        assert!(s.skip_string_literal());
        assert_eq!(s.peek(), Some(b' '));
    }
}
