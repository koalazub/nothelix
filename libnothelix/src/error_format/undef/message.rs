use super::super::scan::is_identifier;

const PUNCTUATION: [char; 7] = ['`', '\'', '"', '.', ',', ':', ';'];

pub(super) fn undefined_symbols(message: &str) -> Vec<String> {
    let words: Vec<&str> = message.split_whitespace().collect();
    let mut symbols: Vec<String> = Vec::new();
    for window in words.windows(3) {
        if window[1] != "not" || bare(window[2]) != "defined" {
            continue;
        }
        let symbol = bare(window[0]);
        if is_identifier(symbol) && !symbols.iter().any(|seen| seen == symbol) {
            symbols.push(symbol.to_string());
        }
    }
    symbols
}

pub(super) fn shadowed_package(message: &str) -> Option<String> {
    let words: Vec<&str> = message.split_whitespace().collect();
    words.windows(3).find_map(|window| {
        if window[0] != "exists" || window[1] != "in" {
            return None;
        }
        let package = bare(window[2]);
        is_identifier(package).then(|| package.to_string())
    })
}

fn bare(token: &str) -> &str {
    token.trim_matches(PUNCTUATION)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_symbols_modern_form() {
        let msg = "UndefVarError: `eigen` not defined in `Main`";
        assert_eq!(undefined_symbols(msg), vec!["eigen"]);
    }

    #[test]
    fn parse_symbols_legacy_form() {
        assert_eq!(
            undefined_symbols("UndefVarError: myvar not defined"),
            vec!["myvar"]
        );
    }

    #[test]
    fn parse_symbols_multiple_in_order() {
        let msg = "`eigen` not defined. also `A` not defined";
        assert_eq!(undefined_symbols(msg), vec!["eigen", "A"]);
    }

    #[test]
    fn parse_package_hint_extracts_pkg() {
        let msg = "UndefVarError: `eigen` not defined in `Main`. Hint: a global \
                   variable of this name also exists in LinearAlgebra.";
        assert_eq!(shadowed_package(msg).as_deref(), Some("LinearAlgebra"));
    }

    #[test]
    fn parse_package_hint_absent_yields_none() {
        assert_eq!(shadowed_package("`x` not defined"), None);
    }
}
