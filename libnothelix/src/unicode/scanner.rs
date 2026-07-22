use std::borrow::Cow;
use std::ops::Range;

use super::cursor::{
    alphabetic_end, matching_brace, past_matching_brace, past_spaces, past_spaces_and_tabs,
    past_whitespace,
};
use super::environment::Environment;
use super::escape::{combining_mark, spacing_glyph};
use super::font::latex_font_to_julia;
use super::operators::{is_big_operator, is_math_operator};
use super::overlay::Overlay;
use super::script::Script;
use super::symbol_table::symbol;
use crate::error::{Error, Result, ffi};

#[derive(Debug, Clone, Copy, Default)]
pub struct ScannerOptions {
    pub hide_math_layout: bool,
}

fn is_simple_brace_content(s: &str) -> bool {
    s.chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '=' | '*' | '(' | ')'))
}

struct Scanner<'a> {
    text: &'a str,
    bytes: &'a [u8],
    overlays: Vec<Overlay>,
    pending_limits: u8,
    env_stack: Vec<Environment>,
    options: ScannerOptions,
}

impl<'a> Scanner<'a> {
    fn new(text: &'a str, options: ScannerOptions) -> Self {
        Self {
            text,
            bytes: text.as_bytes(),
            overlays: Vec::new(),
            env_stack: Vec::new(),
            pending_limits: 0,
            options,
        }
    }

    fn scan(mut self) -> Vec<Overlay> {
        let mut i = 0;
        while i < self.bytes.len() {
            i = self.step(i);
        }
        self.overlays
    }

    fn put(&mut self, offset: usize, replacement: impl Into<Cow<'static, str>>) {
        self.overlays.push(Overlay::at(offset, replacement));
    }

    fn hide(&mut self, range: Range<usize>) {
        for byte in range {
            self.overlays.push(Overlay::hide(byte));
        }
    }

    fn step(&mut self, i: usize) -> usize {
        let b = self.bytes;
        let len = b.len();
        let next = b.get(i + 1).copied();

        if self.pending_limits > 0 && !matches!(b[i], b' ' | b'\t' | b'\n' | b'\r' | b'_' | b'^') {
            self.pending_limits = 0;
        }

        if b[i] == b'\\' {
            return match next {
                Some(c) if c.is_ascii_alphabetic() => self.scan_backslash_command(i),
                Some(b'\\') => self.scan_row_separator(i),
                Some(_) => self.scan_non_alpha_backslash(i),
                None => i + 1,
            };
        }

        if b[i] == b'&' && !self.env_stack.is_empty() {
            self.put(i, " ");
            return i + 1;
        }

        if let Some(script) = Script::marked_by(b[i])
            && i + 1 < len
        {
            return match b[i + 1] {
                b'{' => self.scan_braced_script(i, script),
                b'\\' => self.scan_command_script(i, script),
                c if c != b[i] => self.scan_inline_script(i, script),
                _ => i + 1,
            };
        }

        i + 1
    }

    fn command_name_end(&self, from: usize) -> usize {
        alphabetic_end(self.bytes, from)
    }

    fn skip_spaces(&self, i: usize) -> usize {
        past_spaces(self.bytes, i)
    }

    fn consume_pending_limit(&mut self) -> bool {
        if self.pending_limits == 0 {
            return false;
        }
        self.pending_limits -= 1;
        self.options.hide_math_layout
    }

    fn arm_limits_after(&mut self, name: &str) {
        if is_big_operator(name) {
            self.pending_limits = 2;
        }
    }

    fn place_fence(&mut self, fence: &'static str, content_from: usize, fallback: usize) {
        let at = past_whitespace(self.bytes, content_from);
        match self.text[at..].chars().next() {
            Some(ch) if ch.is_ascii_alphanumeric() => self.put(at, format!("{fence}{ch}")),
            _ => self.put(fallback, fence),
        }
    }

    fn scan_command_script(&mut self, marker: usize, script: Script) -> usize {
        let name_start = marker + 2;
        let name_end = self.command_name_end(name_start);
        if self.consume_pending_limit() {
            self.hide(marker..name_end);
            return name_end;
        }
        match script.of_command(&self.text[name_start..name_end]) {
            Some(glyph) => {
                self.put(marker, glyph);
                self.hide(marker + 1..name_end);
                name_end
            }
            None => marker + 1,
        }
    }

