//! Byte-offset LaTeX scanner.
//!
//! Walks a math region's text once, dispatching to a focused `scan_*`
//! method per case. Each method takes the current cursor position and
//! returns the position to continue from — no implicit `i` mutation
//! leaks between cases.
//!
//! The old monolithic `latex_overlays` body was a 450-line nested if/else
//! chain; extracting the cases into methods removes the accidental
//! coupling where one arm could silently leak state into another.

use std::borrow::Cow;

use super::fence::{close_fence, mid_fence, open_fence};
use super::overlay::Overlay;
use super::sub_super::{latex_font_to_julia, map_lookup, SUB_MAP, SUPER_MAP};
use super::symbol_table::unicode_lookup;

/// Track which environment we're inside and which row we're on within it.
/// This lets the scanner emit the right Unicode fence character on each row
/// boundary within a matrix-style environment.
struct EnvState {
    env_name: String,
    row: usize,
    total_rows: usize,
}

/// Count rows in environment content for fence character selection.
/// Row count = number of `\\` delimiters + 1, within each env.
fn count_rows_in_env(text: &str, env_start: usize, env_end: usize) -> usize {
    let content = &text[env_start..env_end];
    let mut rows = 1;
    let mut k = 0;
    let b = content.as_bytes();
    while k + 1 < b.len() {
        if b[k] == b'\\' && b[k + 1] == b'\\' {
            rows += 1;
            k += 2;
        } else {
            k += 1;
        }
    }
    rows
}

/// Byte-offset LaTeX scanner. Walks text once, dispatching to a focused
/// `scan_*` method per case. Each method takes the current cursor position
/// and returns the position to continue from — no implicit `i` mutation.
///
/// The old monolithic `latex_overlays` body was a 450-line nested if/else
/// chain; extracting the cases into methods removes the accidental coupling
/// where one arm could silently leak state into another.
struct Scanner<'a> {
    text: &'a str,
    bytes: &'a [u8],
    overlays: Vec<Overlay>,
    env_stack: Vec<EnvState>,
}

impl<'a> Scanner<'a> {
    fn new(text: &'a str) -> Self {
        Self {
            text,
            bytes: text.as_bytes(),
            overlays: Vec::new(),
            env_stack: Vec::new(),
        }
    }

    /// Drive the scanner to completion and return the accumulated overlays.
    fn scan(mut self) -> Vec<Overlay> {
        let mut i = 0;
        while i < self.bytes.len() {
            i = self.step(i);
        }
        self.overlays
    }

    /// Dispatch from position `i` to the matching case method.
    /// Returns the next position to scan from.
    fn step(&mut self, i: usize) -> usize {
        let b = self.bytes;
        let len = b.len();

        // \alpha, \frac, \mathbf, \begin, \end, \text, ...
        if b[i] == b'\\' && i + 1 < len && b[i + 1].is_ascii_alphabetic() {
            return self.scan_backslash_command(i);
        }
        // \\ row separator (must come BEFORE the non-alpha backslash case
        // because \\ starts with a non-alpha second byte)
        if b[i] == b'\\' && i + 1 < len && b[i + 1] == b'\\' {
            return self.scan_row_separator(i);
        }
        // \| \{ \} \, \; \! \space \( \) \[ \]
        if b[i] == b'\\' && i + 1 < len && !b[i + 1].is_ascii_alphabetic() {
            return self.scan_non_alpha_backslash(i);
        }
        // & column separator (only inside matrix-style environments)
        if b[i] == b'&' && !self.env_stack.is_empty() {
            self.overlays.push(Overlay::at(i, " "));
            return i + 1;
        }
        // ^{...}
        if b[i] == b'^' && i + 1 < len && b[i + 1] == b'{' {
            return self.scan_braced_superscript(i);
        }
        // ^x (digit, n, i, +/-/=, parens)
        if b[i] == b'^'
            && i + 1 < len
            && !b[i + 1].is_ascii_alphabetic()
            && b[i + 1] != b'{'
            && b[i + 1] != b'\\'
        {
            return self.scan_inline_superscript(i);
        }
        // _{...}
        if b[i] == b'_' && i + 1 < len && b[i + 1] == b'{' {
            return self.scan_braced_subscript(i);
        }
        // _x
        if b[i] == b'_'
            && i + 1 < len
            && b[i + 1] != b'{'
            && b[i + 1] != b'\\'
            && b[i + 1] != b'_'
        {
            return self.scan_inline_subscript(i);
        }

        i + 1
    }

