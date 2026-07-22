use super::hints::ErrorHint;
use super::tokenize::ErrorTokens;

const PRIORITY_WEIGHT: isize = 1000;
const TYPED_BONUS: isize = 10;

pub(super) fn find_hint<'a>(hints: &'a [ErrorHint], tokens: &ErrorTokens) -> Option<&'a ErrorHint> {
    let full_text = format!("{}: {}", tokens.error_type, tokens.message);
    let mut best: Option<(isize, &ErrorHint)> = None;

    for hint in hints {
        if !hint.match_type.is_empty() && !tokens.error_type.contains(&hint.match_type) {
            continue;
        }
        if !hint.selector.matches(&full_text) {
            continue;
        }
        let score = score(hint);
        if best.is_none_or(|(best_score, _)| score > best_score) {
            best = Some((score, hint));
        }
    }

    best.map(|(_, hint)| hint)
}

fn score(hint: &ErrorHint) -> isize {
    hint.priority as isize * PRIORITY_WEIGHT
        + if hint.match_type.is_empty() {
            0
        } else {
            TYPED_BONUS
        }
        + hint.selector.specificity() as isize
}

pub(super) fn expand_template(template: &str, tokens: &ErrorTokens) -> String {
    let mut result = template.replace("{func}", &tokens.func_name);
    for (index, type_name) in tokens.types.iter().enumerate() {
        result = result.replace(&format!("{{type{}}}", index + 1), type_name);
    }
    let var = tokens
        .message
        .split_whitespace()
        .next()
        .map_or("", |word| word.trim_matches('`'));
    result.replace("{var}", var)
}

#[cfg(test)]
mod tests {
    use super::super::tokenize::tokenize_error;
    use super::*;

    #[test]
    fn expand_fills_func_type_and_var_placeholders() {
        let tokens = tokenize_error("MethodError", "no method matching f(::Int64, ::Float64)");
        let out = expand_template("{func} got {type1} and {type2}", &tokens);
        assert_eq!(out, "f got Int64 and Float64");
    }

    #[test]
    fn expand_var_uses_the_first_word_without_backticks() {
        let tokens = tokenize_error("UndefVarError", "`eigen` not defined");
        assert_eq!(expand_template("{var} missing", &tokens), "eigen missing");
    }

    #[test]
    fn expand_var_on_empty_message_yields_empty() {
        let tokens = tokenize_error("UndefVarError", "");
        assert_eq!(expand_template("[{var}]", &tokens), "[]");
    }
}
