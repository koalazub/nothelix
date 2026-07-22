use serde::Deserialize;

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct RawSelector {
    #[serde(default)]
    all: Vec<String>,
    #[serde(default)]
    any: Vec<String>,
    #[serde(default)]
    none: Vec<String>,
}

#[derive(Default)]
pub struct Selector {
    all: Vec<String>,
    any: Vec<String>,
    none: Vec<String>,
}

impl Selector {
    pub fn nested(raw: RawSelector) -> Self {
        Self {
            all: raw.all,
            any: raw.any,
            none: raw.none,
        }
    }

    pub fn flat(all: Vec<String>, none: Vec<String>) -> Self {
        Self {
            all,
            any: Vec::new(),
            none,
        }
    }

    pub fn specificity(&self) -> usize {
        self.all.len() + self.any.len() + self.none.len()
    }

    pub fn matches(&self, text: &str) -> bool {
        self.all.iter().all(|t| text.contains(t.as_str()))
            && (self.any.is_empty() || self.any.iter().any(|t| text.contains(t.as_str())))
            && !self.none.iter().any(|t| text.contains(t.as_str()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn words(items: &[&str]) -> Vec<String> {
        items.iter().map(|s| (*s).to_string()).collect()
    }

    #[test]
    fn flat_selector_requires_all_and_forbids_none() {
        let selector = Selector::flat(words(&["alpha"]), words(&["beta"]));
        assert!(selector.matches("alpha only"));
        assert!(!selector.matches("alpha and beta"));
        assert!(!selector.matches("nothing"));
    }

    #[test]
    fn any_axis_is_ignored_when_empty() {
        let selector = Selector::flat(words(&["alpha"]), Vec::new());
        assert!(selector.matches("alpha"));
    }

    #[test]
    fn specificity_counts_every_axis() {
        let selector = Selector::nested(RawSelector {
            all: words(&["a"]),
            any: words(&["b", "c"]),
            none: words(&["d"]),
        });
        assert_eq!(selector.specificity(), 4);
        assert!(selector.matches("a b"));
        assert!(!selector.matches("a d b"));
        assert!(!selector.matches("a only"));
    }
}
