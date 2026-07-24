mod selector;

use std::sync::OnceLock;

use serde::Deserialize;

use crate::error::{Error, Result};

use selector::{RawSelector, Selector};

static HINTS_TOML: &str = include_str!("../../../error_hints.toml");
static HINTS: OnceLock<Vec<ErrorHint>> = OnceLock::new();

pub(super) struct ErrorHint {
    pub id: String,
    pub match_type: String,
    pub selector: Selector,
    pub title: String,
    pub help: String,
    pub example: String,
    pub priority: i32,
    pub note_kernel_dir: bool,
}

#[derive(Deserialize)]
struct HintsFile {
    hint: Vec<RawHint>,
}

#[derive(Deserialize)]
struct RawHint {
    id: String,
    #[serde(default)]
    match_type: String,
    #[serde(default)]
    match_tokens: Vec<String>,
    #[serde(default)]
    exclude_tokens: Vec<String>,
    #[serde(default)]
    r#match: Option<RawSelector>,
    title: String,
    help: String,
    #[serde(default)]
    example: String,
    #[serde(default)]
    priority: i32,
    #[serde(default)]
    note_kernel_dir: bool,
}

impl From<RawHint> for ErrorHint {
    fn from(raw: RawHint) -> Self {
        Self {
            selector: match raw.r#match {
                Some(nested) => Selector::nested(nested),
                None => Selector::flat(raw.match_tokens, raw.exclude_tokens),
            },
            id: raw.id,
            match_type: raw.match_type,
            title: raw.title,
            help: raw.help,
            example: raw.example,
            priority: raw.priority,
            note_kernel_dir: raw.note_kernel_dir,
        }
    }
}

pub(super) fn hints() -> Result<&'static [ErrorHint]> {
    if let Some(cached) = HINTS.get() {
        return Ok(cached.as_slice());
    }
    let loaded = load()?;
    Ok(HINTS.get_or_init(|| loaded).as_slice())
}

fn load() -> Result<Vec<ErrorHint>> {
    let file: HintsFile = toml::from_str(HINTS_TOML).map_err(|source| Error::Toml {
        subject: "error_hints.toml",
        source,
    })?;

    let mut hints: Vec<ErrorHint> = file.hint.into_iter().map(ErrorHint::from).collect();
    hints.sort_by(|a, b| {
        b.priority
            .cmp(&a.priority)
            .then_with(|| b.selector.specificity().cmp(&a.selector.specificity()))
    });

    for (position, hint) in hints.iter().enumerate() {
        if hints[..position]
            .iter()
            .any(|earlier| earlier.id == hint.id)
        {
            return Err(Error::Malformed {
                subject: "error_hints.toml",
                detail: format!("duplicate hint id `{}`", hint.id),
            });
        }
    }
    Ok(hints)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn table_loads_with_every_hint() {
        let hints = hints().expect("error_hints.toml must parse");
        assert!(hints.len() >= 50, "got {}", hints.len());
    }

    #[test]
    fn table_is_cached_by_identity() {
        let a = hints().expect("error_hints.toml must parse");
        let b = hints().expect("error_hints.toml must parse");
        assert!(std::ptr::eq(a, b));
    }

    #[test]
    fn table_is_sorted_by_priority_then_specificity() {
        let hints = hints().expect("error_hints.toml must parse");
        for pair in hints.windows(2) {
            let ordered = pair[0].priority > pair[1].priority
                || (pair[0].priority == pair[1].priority
                    && pair[0].selector.specificity() >= pair[1].selector.specificity());
            assert!(ordered, "out of order at `{}`", pair[1].id);
        }
    }
}