    fn scan_inline_script(&mut self, marker: usize, script: Script) -> usize {
        if self.consume_pending_limit() {
            self.hide(marker..marker + 2);
            return marker + 2;
        }
        match script.of_char(self.bytes[marker + 1] as char) {
            Some(glyph) => {
                self.put(marker, glyph);
                self.hide(marker + 1..marker + 2);
                marker + 2
            }
            None => marker + 1,
        }
    }

    fn scan_braced_script(&mut self, marker: usize, script: Script) -> usize {
        let content_start = marker + 2;
        let close = matching_brace(self.bytes, content_start);
        if close >= self.bytes.len() {
            return close;
        }
        let content = &self.text[content_start..close];
        let past_close = close + 1;

        if self.consume_pending_limit() {
            self.hide(marker..past_close);
            return past_close;
        }

        let glyphs: Vec<Option<&'static str>> =
            content.chars().map(|ch| script.of_char(ch)).collect();
        if glyphs.iter().any(Option::is_some) && is_simple_brace_content(content) {
            self.hide(marker..content_start);
            self.replace_each_char(content, content_start, &glyphs);
            self.hide(past_close - 1..past_close);
            return past_close;
        }

        if let Some(glyph) = script.of_braced_command(content) {
            self.put(marker, glyph);
            self.hide(marker + 1..past_close);
            return past_close;
        }

        self.hide(marker + 1..marker + 2);
        self.hide(past_close - 1..past_close);
        content_start
    }

