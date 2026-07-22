#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum Align {
    Left,
    Center,
    Right,
}

impl Align {
    pub(super) fn typst(self) -> &'static str {
        match self {
            Self::Left => "left",
            Self::Center => "center",
            Self::Right => "right",
        }
    }

    fn from_rule_cell(cell: &str) -> Self {
        let rule = cell.trim();
        match (rule.starts_with(':'), rule.ends_with(':')) {
            (true, true) => Self::Center,
            (false, true) => Self::Right,
            _ => Self::Left,
        }
    }
}

pub(super) struct PipeTable {
    pub(super) aligns: Vec<Align>,
    pub(super) header: Vec<String>,
    pub(super) body: Vec<Vec<String>>,
}

impl PipeTable {
    pub(super) fn parse(lines: &[&str]) -> Option<Self> {
        let mut header: Option<Vec<String>> = None;
        let mut aligns: Option<Vec<Align>> = None;
        let mut body: Vec<Vec<String>> = Vec::new();

        for line in lines {
            let trimmed = line.trim();
            if !trimmed.contains('|') {
                return None;
            }
            let cells = split_cells(trimmed);
            if is_rule(&cells) {
                if header.is_none() || aligns.is_some() {
                    return None;
                }
                aligns = Some(cells.iter().map(|c| Align::from_rule_cell(c)).collect());
            } else if header.is_none() {
                header = Some(cells.iter().map(|c| to_plain_text(c)).collect());
            } else {
                body.push(cells.iter().map(|c| to_plain_text(c)).collect());
            }
        }

        let (header, aligns) = (header?, aligns?);
        if body.is_empty() {
            return None;
        }

        let columns = header
            .len()
            .max(body.iter().map(Vec::len).max().unwrap_or(0))
            .max(1);

        Some(Self {
            aligns: (0..columns)
                .map(|i| aligns.get(i).copied().unwrap_or(Align::Left))
                .collect(),
            header,
            body,
        })
    }
}

fn split_cells(line: &str) -> Vec<String> {
    let mut body = line.trim();
    body = body.strip_prefix('|').unwrap_or(body);
    body = body.strip_suffix('|').unwrap_or(body);

    let mut cells = Vec::new();
    let mut cell = String::new();
    let mut chars = body.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\\'
            && let Some(&escaped) = chars.peek()
        {
            cell.push(c);
            cell.push(escaped);
            chars.next();
            continue;
        }
        if c == '|' {
            cells.push(cell.trim().to_string());
            cell.clear();
        } else {
            cell.push(c);
        }
    }
    cells.push(cell.trim().to_string());
    cells
}

fn is_rule(cells: &[String]) -> bool {
    !cells.is_empty()
        && cells.iter().all(|cell| {
            let rule = cell.trim();
            !rule.is_empty() && rule.chars().all(|ch| ch == '-' || ch == ':') && rule.contains('-')
        })
}

fn to_plain_text(cell: &str) -> String {
    let bytes = cell.as_bytes();
    let mut out = String::new();
    let mut i = 0;

    while i < cell.len() {
        let c = bytes[i];

        if c == b'\\'
            && let Some(escaped) = cell[i + 1..].chars().next()
        {
            out.push(escaped);
            i += 1 + escaped.len_utf8();
            continue;
        }

        if c == b'`'
            && let Some(rel) = cell[i + 1..].find('`')
        {
            out.push_str(&cell[i + 1..i + 1 + rel]);
            i = i + 1 + rel + 1;
            continue;
        }

        if c == b'['
            && let Some(close) = cell[i + 1..].find(']')
        {
            let label = &cell[i + 1..i + 1 + close];
            let after = i + 1 + close + 1;
            if bytes.get(after) == Some(&b'(')
                && let Some(rel) = cell[after + 1..].find(')')
            {
                out.push_str(&to_plain_text(label));
                i = after + 1 + rel + 1;
                continue;
            }
        }

        if c == b'*'
            && bytes.get(i + 1) == Some(&b'*')
            && let Some(rel) = cell[i + 2..].find("**")
        {
            out.push_str(&to_plain_text(&cell[i + 2..i + 2 + rel]));
            i = i + 2 + rel + 2;
            continue;
        }

        let Some(ch) = cell[i..].chars().next() else {
            break;
        };
        out.push(if ch == '\t' { ' ' } else { ch });
        i += ch.len_utf8();
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "| File | Description | URL |\n\
                          |:-----|:-----------:|----:|\n\
                          | a.h5 | strain data | link |";

    fn parse(md: &str) -> Option<PipeTable> {
        PipeTable::parse(&md.lines().collect::<Vec<_>>())
    }

    #[test]
    fn parses_header_separator_body() {
        let table = parse(SAMPLE).expect("should parse");
        assert_eq!(table.header, vec!["File", "Description", "URL"]);
        assert_eq!(table.body.len(), 1);
        assert_eq!(table.aligns.len(), 3);
    }

    #[test]
    fn alignment_from_separator_colons() {
        assert_eq!(Align::from_rule_cell(":---"), Align::Left);
        assert_eq!(Align::from_rule_cell(":--:"), Align::Center);
        assert_eq!(Align::from_rule_cell("---:"), Align::Right);
        assert_eq!(Align::from_rule_cell("---"), Align::Left);
    }

    #[test]
    fn rejects_without_separator() {
        assert!(PipeTable::parse(&["| a | b |", "| c | d |"]).is_none());
    }

    #[test]
    fn rejects_non_table() {
        assert!(PipeTable::parse(&["just prose", "more prose"]).is_none());
    }

    #[test]
    fn escaped_pipe_stays_in_cell() {
        let table = parse("| a \\| b | c |\n|--|--|\n| d | e |").expect("should parse");
        assert_eq!(table.header[0], "a | b");
    }

    #[test]
    fn inline_markup_stripped() {
        let table = parse("| `code` | [lbl](http://x) | **b** |\n|--|--|--|\n| x | y | z |")
            .expect("should parse");
        assert_eq!(table.header, vec!["code", "lbl", "b"]);
    }
}
