#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum TerminalGraphics {
    Kitty,
    ITerm2,
    UnicodeBlocks,
}

impl TerminalGraphics {
    pub(super) fn detect() -> Self {
        if std::env::var("KITTY_WINDOW_ID").is_ok() {
            return Self::Kitty;
        }
        if let Ok(term) = std::env::var("TERM")
            && term.contains("kitty")
        {
            return Self::Kitty;
        }
        if let Ok(program) = std::env::var("TERM_PROGRAM")
            && (program == "iTerm.app" || program == "WezTerm")
        {
            return Self::ITerm2;
        }
        Self::UnicodeBlocks
    }

    pub(super) fn name(self) -> &'static str {
        match self {
            Self::Kitty => "kitty",
            Self::ITerm2 => "iterm",
            Self::UnicodeBlocks => "block",
        }
    }
}
