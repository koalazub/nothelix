const READABLE_ON_DARK: &str = "e8e8e8";

#[derive(Clone, PartialEq, Eq, Hash)]
pub(crate) struct HexColor(String);

impl HexColor {
    pub(crate) fn parse(input: &str) -> Self {
        let hex = input.trim().trim_start_matches('#');
        let expanded = match hex.len() {
            3 => hex.chars().flat_map(|c| [c, c]).collect::<String>(),
            6 => hex.to_string(),
            _ => return Self::default(),
        };
        if expanded.bytes().all(|b| b.is_ascii_hexdigit()) {
            Self(expanded.to_ascii_lowercase())
        } else {
            Self::default()
        }
    }

    pub(crate) fn as_str(&self) -> &str {
        &self.0
    }
}

impl Default for HexColor {
    fn default() -> Self {
        Self(READABLE_ON_DARK.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::HexColor;

    #[test]
    fn hex_colour_normalises_and_defaults() {
        assert_eq!(HexColor::parse("#FFFFFF").as_str(), "ffffff");
        assert_eq!(HexColor::parse("e8e8e8").as_str(), "e8e8e8");
        assert_eq!(HexColor::parse("fff").as_str(), "ffffff");
        assert_eq!(HexColor::parse("not-a-colour").as_str(), "e8e8e8");
        assert_eq!(HexColor::parse("").as_str(), "e8e8e8");
    }
}