    /// Parse a `\commandname` and dispatch to the matching sub-case.
    fn scan_backslash_command(&mut self, start: usize) -> usize {
        let cmd_start = start;
        let mut i = start + 1; // skip backslash
        let name_start = i;
        while i < self.bytes.len() && self.bytes[i].is_ascii_alphabetic() {
            i += 1;
        }
        let name_end = i;
        let name = &self.text[name_start..name_end];

        match name {
            "begin" => self.scan_begin_env(cmd_start, name_end),
            "end" => self.scan_end_env(cmd_start, name_end),
            "text" | "mathrm" | "operatorname" => self.scan_text_command(cmd_start, name_end),
            "mathbf" | "textbf" | "boldsymbol" | "mathbb" | "mathcal" | "cal" | "mathfrak"
            | "frak" | "mathit" | "textit" | "mathsf" | "mathtt" => {
                self.scan_font_command(cmd_start, name_end, name)
            }
            "frac" | "dfrac" | "tfrac" => self.scan_frac_command(cmd_start, name_end),
            _ => self.scan_simple_command(cmd_start, name_end, name),
        }
    }

    /// `\begin{env_name}` — push an env onto the stack and emit the opening fence.
    fn scan_begin_env(&mut self, cmd_start: usize, mut i: usize) -> usize {
        while i < self.bytes.len() && self.bytes[i] == b' ' {
            i += 1;
        }
        if i >= self.bytes.len() || self.bytes[i] != b'{' {
            return i;
        }
        i += 1;
        let env_name_start = i;
        while i < self.bytes.len() && self.bytes[i] != b'}' {
            i += 1;
        }
        let env_name = self.text[env_name_start..i].to_string();
        if i < self.bytes.len() {
            i += 1; // skip }
        }

        // Find matching \end{env_name}.
        let end_tag = format!("\\end{{{}}}", env_name);
        let env_content_start = i;
        let env_end_pos = self.text[env_content_start..]
            .find(&end_tag)
            .map(|pos| env_content_start + pos)
            .unwrap_or(self.text.len());

        let total_rows = count_rows_in_env(self.text, env_content_start, env_end_pos);

        Overlay::hide_range(&mut self.overlays, cmd_start, i);

        let fence = open_fence(&env_name, total_rows);
        self.env_stack.push(EnvState {
            env_name,
            row: 0,
            total_rows,
        });
        if !fence.is_empty() {
            self.overlays.push(Overlay::at(cmd_start, fence));
        }
        i
    }

    /// `\end{env_name}` — pop the env stack and emit the closing fence.
    fn scan_end_env(&mut self, cmd_start: usize, mut i: usize) -> usize {
        while i < self.bytes.len() && self.bytes[i] == b' ' {
            i += 1;
        }
        if i >= self.bytes.len() || self.bytes[i] != b'{' {
            return i;
        }
        i += 1;
        while i < self.bytes.len() && self.bytes[i] != b'}' {
            i += 1;
        }
        if i < self.bytes.len() {
            i += 1;
        }
        let fence = self
            .env_stack
            .pop()
            .map(|env| close_fence(&env.env_name, env.total_rows))
            .unwrap_or_default();
        Overlay::hide_range(&mut self.overlays, cmd_start, i);
        if !fence.is_empty() {
            self.overlays.push(Overlay::at(i - 1, fence));
        }
        i
    }

    /// `\text{...}`, `\mathrm{...}`, `\operatorname{...}` — hide the command
    /// wrapper but keep the inner content visible.
    fn scan_text_command(&mut self, cmd_start: usize, mut i: usize) -> usize {
        while i < self.bytes.len() && self.bytes[i] == b' ' {
            i += 1;
        }
        if i >= self.bytes.len() || self.bytes[i] != b'{' {
            return i;
        }
        // Hide `\text{` (including the opening brace).
        Overlay::hide_range(&mut self.overlays, cmd_start, i + 1);
        i += 1;
        while i < self.bytes.len() && self.bytes[i] != b'}' {
            i += 1;
        }
        if i < self.bytes.len() {
            self.overlays.push(Overlay::hide(i));
            i += 1;
        }
        i
    }

