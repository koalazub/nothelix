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

/// Per-scan options. Passed by the caller instead of pulled from a
/// process-global atomic — the previous design meant any two documents
/// open in parallel would fight over the toggle, and the flag was
/// spooky-action-at-a-distance from the Scheme plugin's perspective.
#[derive(Debug, Clone, Copy, Default)]
pub struct ScannerOptions {
    /// When `true`, big-operator limits (`_{a}^{b}` after `\sum`, `\int`,
    /// …) and `\frac{num}{den}` wrappers are fully hidden so the
    /// math-render plugin's stacked virtual rows don't collide with a
    /// redundant inline rendering. Default: keep inline limits visible.
    pub hide_math_layout: bool,
}

/// Track which environment we're inside and which row we're on within it.
/// This lets the scanner emit the right Unicode fence character on each row
/// boundary within a matrix-style environment.
struct EnvState {
    env_name: String,
    row: usize,
    total_rows: usize,
}

/// Big operators (`\sum`, `\int`, `\prod`, …) carry *limits* in their
/// `_{...}` and `^{...}`, not scripts. We keep those limits at normal size
/// rather than shrinking them into tiny Unicode super/subscript glyphs.
fn is_big_operator(name: &str) -> bool {
    matches!(
        name,
        "sum" | "prod" | "coprod"
        | "int" | "iint" | "iiint" | "iiiint" | "oint" | "oiint" | "oiiint"
        | "bigcup" | "bigcap" | "bigvee" | "bigwedge"
        | "bigoplus" | "bigotimes" | "bigodot"
        | "biguplus" | "bigsqcup"
        | "lim" | "liminf" | "limsup"
        | "min" | "max" | "sup" | "inf"
        | "argmin" | "argmax"
    )
}

/// LaTeX math operators render as upright multi-letter text rather than a
/// single Unicode glyph (`\cos` → `cos`, not a single codepoint). The
/// scanner hides the leading `\` and leaves the name visible, matching what
/// `\operatorname{cos}` would produce.
fn is_math_operator(name: &str) -> bool {
    matches!(
        name,
        "cos" | "sin" | "tan" | "cot" | "sec" | "csc"
        | "arccos" | "arcsin" | "arctan" | "arccot" | "arcsec" | "arccsc"
        | "cosh" | "sinh" | "tanh" | "coth" | "sech" | "csch"
        | "exp" | "log" | "ln" | "lg"
        | "max" | "min" | "sup" | "inf" | "lim" | "liminf" | "limsup"
        | "det" | "dim" | "arg" | "ker" | "gcd" | "lcm"
        | "Pr" | "deg" | "hom" | "mod" | "bmod" | "pmod"
        | "sinc" | "erf" | "tr" | "rank"
    )
}

