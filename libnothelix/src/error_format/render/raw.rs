use super::super::enrichment::scan_error_location;
use super::super::hints::ErrorHint;
use super::super::matching::{expand_template, find_hint};
use super::super::text::{clean_message, first_line, truncate};
use super::super::tokenize::{ErrorTokens, tokenize_error};
use super::report::{Headline, Report};

const MAX_CONTENT_LINES: usize = 3;
const HEADLINE_WIDTH: usize = 100;
const QUOTED_WIDTH: usize = 120;

const HINTED_NOISE: [&str; 3] = ["Error @", "ParseError", "LoadError"];
const UNHINTED_NOISE: [&str; 1] = ["Error @"];

pub fn format_raw(raw: &str, hints: &[ErrorHint]) -> String {
    let cleaned = clean_message(raw);
    let tokens = tokenize_error("", cleaned);
    let location = scan_error_location(cleaned);
    let mut report = Report::default();

    match find_hint(hints, &tokens) {
        Some(hint) => {
            report.headline(&Headline::Coded {
                code: &hint.id,
                title: &expand_template(&hint.title, &tokens),
            });
            report.arrow_to(location.as_deref());
            report.gutter();

            let content = content_lines(cleaned, &HINTED_NOISE, 0);
            if content.is_empty() {
                report.quoted(&truncate(first_line(cleaned), QUOTED_WIDTH));
            }
            for line in content {
                report.quoted(line);
            }
            report.gutter();

            if !hint.help.is_empty() {
                report.wrapped_help(&expand_template(&hint.help, &tokens));
            }
            if !hint.example.is_empty() {
                report.example(&expand_template(&hint.example, &tokens));
            }
        }
        None => {
            let title = untyped_title(&tokens, cleaned);
            report.headline(&untyped_headline(&tokens, &title));
            report.arrow_to(location.as_deref());
            report.gutter();

            let content = content_lines(cleaned, &UNHINTED_NOISE, 1);
            if content.is_empty() && !tokens.message.is_empty() {
                report.quoted(&tokens.message);
            }
            for line in content {
                report.quoted(line);
            }
            report.gutter();
        }
    }

    report.finish()
}

fn untyped_title(tokens: &ErrorTokens, cleaned: &str) -> String {
    if tokens.error_type.is_empty() {
        truncate(first_line(cleaned), HEADLINE_WIDTH)
    } else {
        truncate(&tokens.message, HEADLINE_WIDTH)
    }
}

fn untyped_headline<'a>(tokens: &'a ErrorTokens, title: &'a str) -> Headline<'a> {
    if tokens.error_type.is_empty() {
        Headline::Untyped(title)
    } else if tokens.message.is_empty() {
        Headline::CodeOnly(&tokens.error_type)
    } else {
        Headline::Coded {
            code: &tokens.error_type,
            title,
        }
    }
}

fn content_lines<'a>(cleaned: &'a str, noise: &[&str], skip: usize) -> Vec<&'a str> {
    cleaned
        .lines()
        .filter(|line| {
            let text = line.trim().trim_start_matches("# ");
            !text.is_empty() && !noise.iter().any(|prefix| text.starts_with(prefix))
        })
        .skip(skip)
        .take(MAX_CONTENT_LINES)
        .collect()
}
