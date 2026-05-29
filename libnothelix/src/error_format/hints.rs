//! Hint definitions — the `error_hints.toml`-loaded knowledge base.
//!
//! Each hint binds a selector (token-based predicate over an error
//! message) to a user-facing title/help/example/example trio. The
//! `hints()` accessor lazily loads + sorts the static table by
//! (priority desc, specificity desc).

use std::sync::OnceLock;

use serde::Deserialize;

static HINTS_TOML: &str = include_str!("../../error_hints.toml");

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
    /// Optional nested-form selector. When present it supersedes
    /// `match_tokens`/`exclude_tokens`. Shape:
    ///   match = { all = ["a"], any = ["b","c"], none = ["d"] }
    /// Empty `all`/`any` means "no requirement" on that axis.
    #[serde(default)]
    r#match: Option<RawSelector>,
    title: String,
    help: String,
    #[serde(default)]
    example: String,
    /// Explicit tie-breaker when multiple hints would match. Higher
    /// `priority` wins; within equal priority the rule with more
    /// `match_tokens` (more specific) wins; among equals, first in
    /// file order wins.
    #[serde(default)]
    priority: i32,
}

/// Nested selector that replaces the flat `match_tokens`/`exclude_tokens`
/// split. Each axis is independent:
///   `all`   — every listed token MUST appear in the error message
///   `any`   — at least ONE listed token must appear (empty = ignored)
///   `none`  — NONE of the listed tokens may appear
#[derive(Debug, Default, Deserialize, Clone)]
#[serde(rename_all = "snake_case")]
struct RawSelector {
    #[serde(default)]
    all: Vec<String>,
    #[serde(default)]
    any: Vec<String>,
    #[serde(default)]
    none: Vec<String>,
}

pub struct ErrorHint {
    pub id: String,
    pub match_type: String,
    /// Normalised selector. `flat_tokens(&self.selector)` / similar can
    /// rebuild the old flat view for hints that need it; `match_hint`
    /// below consults the selector directly.
    pub selector: Selector,
    pub title: String,
    pub help: String,
    pub example: String,
    pub priority: i32,
}

/// The in-memory, validated form of `RawSelector`. Collapses the old
/// flat `match_tokens` + `exclude_tokens` fields into the same shape
/// as nested `match = { ... }`, so the matcher only has to walk one
/// structure regardless of how the hint was spelled in TOML.
#[derive(Debug, Clone, Default)]
pub struct Selector {
    pub all: Vec<String>,
    pub any: Vec<String>,
    pub none: Vec<String>,
}

impl Selector {
    /// Derive from the raw TOML. If `nested` is present it wins;
    /// otherwise fold the legacy flat fields in (`match_tokens` → `all`,
    /// `exclude_tokens` → `none`).
    fn from_raw(
        nested: Option<RawSelector>,
        flat_all: Vec<String>,
        flat_none: Vec<String>,
    ) -> Self {
        match nested {
            Some(s) => Self { all: s.all, any: s.any, none: s.none },
            None => Self { all: flat_all, any: Vec::new(), none: flat_none },
        }
    }

    /// Constraint count — used by the specificity score in `find_hint`.
    pub fn specificity(&self) -> usize {
        self.all.len() + self.any.len() + self.none.len()
    }

    pub fn matches(&self, full_text: &str) -> bool {
        if !self.all.iter().all(|t| full_text.contains(t.as_str())) {
            return false;
        }
        if !self.any.is_empty() && !self.any.iter().any(|t| full_text.contains(t.as_str())) {
            return false;
        }
        if self.none.iter().any(|t| full_text.contains(t.as_str())) {
            return false;
        }
        true
    }
}

static HINTS: OnceLock<Vec<ErrorHint>> = OnceLock::new();

pub fn hints() -> &'static [ErrorHint] {
    HINTS.get_or_init(|| {
        let file: HintsFile =
            toml::from_str(HINTS_TOML).unwrap_or(HintsFile { hint: vec![] });

        let mut loaded: Vec<ErrorHint> = file
            .hint
            .into_iter()
            .map(|h| {
                let selector =
                    Selector::from_raw(h.r#match, h.match_tokens, h.exclude_tokens);
                ErrorHint {
                    id: h.id,
                    match_type: h.match_type,
                    selector,
                    title: h.title,
                    help: h.help,
                    example: h.example,
                    priority: h.priority,
                }
            })
            .collect();

        // Priority-dominant ordering — more-specific selectors break
        // priority ties. Sort is stable so equal keys keep file order.
        loaded.sort_by(|a, b| {
            b.priority
                .cmp(&a.priority)
                .then_with(|| b.selector.specificity().cmp(&a.selector.specificity()))
        });

        // Drop duplicate ids. The table is under 100 entries so a
        // linear scan via `any`+`==` is cheaper than allocating a
        // HashSet. Keeps the first occurrence (highest priority, most
        // specific) after the sort above.
        let mut out: Vec<ErrorHint> = Vec::with_capacity(loaded.len());
        for hint in loaded {
            if out.iter().any(|existing| existing.id == hint.id) {
                eprintln!(
                    "error_hints.toml: duplicate hint id `{}` — dropping shadowed entry",
                    hint.id
                );
                continue;
            }
            out.push(hint);
        }
        out
    })
}