    /// `\mathbf{v}`, `\mathbb{R}`, etc. — replace content letters with their
    /// Unicode math variants and hide the command wrapper.
    fn scan_font_command(&mut self, cmd_start: usize, i: usize, name: &str) -> usize {
        let mut j = i;
        while j < self.bytes.len() && self.bytes[j] == b' ' {
            j += 1;
        }
        if j >= self.bytes.len() || self.bytes[j] != b'{' {
            return i;
        }
        j += 1;
        let content_start = j;
        while j < self.bytes.len() && self.bytes[j] != b'}' {
            j += 1;
        }
        if j >= self.bytes.len() {
            return i;
        }
        let content = &self.text[content_start..j];
        j += 1; // skip }

        // Fast path: single-char content with a direct mapping.
        if content.len() == 1 {
            if let Some(replacement) = latex_font_to_julia(name, content) {
                self.overlays.push(Overlay::at(cmd_start, replacement));
                Overlay::hide_range(&mut self.overlays, cmd_start + 1, j);
                return j;
            }
        }

        // Multi-char: replace every char that has a mapping.
        let mut any_replaced = false;
        let mut replacements: Vec<Option<&'static str>> = Vec::new();
        for ch in content.chars() {
            if let Some(r) = latex_font_to_julia(name, &ch.to_string()) {
                replacements.push(Some(r));
                any_replaced = true;
            } else {
                replacements.push(None);
            }
        }
        if any_replaced {
            Overlay::hide_range(&mut self.overlays, cmd_start, content_start);
            let mut char_offset = content_start;
            for (ci, ch) in content.chars().enumerate() {
                if let Some(r) = replacements[ci] {
                    self.overlays.push(Overlay::at(char_offset, r));
                }
                char_offset += ch.len_utf8();
            }
            self.overlays.push(Overlay::hide(j - 1));
        }
        j
    }

    /// `\frac{num}{den}` / `\dfrac` / `\tfrac` — emit a fraction-slash glyph
    /// between the numerator and denominator and hide the command wrapper.
    fn scan_frac_command(&mut self, cmd_start: usize, i: usize) -> usize {
        let mut j = i;
        while j < self.bytes.len() && self.bytes[j] == b' ' {
            j += 1;
        }
        if j >= self.bytes.len() || self.bytes[j] != b'{' {
            // Malformed fallback: hide the whole command name.
            Overlay::hide_range(&mut self.overlays, cmd_start, i);
            return i;
        }
        j += 1;
        Overlay::hide_range(&mut self.overlays, cmd_start, j);

        // Scan numerator until matching `}`.
        let num_close = Self::find_matching_brace(self.bytes, j);
        self.overlays.push(Overlay::at(num_close - 1, "⁄"));

        // Skip whitespace, then scan denominator.
        let mut k = num_close;
        while k < self.bytes.len() && self.bytes[k] == b' ' {
            k += 1;
        }
        if k < self.bytes.len() && self.bytes[k] == b'{' {
            self.overlays.push(Overlay::hide(k));
            k += 1;
            let den_close = Self::find_matching_brace(self.bytes, k);
            self.overlays.push(Overlay::hide(den_close - 1));
        }
        num_close
    }

    /// Given a position `j` that points JUST past an opening `{`, return the
    /// byte position one past the matching closing `}`. If no matching brace
    /// is found, returns the end of the input.
    fn find_matching_brace(bytes: &[u8], mut j: usize) -> usize {
        let mut depth = 1i32;
        while j < bytes.len() && depth > 0 {
            match bytes[j] {
                b'{' => depth += 1,
                b'}' => depth -= 1,
                _ => {}
            }
            j += 1;
        }
        j
    }

    /// Simple `\name` lookup — falls back to the Julia symbol table.
    fn scan_simple_command(&mut self, cmd_start: usize, i: usize, name: &str) -> usize {
        let lookup = unicode_lookup(name.to_string());
        if !lookup.is_empty() {
            self.overlays
                .push(Overlay::at(cmd_start, Cow::Owned(lookup)));
            Overlay::hide_range(&mut self.overlays, cmd_start + 1, i);
        }
        i
    }

