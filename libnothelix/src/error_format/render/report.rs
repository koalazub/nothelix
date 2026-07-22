use std::fmt::Write;

use super::super::text::wrap;

const HELP_WIDTH: usize = 72;

pub enum Headline<'a> {
    Coded { code: &'a str, title: &'a str },
    CodeOnly(&'a str),
    Untyped(&'a str),
}

#[derive(Default)]
pub struct Report(String);

impl Report {
    pub fn headline(&mut self, headline: &Headline<'_>) {
        let _ = match *headline {
            Headline::Coded { code, title } => writeln!(self.0, "error[{code}]: {title}"),
            Headline::CodeOnly(code) => writeln!(self.0, "error[{code}]"),
            Headline::Untyped(text) => writeln!(self.0, "error: {text}"),
        };
    }

    fn arrow(&mut self, target: &str) {
        let _ = writeln!(self.0, "  --> {target}");
    }

    pub fn arrow_to(&mut self, target: Option<&str>) {
        if let Some(target) = target {
            self.arrow(target);
        }
    }

    pub fn cell_arrow(&mut self, cell_index: i64, cell_line: i64) {
        if cell_index < 0 {
            return;
        }
        if cell_line > 0 {
            self.arrow(&format!("cell {cell_index}, line {cell_line}"));
        } else {
            self.arrow(&format!("cell {cell_index}"));
        }
    }

    pub fn gutter(&mut self) {
        self.0.push_str("   |\n");
    }

    pub fn source_frame(&mut self, source_line: &str, cell_line: i64) {
        if source_line.is_empty() || cell_line <= 0 {
            return;
        }
        let source = source_line.trim_end();
        let _ = writeln!(self.0, "{cell_line:>3} | {source}");
        let leading = source_line.len() - source_line.trim_start().len();
        let width = source.trim().len();
        if width > 0 {
            let _ = writeln!(self.0, "   | {}{}", " ".repeat(leading), "^".repeat(width));
        }
    }

    pub fn quoted(&mut self, text: &str) {
        let _ = writeln!(self.0, "   | {text}");
    }

    pub fn note(&mut self, text: &str) {
        let _ = writeln!(self.0, "   = note: {text}");
    }

    pub fn help(&mut self, text: &str) {
        let _ = writeln!(self.0, "   = help: {text}");
    }

    pub fn wrapped_help(&mut self, text: &str) {
        for line in wrap(text, HELP_WIDTH) {
            self.help(&line);
        }
    }

    pub fn example(&mut self, text: &str) {
        self.0.push_str("   = example:\n");
        for line in text.lines() {
            let _ = writeln!(self.0, "   |   {line}");
        }
    }

    pub fn guidance(&mut self, text: &str) {
        let _ = writeln!(self.0, "   = {text}");
    }

    pub fn block(&mut self, text: &str) {
        for line in text.lines() {
            self.0.push_str(line);
            self.0.push('\n');
        }
    }

    pub fn call_chain(&mut self, entries: &[String]) {
        self.0.push_str("   = note: call chain:\n");
        for (position, entry) in entries.iter().enumerate() {
            let prefix = if position == 0 {
                "   |   → "
            } else {
                "   |     → "
            };
            let _ = writeln!(self.0, "{prefix}{entry}");
        }
    }

    pub fn finish(self) -> String {
        self.0
    }
}