/// A braced sub/superscript is "simple" enough for partial emission when
/// every content char is plain ASCII alphanumeric or a basic math operator.
/// This rules out contents that contain backslashes (e.g. `^{2\pi i kt}`)
/// or nested braces, where leaving individual chars raw produces a mangled
/// display like `²\piⁱkt` instead of the intended superscripted expression.
fn is_simple_brace_content(s: &str) -> bool {
    s.chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '=' | '*' | '(' | ')'))
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
/// When the previous scanner step just emitted a big-operator glyph
/// (`\sum`, `\int`, `\prod`, …), the next `_{...}` / `^{...}` pair carries
/// the operator's *limits*, not subscripts/superscripts on an expression.
/// Converting limits into tiny Unicode super/subscript glyphs produces the
/// "need a microscope" rendering; we'd rather keep the limits at normal
/// size and let the inner commands (`\in`, `\mathbb`, Greek, …) render
/// naturally. This counter tracks how many limit groups are still pending;
/// it's decremented as each `_`/`^` is consumed in limit mode and zeroed
/// the moment any non-limit byte arrives.
struct Scanner<'a> {
    text: &'a str,
    bytes: &'a [u8],
    overlays: Vec<Overlay>,
    pending_limits: u8,
    env_stack: Vec<EnvState>,
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

        // Any non-whitespace, non-`_`, non-`^` byte encountered while
        // we're still "expecting" big-op limits means the limits didn't
        // materialise (e.g. `\sum f(x)` — no limits at all). Clear the
        // flag so later `_`/`^` in the summand aren't mis-treated as
        // limits.
        if self.pending_limits > 0
            && !matches!(b[i], b' ' | b'\t' | b'\n' | b'\r' | b'_' | b'^')
        {
            self.pending_limits = 0;
        }

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
        // ^x — any single byte whose superscript form exists in SUPER_MAP.
        // We used to gate this on `!is_ascii_alphabetic()` back when only
        // `n`/`i` had letter supers; that guard now blocks valid letter
        // supers like `^N` (ℝᴺ), `^T` (transpose), `^k`, etc. Let
        // `map_lookup` in `scan_inline_superscript` decide — unmapped bytes
        // fall through untouched.
        if b[i] == b'^'
            && i + 1 < len
            && b[i + 1] != b'{'
            && b[i + 1] != b'\\'
            && b[i + 1] != b'^'
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
            "newcommand" | "renewcommand" | "providecommand" | "DeclareMathOperator" => {
                self.scan_macro_definition(cmd_start, name_end)
            }
            // `\left(` / `\right)` are sizing directives for paired
            // delimiters — in a text editor there's nothing to scale, so we
            // hide just the command and let the delimiter that follows
            // render unchanged.
            "left" | "right" | "bigl" | "bigr" | "Bigl" | "Bigr"
            | "biggl" | "biggr" | "Biggl" | "Biggr"
            | "big" | "Big" | "bigg" | "Bigg" => {
                Overlay::hide_range(&mut self.overlays, cmd_start, name_end);
                name_end
            }
            // `\tilde{x}` / `\bar{x}` / `\hat{x}` / `\vec{x}` / `\dot{x}` /
            // `\ddot{x}` / `\widetilde{x}` / `\widehat{x}` apply a combining
            // diacritic. Handle them via a dedicated scanner that hides the
            // wrapper and attaches the combining mark to the content's
            // last grapheme.
            "tilde" | "widetilde" | "bar" | "overline" | "hat" | "widehat"
            | "vec" | "dot" | "ddot" | "mathring" => {
                self.scan_combining_mark_command(cmd_start, name_end, name)
            }
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
            // Prefer prepending the fence to the first content grapheme so
            // `\begin{cases}` on its own line doesn't waste a whole editor
            // row just to show `⎧`. Skip leading whitespace/newline after
            // `\begin{env}` and use a 2-grapheme replacement (`⎧t`) on the
            // first alnum char. Falls back to emitting at `cmd_start` when
            // the next content char isn't safe to overlay (e.g. `\leq`,
            // where `scan_simple_command` would also write at that offset).
            let mut content_pos = i;
            while content_pos < self.bytes.len()
                && matches!(self.bytes[content_pos], b' ' | b'\t' | b'\n' | b'\r')
            {
                content_pos += 1;
            }
            let placed = if content_pos < self.bytes.len() {
                let ch = self.text[content_pos..].chars().next().unwrap();
                if ch.is_ascii_alphanumeric() {
                    let replacement = format!("{fence}{ch}");
                    self.overlays.push(Overlay::at(content_pos, replacement));
                    true
                } else {
                    false
                }
            } else {
                false
            };
            if !placed {
                self.overlays.push(Overlay::at(cmd_start, fence));
            }
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
        // If any `\\` row separator ran, the last one already placed the
        // closing fence on the last row (see `scan_row_separator`). Only
        // emit `⎩` here for single-row environments that had no separators.
        let (fence, had_separators) = self
            .env_stack
            .pop()
            .map(|env| (close_fence(&env.env_name, env.total_rows), env.row > 0))
            .unwrap_or_default();
        Overlay::hide_range(&mut self.overlays, cmd_start, i);
        if !fence.is_empty() && !had_separators {
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
    /// between the numerator and denominator, hide the command wrapper, and
    /// return the position of the numerator's first byte so the main scan
    /// loop walks the numerator + denominator content recursively. Without
    /// that recursion, `\frac{C_k+C^*_{N-k}}{2}` would leave `_k`, `^*`, and
    /// `_{N-k}` inside the numerator raw.
    fn scan_frac_command(&mut self, cmd_start: usize, i: usize) -> usize {
        let mut j = i;
        while j < self.bytes.len() && self.bytes[j] == b' ' {
            j += 1;
        }
        if j >= self.bytes.len() || self.bytes[j] != b'{' {
            Overlay::hide_range(&mut self.overlays, cmd_start, i);
            return i;
        }
        j += 1;
        Overlay::hide_range(&mut self.overlays, cmd_start, j);
        let num_start = j;

        let num_close = Self::find_matching_brace(self.bytes, j);

        // Locate denominator `{…}` so we know the full span of the frac.
        let mut den_open = num_close;
        while den_open < self.bytes.len() && self.bytes[den_open] == b' ' {
            den_open += 1;
        }
        let den_close = if den_open < self.bytes.len() && self.bytes[den_open] == b'{' {
            let after_brace = den_open + 1;
            Self::find_matching_brace(self.bytes, after_brace)
        } else {
            num_close
        };

        if self.options.hide_math_layout {
            // The math-render plugin is painting numerator above /
            // denominator below — hide the entire `\frac{..}{..}` so
            // both representations don't fight over the same row.
            Overlay::hide_range(&mut self.overlays, cmd_start, den_close);
            return den_close;
        }

        if num_close > j {
            self.overlays.push(Overlay::at(num_close - 1, "⁄"));
        }
        if den_open < self.bytes.len() && self.bytes[den_open] == b'{' {
            self.overlays.push(Overlay::hide(den_open));
            if den_close > den_open + 1 {
                self.overlays.push(Overlay::hide(den_close - 1));
            }
        }
        num_start
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

    /// `\newcommand{name}{body}`, `\renewcommand{name}{body}`,
    /// `\providecommand{name}{body}`, `\DeclareMathOperator{name}{body}`.
    /// These are LaTeX preamble directives that define macros — they produce
    /// no output of their own. Hide the whole definition (command name,
    /// optional args, both brace groups) so the notebook doesn't show a raw
    /// `\renewcommand{\R}{ℝ}` line where only the `ℝ` looks rendered.
    fn scan_macro_definition(&mut self, cmd_start: usize, name_end: usize) -> usize {
        let mut j = name_end;

        // Skip optional bracketed args: `\newcommand{\R}[0]{\mathbb R}`.
        let skip_ws = |bytes: &[u8], mut k: usize| {
            while k < bytes.len() && matches!(bytes[k], b' ' | b'\t') {
                k += 1;
            }
            k
        };

        // Consume two brace groups (some variants take more optional args,
        // but the common case is `{name}{body}`).
        let mut consumed_groups = 0;
        while consumed_groups < 2 {
            j = skip_ws(self.bytes, j);
            // Skip any [optional] argument blocks.
            while j < self.bytes.len() && self.bytes[j] == b'[' {
                while j < self.bytes.len() && self.bytes[j] != b']' {
                    j += 1;
                }
                if j < self.bytes.len() {
                    j += 1; // skip ]
                }
                j = skip_ws(self.bytes, j);
            }
            if j >= self.bytes.len() || self.bytes[j] != b'{' {
                break;
            }
            j += 1; // past {
            j = Self::find_matching_brace(self.bytes, j);
            consumed_groups += 1;
        }

        Overlay::hide_range(&mut self.overlays, cmd_start, j);
        j
    }

    /// `\tilde{X}`, `\hat{X}`, `\bar{X}`, `\vec{X}`, etc. — hide the
    /// command wrapper and attach the combining mark to the content's last
    /// grapheme so `\tilde{x}` renders as `x̃`, not `~{x}` or a loose mark.
    fn scan_combining_mark_command(
        &mut self,
        cmd_start: usize,
        i: usize,
        name: &str,
    ) -> usize {
        let combining: &'static str = match name {
            "tilde" | "widetilde" => "\u{0303}",
            "bar" | "overline" => "\u{0304}",
            "hat" | "widehat" => "\u{0302}",
            "vec" => "\u{20D7}",
            "dot" => "\u{0307}",
            "ddot" => "\u{0308}",
            "mathring" => "\u{030A}",
            _ => return self.scan_simple_command(cmd_start, i, name),
        };

        // Find the `{`; bail to the simple-command path if there isn't one.
        let mut j = i;
        while j < self.bytes.len() && self.bytes[j] == b' ' {
            j += 1;
        }
        if j >= self.bytes.len() || self.bytes[j] != b'{' {
            return self.scan_simple_command(cmd_start, i, name);
        }
        let open_brace = j;
        j += 1;
        let content_start = j;
        let close_brace = Self::find_matching_brace(self.bytes, j) - 1;
        if close_brace <= content_start {
            // Empty body — just hide everything.
            Overlay::hide_range(&mut self.overlays, cmd_start, close_brace + 1);
            return close_brace + 1;
        }

        // Hide `\cmd` and the opening brace.
        Overlay::hide_range(&mut self.overlays, cmd_start, open_brace + 1);
        // Hide the closing brace and stick the combining mark in its place.
        // Combining marks render against the preceding grapheme, so placing
        // it where the `}` was means it attaches to the content's last
        // character rather than floating off the end.
        self.overlays.push(Overlay::at(close_brace, combining));
        close_brace + 1
    }

    /// Simple `\name` lookup — falls back to the Julia symbol table.
    ///
    /// LaTeX math operators (`\cos`, `\sin`, `\log`, `\max`, …) don't have
    /// a single-glyph Unicode form; they render as upright multi-letter
    /// text. We handle them by hiding just the backslash, leaving the
    /// operator name visible. This is what `\operatorname{cos}` would
    /// produce and matches Jupyter/KaTeX output.
    fn scan_simple_command(&mut self, cmd_start: usize, i: usize, name: &str) -> usize {
        if is_math_operator(name) {
            self.overlays.push(Overlay::hide(cmd_start));
            if is_big_operator(name) {
                self.pending_limits = 2;
            }
            return i;
        }
        let lookup = unicode_lookup(name.to_string());
        if !lookup.is_empty() {
            self.overlays
                .push(Overlay::at(cmd_start, Cow::Owned(lookup)));
            Overlay::hide_range(&mut self.overlays, cmd_start + 1, i);
            if is_big_operator(name) {
                self.pending_limits = 2;
            }
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
    ///
    /// The fence goes at the START of the next content row (prepended to its
    /// first char via a 2-grapheme replacement) so `⎧` / `⎨` / `⎩` stack
    /// vertically at a consistent column rather than trailing off the end of
    /// the previous row. The last separator emits the CLOSING fence (`⎩`)
    /// rather than the mid-fence, so `\end{cases}` on its own line ends up
    /// with nothing to draw and can stay visually empty instead of burning a
    /// row on a lone `⎩`. Falls back to emitting at the `\\` position when
    /// the next row starts with a scanner-special char like `\` or `^`,
    /// where overlaying the first grapheme would clash with another overlay.
    fn scan_row_separator(&mut self, i: usize) -> usize {
        if self.env_stack.is_empty() {
            return i + 1;
        }
        let env = self.env_stack.last_mut().unwrap();
        env.row += 1;
        let row = env.row;
        let total_rows = env.total_rows;
        let env_name = env.env_name.clone();
        let row_fence = if row == total_rows.saturating_sub(1) {
            close_fence(&env_name, total_rows)
        } else {
            mid_fence(&env_name, row, total_rows)
        };
        Overlay::hide_range(&mut self.overlays, i, i + 2);
        if !row_fence.is_empty() {
            let mut j = i + 2;
            while j < self.bytes.len()
                && matches!(self.bytes[j], b' ' | b'\t' | b'\n' | b'\r')
            {
                j += 1;
            }
            let placed = if j < self.bytes.len() {
                let ch = self.text[j..].chars().next().unwrap();
                if ch.is_ascii_alphanumeric() {
                    let replacement = format!("{row_fence}{ch}");
                    self.overlays.push(Overlay::at(j, replacement));
                    true
                } else {
                    false
                }
            } else {
                false
            };
            if !placed {
                self.overlays.push(Overlay::at(i, row_fence));
            }
        }
        i + 2
    }

    /// `^{...}` — emit one superscript glyph per content character.
    /// Partial emission: if at least one character has a superscript variant,
    /// hide the `^{...}` delimiters and emit what we can; leave unmappable
    /// characters as plain text. This keeps `^{T*}` readable as `ᵀ*` rather
    /// than leaving the whole thing raw.
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

        if self.pending_limits > 0 {
            self.pending_limits -= 1;
            if self.options.hide_math_layout {
                // The math-render plugin is painting this limit above/
                // below the operator glyph — hide the whole inline form
                // including content so we don't render both.
                Overlay::hide_range(&mut self.overlays, caret_pos, past_close);
                return past_close;
            }
            // Default: keep limits inline but hide only the braces so
            // `^{...}` reads as `^...`, preserving the `^` as a visual
            // cue for sub-vs-super. Return `content_start` so the main
            // loop scans content for `\in`, `\mathbb`, Greek, etc.
            self.overlays.push(Overlay::hide(caret_pos + 1));
            self.overlays.push(Overlay::hide(past_close - 1));
            return content_start;
        }

        let supers: Vec<Option<&'static str>> = content
            .chars()
            .map(|c| map_lookup(SUPER_MAP, c))
            .collect();
        let any_mapped = supers.iter().any(Option::is_some);

        if any_mapped && is_simple_brace_content(content) {
            self.overlays.push(Overlay::hide(caret_pos));
            self.overlays.push(Overlay::hide(caret_pos + 1));
            let mut char_offset = content_start;
            for (ci, ch) in content.chars().enumerate() {
                if let Some(rep) = supers[ci] {
                    self.overlays.push(Overlay::at(char_offset, rep));
                }
                char_offset += ch.len_utf8();
            }
            self.overlays.push(Overlay::hide(past_close - 1));
            return past_close;
        }

        // Content isn't simple enough for in-place unicode super substitution
        // (contains backslash commands like `\pi`, nested braces, etc.). Keep
        // the `^` as a visual cue, hide only the braces, and return
        // `content_start` so the main scan loop walks the body — that way
        // `\pi` → π, `\in` → ∈, etc. still get concealed inside the exponent,
        // even though the result can't be shrunk to unicode superscript.
        self.overlays.push(Overlay::hide(caret_pos + 1));
        self.overlays.push(Overlay::hide(past_close - 1));
        content_start
    }

    /// `^x` single-character superscript.
    fn scan_inline_superscript(&mut self, i: usize) -> usize {
        if self.pending_limits > 0 {
            self.pending_limits -= 1;
            if self.options.hide_math_layout {
                // Hide `^x` entirely so only the stacked virtual-line
                // version is visible.
                self.overlays.push(Overlay::hide(i));
                self.overlays.push(Overlay::hide(i + 1));
                return i + 2;
            }
            // Keep `^x` visible but return i+2 (skip past both the caret
            // and the char) — otherwise the main loop re-visits the char,
            // fails the pending-limits whitelist, and zeroes the counter,
            // which means the *next* limit (`^` after `_0`) falls through
            // to the regular-super path and gets converted to unicode.
            // That produced the asymmetric `∫_0¹` rendering: sub at
            // normal size, super shrunk.
            return i + 2;
        }
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
    /// Partial emission: if at least one character has a subscript variant,
    /// hide the `_{...}` delimiters and emit what we can; leave unmappable
    /// characters as plain text. This keeps `_{N-k}` readable as `N₋ₖ`
    /// (uppercase N has no Unicode subscript form) rather than leaving the
    /// whole thing raw.
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

        if self.pending_limits > 0 {
            self.pending_limits -= 1;
            if self.options.hide_math_layout {
                Overlay::hide_range(&mut self.overlays, underscore_pos, past_close);
                return past_close;
            }
            self.overlays.push(Overlay::hide(underscore_pos + 1));
            self.overlays.push(Overlay::hide(past_close - 1));
            return content_start;
        }

        let subs: Vec<Option<&'static str>> = content
            .chars()
            .map(|c| map_lookup(SUB_MAP, c))
            .collect();
        let any_mapped = subs.iter().any(Option::is_some);

        if any_mapped && is_simple_brace_content(content) {
            self.overlays.push(Overlay::hide(underscore_pos));
            self.overlays.push(Overlay::hide(underscore_pos + 1));
            let mut char_offset = content_start;
            for (ci, ch) in content.chars().enumerate() {
                if let Some(rep) = subs[ci] {
                    self.overlays.push(Overlay::at(char_offset, rep));
                }
                char_offset += ch.len_utf8();
            }
            self.overlays.push(Overlay::hide(past_close - 1));
            return past_close;
        }

        // Complex content — keep `_` visible as a visual cue, hide braces,
        // and recurse so inner backslash commands (`\in`, `\pi`, `\mathbb`)
        // still get concealed even though we can't shrink them to unicode
        // subscript forms.
        self.overlays.push(Overlay::hide(underscore_pos + 1));
        self.overlays.push(Overlay::hide(past_close - 1));
        content_start
    }

    /// `_x` single-character subscript. Leaves the source raw when `x` has
    /// no Unicode subscript form. (An earlier version fell back to the Julia
    /// symbol table here, which produced false positives like `_c` → ̧
    /// combining-cedilla because single Latin letters happen to name
    /// combining-mark characters in the table.)
    fn scan_inline_subscript(&mut self, i: usize) -> usize {
        if self.pending_limits > 0 {
            self.pending_limits -= 1;
            if self.options.hide_math_layout {
                self.overlays.push(Overlay::hide(i));
                self.overlays.push(Overlay::hide(i + 1));
                return i + 2;
            }
            // Skip past both `_` and the limit char so pending_limits
            // stays for the paired `^` — see matching note in
            // scan_inline_superscript.
            return i + 2;
        }
        let ch = self.bytes[i + 1] as char;
        if let Some(rep) = map_lookup(SUB_MAP, ch) {
            self.overlays.push(Overlay::at(i, rep));
            self.overlays.push(Overlay::hide(i + 1));
            i + 2
        } else {
            i + 1
        }
    }
}

/// Public FFI entry point. Scans one math region's worth of text and returns
/// a JSON array of `{"offset": N, "replacement": "X"}` entries. Uses
/// default options — callers that need `hide_math_layout` go through
/// [`latex_overlays_with_options`] or [`scan_to_vec_opts`].
pub fn latex_overlays(text: String) -> String {
    latex_overlays_with_options(text, false)
}

/// FFI entry point with explicit `hide_math_layout`. `true` hides big-op
/// limits and `\frac` bodies so the math-render virtual-row plugin can
/// paint them instead.
pub fn latex_overlays_with_options(text: String, hide_math_layout: bool) -> String {
    let opts = ScannerOptions { hide_math_layout };
    let overlays = Scanner::new(&text, opts).scan();
    serde_json::to_string(&overlays).unwrap_or_else(|_| "[]".to_string())
}

/// Scan a math region and return overlays as `(byte_offset, replacement)`
/// tuples. Used by the conceal pipeline; every caller threads options
/// through explicitly so there's no default-variant wrapper anymore.
pub(super) fn scan_to_vec_opts(text: &str, options: ScannerOptions) -> Vec<(usize, String)> {
    Scanner::new(text, options)
        .scan()
        .into_iter()
        .map(|o| (o.offset, o.replacement.into_owned()))
        .collect()
}