    /// `\|` `\{` `\}` `\,` `\;` `\!` `\ ` `\(` `\)` `\[` `\]`
    fn scan_non_alpha_backslash(&mut self, i: usize) -> usize {
        let ch = self.bytes[i + 1];
        let replacement: Option<&'static str> = match ch {
            b'|' => Some("‖"),
            b'{' => Some("{"),
            b'}' => Some("}"),
            b',' => Some("\u{2006}"), // thin space
            b';' => Some("\u{2005}"), // medium space
            b'!' => Some("\u{200B}"), // zero-width
            b' ' => Some(" "),
            _ => None,
        };
        if let Some(rep) = replacement {
            self.overlays.push(Overlay::at(i, rep));
            self.overlays.push(Overlay::hide(i + 1));
            i + 2
        } else if matches!(ch, b'(' | b')' | b'[' | b']') {
            // Math region delimiters: hide the backslash, leave the paren
            // for the outer scanner to handle.
            self.overlays.push(Overlay::hide(i));
            i + 1
        } else {
            i + 1
        }
    }

    /// `\\` inside a matrix-style environment → row break + fence character.
    fn scan_row_separator(&mut self, i: usize) -> usize {
        if self.env_stack.is_empty() {
            return i + 1;
        }
        let env = self.env_stack.last_mut().unwrap();
        env.row += 1;
        let row = env.row;
        let total_rows = env.total_rows;
        let env_name = env.env_name.clone();
        let row_fence = mid_fence(&env_name, row, total_rows);
        Overlay::hide_range(&mut self.overlays, i, i + 2);
        if !row_fence.is_empty() {
            self.overlays.push(Overlay::at(i, row_fence));
        }
        i + 2
    }

    /// `^{...}` — emit one superscript glyph per content character.
    /// Abort (leave raw) if any character has no superscript variant.
    fn scan_braced_superscript(&mut self, i: usize) -> usize {
        let caret_pos = i;
        let mut j = i + 2;
        let content_start = j;
        while j < self.bytes.len() && self.bytes[j] != b'}' {
            j += 1;
        }
        if j >= self.bytes.len() {
            return j;
        }
        let content = &self.text[content_start..j];
        let past_close = j + 1;

        let supers: Option<Vec<&'static str>> =
            content.chars().map(|c| map_lookup(SUPER_MAP, c)).collect();

        if let Some(supers) = supers {
            self.overlays.push(Overlay::hide(caret_pos));
            self.overlays.push(Overlay::hide(caret_pos + 1));
            let mut char_offset = content_start;
            for (ci, ch) in content.chars().enumerate() {
                self.overlays.push(Overlay::at(char_offset, supers[ci]));
                char_offset += ch.len_utf8();
            }
            self.overlays.push(Overlay::hide(past_close - 1));
        }
        past_close
    }

    /// `^x` single-character superscript.
    fn scan_inline_superscript(&mut self, i: usize) -> usize {
        let ch = self.bytes[i + 1] as char;
        if let Some(rep) = map_lookup(SUPER_MAP, ch) {
            self.overlays.push(Overlay::at(i, rep));
            self.overlays.push(Overlay::hide(i + 1));
            i + 2
        } else {
            i + 1
        }
    }

    /// `_{...}` — emit one subscript glyph per content character.
    fn scan_braced_subscript(&mut self, i: usize) -> usize {
        let underscore_pos = i;
        let mut j = i + 2;
        let content_start = j;
        while j < self.bytes.len() && self.bytes[j] != b'}' {
            j += 1;
        }
        if j >= self.bytes.len() {
            return j;
        }
        let content = &self.text[content_start..j];
        let past_close = j + 1;

        let subs: Option<Vec<&'static str>> =
            content.chars().map(|c| map_lookup(SUB_MAP, c)).collect();

        if let Some(subs) = subs {
            self.overlays.push(Overlay::hide(underscore_pos));
            self.overlays.push(Overlay::hide(underscore_pos + 1));
            let mut char_offset = content_start;
            for (ci, ch) in content.chars().enumerate() {
                self.overlays.push(Overlay::at(char_offset, subs[ci]));
                char_offset += ch.len_utf8();
            }
            self.overlays.push(Overlay::hide(past_close - 1));
        }
        past_close
    }

    /// `_x` single-character subscript. Falls through to Julia symbol table
    /// if the character has no dedicated subscript codepoint (handles Greek).
    fn scan_inline_subscript(&mut self, i: usize) -> usize {
        let ch = self.bytes[i + 1] as char;
        if let Some(rep) = map_lookup(SUB_MAP, ch) {
            self.overlays.push(Overlay::at(i, rep));
            self.overlays.push(Overlay::hide(i + 1));
            i + 2
        } else {
            let lookup = unicode_lookup(ch.to_string());
            if !lookup.is_empty() {
                self.overlays.push(Overlay::at(i, Cow::Owned(lookup)));
                self.overlays.push(Overlay::hide(i + 1));
                i + 2
            } else {
                i + 1
            }
        }
    }
}

/// Public FFI entry point. Scans one math region's worth of text and returns
/// a JSON array of `{"offset": N, "replacement": "X"}` entries.
pub fn latex_overlays(text: String) -> String {
    let overlays = Scanner::new(&text).scan();
    serde_json::to_string(&overlays).unwrap_or_else(|_| "[]".to_string())
}

/// Scan a math region and return overlays as `(byte_offset, replacement)`
/// tuples. Used by the conceal code path to avoid a JSON round-trip.
pub(super) fn scan_to_vec(text: &str) -> Vec<(usize, String)> {
    Scanner::new(text)
        .scan()
        .into_iter()
        .map(|o| (o.offset, o.replacement.into_owned()))
        .collect()
}
