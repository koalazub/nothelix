#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CellKind {
    Code,
    Markdown,
    Typst,
    Raw,
}

impl CellKind {
    const ALL: [Self; 4] = [Self::Code, Self::Markdown, Self::Raw, Self::Typst];

    const fn tag(self) -> &'static str {
        match self {
            Self::Code => "@cell",
            Self::Markdown => "@markdown",
            Self::Typst => "@typst",
            Self::Raw => "@raw",
        }
    }

    pub(super) const fn is_prose(self) -> bool {
        matches!(self, Self::Markdown | Self::Typst | Self::Raw)
    }

    pub(super) const fn ipynb_type(self) -> &'static str {
        match self {
            Self::Code => "code",
            Self::Markdown | Self::Typst => "markdown",
            Self::Raw => "raw",
        }
    }
}

const UNSTAMPED: isize = 0;

pub(super) struct Marker {
    pub kind: CellKind,
    pub index: isize,
    pub comment: String,
}

impl Marker {
    pub(super) fn parse(line: &str) -> Option<Self> {
        CellKind::ALL
            .into_iter()
            .find_map(|kind| Self::parse_as(line, kind))
    }

    fn parse_as(line: &str, kind: CellKind) -> Option<Self> {
        let tag = kind.tag();
        if line.trim_end() == tag {
            return Some(Self {
                kind,
                index: UNSTAMPED,
                comment: String::new(),
            });
        }
        let rest = line.strip_prefix(tag)?.strip_prefix(' ')?;
        Some(Self {
            kind,
            index: stamped_index(rest),
            comment: trailing_comment(rest),
        })
    }
}

fn stamped_index(rest: &str) -> isize {
    rest.split_whitespace()
        .next()
        .and_then(|token| token.parse().ok())
        .unwrap_or(UNSTAMPED)
}

fn trailing_comment(rest: &str) -> String {
    let Some(hash) = rest.find(" #") else {
        return String::new();
    };
    let comment = rest[hash + 1..].trim();
    if comment.starts_with('#') {
        comment.to_string()
    } else {
        String::new()
    }
}

#[cfg(test)]
mod tests {
    use super::{CellKind, Marker};

    #[test]
    fn bare_tag_is_a_boundary_without_a_stamp() {
        let marker = Marker::parse("@cell").unwrap();
        assert_eq!(marker.kind, CellKind::Code);
        assert_eq!(marker.index, 0);
        assert_eq!(marker.comment, "");
    }

    #[test]
    fn every_kind_parses_its_own_tag() {
        for (line, kind) in [
            ("@cell 0 :julia", CellKind::Code),
            ("@markdown 1", CellKind::Markdown),
            ("@raw 2", CellKind::Raw),
            ("@typst 3", CellKind::Typst),
        ] {
            let marker = Marker::parse(line).unwrap();
            assert_eq!(marker.kind, kind, "{line}");
        }
    }

    #[test]
    fn stamp_survives_a_language_suffix_and_a_comment() {
        let marker = Marker::parse("@cell 7 :julia # Q1").unwrap();
        assert_eq!(marker.index, 7);
        assert_eq!(marker.comment, "# Q1");
    }

    #[test]
    fn unparsable_stamp_falls_back_to_the_unstamped_placeholder() {
        assert_eq!(Marker::parse("@cell :julia").unwrap().index, 0);
    }

    #[test]
    fn prose_kinds_are_the_ones_stored_as_comments() {
        assert!(!CellKind::Code.is_prose());
        for kind in [CellKind::Markdown, CellKind::Typst, CellKind::Raw] {
            assert!(kind.is_prose(), "{kind:?}");
        }
    }

    #[test]
    fn typst_cells_land_in_ipynb_as_markdown() {
        assert_eq!(CellKind::Typst.ipynb_type(), "markdown");
        assert_eq!(CellKind::Raw.ipynb_type(), "raw");
        assert_eq!(CellKind::Code.ipynb_type(), "code");
    }

    #[test]
    fn non_marker_lines_are_rejected() {
        for line in ["x = 1", "# @image a.png", "@cells 0", ""] {
            assert!(Marker::parse(line).is_none(), "{line}");
        }
    }
}