    fn replace_each_char(
        &mut self,
        content: &str,
        content_start: usize,
        glyphs: &[Option<&'static str>],
    ) {
        let mut offset = content_start;
        for (ch, glyph) in content.chars().zip(glyphs) {
            if let Some(glyph) = *glyph {
                self.put(offset, glyph);
            }
            offset += ch.len_utf8();
        }
    }

    fn scan_backslash_command(&mut self, cmd_start: usize) -> usize {
        let name_end = self.command_name_end(cmd_start + 1);
        let name = &self.text[cmd_start + 1..name_end];

        match name {
            "begin" => self.scan_begin_env(cmd_start, name_end),
            "end" => self.scan_end_env(cmd_start, name_end),
            "text" | "mathrm" | "operatorname" => self.scan_text_command(cmd_start, name_end),
            "mathbf" | "textbf" | "boldsymbol" | "mathbb" | "mathcal" | "cal" | "mathfrak"
            | "frak" | "mathit" | "textit" | "mathsf" | "mathtt" => {
                self.scan_font_command(cmd_start, name_end, name)
            }
            "frac" | "dfrac" | "tfrac" => self.scan_frac_command(cmd_start, name_end),
            "newcommand" | "renewcommand" | "providecommand" | "DeclareMathOperator" => {
                self.scan_macro_definition(cmd_start, name_end)
            }
            "left" | "right" | "bigl" | "bigr" | "Bigl" | "Bigr" | "biggl" | "biggr" | "Biggl"
            | "Biggr" | "big" | "Big" | "bigg" | "Bigg" => {
                self.hide(cmd_start..name_end);
                name_end
            }
            _ => match combining_mark(name) {
                Some(mark) => self.scan_combining_mark_command(cmd_start, name_end, name, mark),
                None => self.scan_simple_command(cmd_start, name_end, name),
            },
        }
    }

    fn scan_env_tag(&self, after_name: usize) -> Option<(&'a str, usize)> {
        let open = self.skip_spaces(after_name);
        if self.bytes.get(open) != Some(&b'{') {
            return None;
        }
        let name_start = open + 1;
        let mut j = name_start;
        while j < self.bytes.len() && self.bytes[j] != b'}' {
            j += 1;
        }
        let past_close = if j < self.bytes.len() { j + 1 } else { j };
        Some((&self.text[name_start..j], past_close))
    }

    fn scan_begin_env(&mut self, cmd_start: usize, after_name: usize) -> usize {
        let Some((env_name, content_start)) = self.scan_env_tag(after_name) else {
            return self.skip_spaces(after_name);
        };

        let end_tag = format!("\\end{{{env_name}}}");
        let env_end = self.text[content_start..]
            .find(&end_tag)
            .map_or(self.text.len(), |pos| content_start + pos);
        let environment = Environment::opening(env_name, &self.text[content_start..env_end]);
        let open_fence = environment.open_fence();

        self.hide(cmd_start..content_start);
        self.env_stack.push(environment);
        if let Some(fence) = open_fence {
            self.place_fence(fence, content_start, cmd_start);
        }
        content_start
    }

    fn scan_end_env(&mut self, cmd_start: usize, after_name: usize) -> usize {
        let Some((_, past_close)) = self.scan_env_tag(after_name) else {
            return self.skip_spaces(after_name);
        };
        let close_fence = self.env_stack.pop().and_then(Environment::close_fence);
        self.hide(cmd_start..past_close);
        if let Some(fence) = close_fence {
            self.put(past_close - 1, fence);
        }
        past_close
    }

    fn scan_row_separator(&mut self, i: usize) -> usize {
        let Some(environment) = self.env_stack.last_mut() else {
            return i + 1;
        };
        let row_fence = environment.advance_row();
        self.hide(i..i + 2);
        if let Some(fence) = row_fence {
            self.place_fence(fence, i + 2, i);
        }
        i + 2
    }

    fn scan_text_command(&mut self, cmd_start: usize, after_name: usize) -> usize {
        let open = self.skip_spaces(after_name);
        if self.bytes.get(open) != Some(&b'{') {
            return open;
        }
        self.hide(cmd_start..open + 1);
        let mut j = open + 1;
        while j < self.bytes.len() && self.bytes[j] != b'}' {
            j += 1;
        }
        if j < self.bytes.len() {
            self.hide(j..j + 1);
            j += 1;
        }
        j
    }

    fn scan_font_command(&mut self, cmd_start: usize, after_name: usize, name: &str) -> usize {
        let open = self.skip_spaces(after_name);
        if self.bytes.get(open) != Some(&b'{') {
            return after_name;
        }
        let content_start = open + 1;
        let mut close = content_start;
        while close < self.bytes.len() && self.bytes[close] != b'}' {
            close += 1;
        }
        if close >= self.bytes.len() {
            return after_name;
        }
        let content = &self.text[content_start..close];
        let past_close = close + 1;

        if content.len() == 1
            && let Some(glyph) = latex_font_to_julia(name, content)
        {
            self.put(cmd_start, glyph);
            self.hide(cmd_start + 1..past_close);
            return past_close;
        }

        let glyphs: Vec<Option<&'static str>> = content
            .chars()
            .map(|ch| latex_font_to_julia(name, &ch.to_string()))
            .collect();
        if glyphs.iter().any(Option::is_some) {
            self.hide(cmd_start..content_start);
            self.replace_each_char(content, content_start, &glyphs);
            self.hide(close..past_close);
        }
        past_close
    }

    fn scan_frac_command(&mut self, cmd_start: usize, after_name: usize) -> usize {
        let num_open = self.skip_spaces(after_name);
        if self.bytes.get(num_open) != Some(&b'{') {
            self.hide(cmd_start..after_name);
            return after_name;
        }
        let num_start = num_open + 1;
        self.hide(cmd_start..num_start);

        let past_num = past_matching_brace(self.bytes, num_start);
        let den_open = self.skip_spaces(past_num);
        let has_denominator = self.bytes.get(den_open) == Some(&b'{');
        let past_den = if has_denominator {
            past_matching_brace(self.bytes, den_open + 1)
        } else {
            past_num
        };

        if self.options.hide_math_layout {
            self.hide(cmd_start..past_den);
            return past_den;
        }

        if past_num > num_start {
            self.put(past_num - 1, "⁄");
        }
        if has_denominator {
            self.hide(den_open..den_open + 1);
            if past_den > den_open + 1 {
                self.hide(past_den - 1..past_den);
            }
        }
        num_start
    }

    fn scan_macro_definition(&mut self, cmd_start: usize, after_name: usize) -> usize {
        let mut j = after_name;
        for _ in 0..2 {
            j = past_spaces_and_tabs(self.bytes, j);
            while self.bytes.get(j) == Some(&b'[') {
                while j < self.bytes.len() && self.bytes[j] != b']' {
                    j += 1;
                }
                if j < self.bytes.len() {
                    j += 1;
                }
                j = past_spaces_and_tabs(self.bytes, j);
            }
            if self.bytes.get(j) != Some(&b'{') {
                break;
            }
            j = past_matching_brace(self.bytes, j + 1);
        }
        self.hide(cmd_start..j);
        j
    }

    fn scan_combining_mark_command(
        &mut self,
        cmd_start: usize,
        after_name: usize,
        name: &str,
        mark: &'static str,
    ) -> usize {
        let open = self.skip_spaces(after_name);
        if self.bytes.get(open) != Some(&b'{') {
            return self.scan_simple_command(cmd_start, after_name, name);
        }
        let content_start = open + 1;
        let close = past_matching_brace(self.bytes, content_start) - 1;
        if close <= content_start {
            self.hide(cmd_start..close + 1);
            return close + 1;
        }
        self.hide(cmd_start..content_start);
        self.put(close, mark);
        content_start
    }

    fn scan_simple_command(&mut self, cmd_start: usize, after_name: usize, name: &str) -> usize {
        if is_math_operator(name) {
            self.hide(cmd_start..cmd_start + 1);
            self.arm_limits_after(name);
            return after_name;
        }
        if let Some(glyph) = symbol(name) {
            self.put(cmd_start, glyph);
            self.hide(cmd_start + 1..after_name);
            self.arm_limits_after(name);
        }
        after_name
    }

    fn scan_non_alpha_backslash(&mut self, i: usize) -> usize {
        let escaped = self.bytes[i + 1];
        if let Some(glyph) = spacing_glyph(escaped) {
            self.put(i, glyph);
            self.hide(i + 1..i + 2);
            return i + 2;
        }
        if matches!(escaped, b'(' | b')' | b'[' | b']') {
            self.hide(i..i + 1);
        }
        i + 1
    }
}

#[allow(clippy::needless_pass_by_value)]
pub fn latex_overlays(text: String) -> String {
    latex_overlays_with_options(text, false)
}

#[allow(clippy::needless_pass_by_value)]
pub fn latex_overlays_with_options(text: String, hide_math_layout: bool) -> String {
    ffi(overlays_json(&text, ScannerOptions { hide_math_layout }))
}

fn overlays_json(text: &str, options: ScannerOptions) -> Result<String> {
    serde_json::to_string(&Scanner::new(text, options).scan()).map_err(|source| Error::Json {
        subject: "latex conceal overlays",
        source,
    })
}

pub(super) fn scan_region(text: &str, options: ScannerOptions) -> Vec<(usize, String)> {
    Scanner::new(text, options)
        .scan()
        .into_iter()
        .map(|o| (o.offset, o.replacement.into_owned()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn replacements(source: &str) -> Vec<String> {
        let json = latex_overlays(source.to_string());
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        parsed
            .as_array()
            .unwrap()
            .iter()
            .map(|o| o["replacement"].as_str().unwrap().to_string())
            .collect()
    }

    fn hidden_offsets(source: &str) -> Vec<usize> {
        let json = latex_overlays(source.to_string());
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        parsed
            .as_array()
            .unwrap()
            .iter()
            .filter(|o| o["replacement"].as_str().unwrap().is_empty())
            .map(|o| o["offset"].as_u64().unwrap() as usize)
            .collect()
    }

    fn contains(source: &str, glyph: &str) -> bool {
        replacements(source).iter().any(|r| r == glyph)
    }

    fn starts_with(source: &str, glyph: char) -> bool {
        replacements(source).iter().any(|r| r.starts_with(glyph))
    }

    #[test]
    fn simple_command_replaces_then_hides_its_name() {
        let json = latex_overlays(r"\alpha + \beta".into());
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        let arr = parsed.as_array().unwrap();
        assert!(!arr.is_empty());
        assert_eq!(arr[0]["offset"], 0);
        assert_eq!(arr[0]["replacement"], "α");
        assert_eq!(arr[1]["replacement"], "");
    }

    #[test]
    fn braced_superscript_maps_every_char() {
        let reps = replacements("10^{-6}");
        assert!(reps.iter().any(|r| r == "⁻"));
        assert!(reps.iter().any(|r| r == "⁶"));
    }

    #[test]
    fn font_command_maps_single_letter() {
        let json = latex_overlays(r"\mathbf{b}".into());
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.as_array().unwrap()[0]["replacement"], "𝐛");
    }

    #[test]
    fn plain_text_yields_no_overlays() {
        assert_eq!(latex_overlays("x + y = z".into()), "[]");
    }

    #[test]
    fn text_command_hides_wrapper_and_keeps_body() {
        let source = r"\text{otherwise}";
        let hidden = hidden_offsets(source);
        assert!(hidden.contains(&0));
        assert!(hidden.contains(&(source.len() - 1)));
    }

    #[test]
    fn cases_env_opens_and_closes_with_fences() {
        let source = r"\begin{cases} 1 & 0 \leq n \leq 2 \\ 0 & \text{otherwise} \end{cases}";
        assert!(starts_with(source, '⎧'), "{:?}", replacements(source));
        assert!(starts_with(source, '⎩'), "{:?}", replacements(source));
        assert!(contains(source, "≤"));
        assert!(contains(source, " "));
    }

    #[test]
    fn three_row_cases_env_uses_the_mid_fence() {
        let source = r"\begin{cases} 1 & a \\ 2 & b \\ 3 & c \end{cases}";
        assert!(starts_with(source, '⎨'), "{:?}", replacements(source));
        assert!(starts_with(source, '⎩'), "{:?}", replacements(source));
    }

    #[test]
    fn pmatrix_env_uses_paren_fences() {
        let source = r"\begin{pmatrix} 1 & 0 \\ 0 & 1 \end{pmatrix}";
        assert!(starts_with(source, '⎛'), "{:?}", replacements(source));
        assert!(starts_with(source, '⎞'), "{:?}", replacements(source));
    }

    #[test]
    fn braced_subscript_maps_its_char() {
        assert!(contains("h_{n}", "ₙ"));
    }

    #[test]
    fn sum_paired_limits_render_as_scripts() {
        let source = r"\sum_{k=-n}^n c_k";
        assert!(contains(source, "∑"));
        assert!(contains(source, "ⁿ"));
        assert!(contains(source, "₋"));
    }

    #[test]
    fn sum_with_bare_single_char_limit_renders_subscript() {
        let source = r"\sum_m \varphi_m";
        assert!(contains(source, "∑"));
        assert!(contains(source, "ₘ"));
    }

    #[test]
    fn integral_with_ascii_bounds_render_as_scripts() {
        let source = r"\int_a^b f(x)dx";
        assert!(contains(source, "∫"));
        assert!(contains(source, "ₐ"));
        assert!(contains(source, "ᵇ"));
    }

    #[test]
    fn prod_with_braced_subscript_and_bare_super_render_as_scripts() {
        let source = r"\prod_{i=1}^n a_i";
        assert!(contains(source, "∏"));
        assert!(contains(source, "ⁿ"));
        assert!(contains(source, "₁"));
    }

    #[test]
    fn inline_super_on_non_big_operator_still_works() {
        let source = r"\alpha^2 + \beta^3";
        assert!(contains(source, "α"));
        assert!(contains(source, "²"));
        assert!(contains(source, "³"));
    }

    #[test]
    fn complex_braced_superscript_drops_braces() {
        let source = r"e^{2\pi i kt}";
        let hidden = hidden_offsets(source);
        assert!(hidden.contains(&source.find('{').unwrap()));
        assert!(hidden.contains(&source.find('}').unwrap()));
        assert!(contains(source, "π"));
    }

    #[test]
    fn complex_braced_subscript_drops_braces() {
        let source = r"x_{L^2(\mathbb{R})}";
        let hidden = hidden_offsets(source);
        assert!(hidden.contains(&source.find('{').unwrap()));
        assert!(hidden.contains(&source.rfind('}').unwrap()));
        assert!(contains(source, "ℝ"));
    }

    #[test]
    fn norm_delimiter_becomes_double_bar() {
        assert!(contains(r"\|B - B_1\|", "‖"));
    }

    #[test]
    fn piecewise_definition_conceals_end_to_end() {
        let source = r"h_n = \begin{cases} 1 & 0 \leq n \leq 2 \\ 0 & \text{otherwise} \end{cases}";
        assert!(contains(source, "ₙ"));
        assert!(starts_with(source, '⎧'));
        assert!(starts_with(source, '⎩'));
        assert!(contains(source, "≤"));
    }

    #[test]
    fn frac_emits_a_slash_and_hides_the_command() {
        let source = r"\frac{a}{b}";
        assert!(contains(source, "⁄"));
        assert!(hidden_offsets(source).contains(&0));
    }

    #[test]
    fn nested_frac_emits_a_slash_per_level() {
        let slashes = replacements(r"\frac{1}{\frac{a}{b}}")
            .iter()
            .filter(|r| *r == "⁄")
            .count();
        assert!(slashes >= 2, "got {slashes}");
    }

    #[test]
    fn hide_math_layout_suppresses_big_op_limits() {
        let visible: Vec<String> = {
            let json = latex_overlays_with_options(r"\sum_{k=0}^n a_k".into(), true);
            let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
            parsed
                .as_array()
                .unwrap()
                .iter()
                .map(|o| o["replacement"].as_str().unwrap().to_string())
                .collect()
        };
        assert!(visible.iter().any(|r| r == "∑"));
        assert!(
            !visible.iter().any(|r| r == "ⁿ"),
            "stacked renderer owns the limits: {visible:?}"
        );
    }
}
