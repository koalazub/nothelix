//! Hint matcher.
//!
//! Picks the best [`ErrorHint`] for a tokenized error message. Scoring
//! is priority-dominant (×1000), then `match_type` presence as a +10
//! nudge, then constraint count as specificity. Within a tie the input
//! file order wins because the underlying sort in `hints()` is stable.

use super::hints::ErrorHint;
use super::tokenize::ErrorTokens;

/// Find the best-matching hint for the given tokens. `None` when no
/// hint's selector matches the full `"error_type: message"` string.
pub(super) fn find_hint<'a>(
    hints: &'a [ErrorHint],
    tokens: &ErrorTokens,
) -> Option<&'a ErrorHint> {
    let full_text = format!("{}: {}", tokens.error_type, tokens.message);
    let mut best: Option<(isize, &ErrorHint)> = None;

    for hint in hints {
        if !hint.match_type.is_empty() && !tokens.error_type.contains(&hint.match_type) {
            continue;
        }
        if !hint.selector.matches(&full_text) {
            continue;
        }

        let score = hint.priority as isize * 1000
            + if hint.match_type.is_empty() { 0 } else { 10 }
            + hint.selector.specificity() as isize;

        if best.is_none_or(|(s, _)| score > s) {
            best = Some((score, hint));
        }
    }

    best.map(|(_, h)| h)
}

/// Expand `{func}` / `{type1..N}` / `{var}` placeholders in a hint
/// template using tokens extracted from the error message.
pub(super) fn expand_template(template: &str, tokens: &ErrorTokens) -> String {
    let mut result = template.to_string();

    if result.contains("{func}") {
        result = result.replace("{func}", &tokens.func_name);
    }

    for (i, t) in tokens.types.iter().enumerate() {
        let placeholder = format!("{{type{}}}", i + 1);
        result = result.replace(&placeholder, t);
    }

    // {var} → first word of the message (for `UndefVarError: varname not defined`)
    if result.contains("{var}") {
        let var = tokens
            .message
            .split_whitespace()
            .next()
            .unwrap_or("")
            .trim_matches('`');
        result = result.replace("{var}", var);
    }

    result
}
