#[derive(Clone, Copy)]
enum MathDelimiter {
    Dollar,
    DoubleDollar,
    Paren,
}

impl MathDelimiter {
    fn opening_at(bytes: &[u8], i: usize) -> Option<Self> {
        match bytes[i] {
            b'$' if bytes.get(i + 1) == Some(&b'$') => Some(Self::DoubleDollar),
            b'$' => Some(Self::Dollar),
            b'\\' if bytes.get(i + 1) == Some(&b'(') => Some(Self::Paren),
            _ => None,
        }
    }

    fn width(self) -> usize {
        match self {
            Self::Dollar => 1,
            Self::DoubleDollar | Self::Paren => 2,
        }
    }

    fn closing_from(self, bytes: &[u8], start: usize) -> Option<usize> {
        match self {
            Self::Dollar => (start..bytes.len()).find(|&j| bytes[j] == b'$'),
            Self::DoubleDollar => find_pair(bytes, start, b'$', b'$'),
            Self::Paren => find_pair(bytes, start, b'\\', b')'),
        }
    }

    fn accepts(self, content: &str) -> bool {
        !matches!(self, Self::Paren) || looks_like_math(content)
    }
}

fn find_pair(bytes: &[u8], start: usize, first: u8, second: u8) -> Option<usize> {
    (start..bytes.len().saturating_sub(1)).find(|&j| bytes[j] == first && bytes[j + 1] == second)
}

fn looks_like_math(content: &str) -> bool {
    if content.len() <= 2 {
        return false;
    }
    if content
        .bytes()
        .any(|b| matches!(b, b'\\' | b'^' | b'_' | b'{'))
    {
        return true;
    }
    content.len() > 4 || content.contains(['+', '=', '-'])
}

pub(crate) fn find_math_regions(text: &str) -> Vec<(usize, usize)> {
    let bytes = text.as_bytes();
    let mut regions = Vec::new();
    let mut i = 0;

    while i < bytes.len() {
        let Some(delimiter) = MathDelimiter::opening_at(bytes, i) else {
            i += 1;
            continue;
        };
        let start = i + delimiter.width();
        let Some(close) = delimiter.closing_from(bytes, start) else {
            break;
        };
        if delimiter.accepts(&text[start..close]) {
            regions.push((start, close));
        }
        i = close + delimiter.width();
    }

    regions
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_markdown_escaped_parens() {
        let text = r"\(a\) First item \(b\) Second item";
        let regions = find_math_regions(text);
        assert!(
            regions.is_empty(),
            "should reject \\(a\\) as not math, got: {regions:?}"
        );
    }

    #[test]
    fn accepts_real_inline_math() {
        let text = r"\(\alpha + \beta\)";
        let regions = find_math_regions(text);
        assert_eq!(regions.len(), 1);
        assert_eq!(&text[regions[0].0..regions[0].1], r"\alpha + \beta");
    }

    #[test]
    fn inline_dollar_still_works() {
        let text = r"$H(e^{j\omega})$";
        let regions = find_math_regions(text);
        assert_eq!(regions.len(), 1);
    }

    #[test]
    fn mixed_escaped_parens_and_real_math() {
        let text = r"\(b\) Compute $H(e^{j\omega})$. For $\omega \in [-\pi, \pi]$";
        let regions = find_math_regions(text);
        assert_eq!(regions.len(), 2, "got: {regions:?}");
    }

    #[test]
    fn exact_user_line_bandwidth() {
        let text =
            r"\(a\) What is the bandwidth of $x$ (in Hz)? What is the Nyquist rate? \[1 mark\]";
        let regions = find_math_regions(text);
        assert_eq!(regions.len(), 1, "should find $x$, got: {regions:?}");
        assert_eq!(&text[regions[0].0..regions[0].1], "x");
    }

    #[test]
    fn unterminated_region_stops_the_scan() {
        assert!(find_math_regions("$\\alpha").is_empty());
        assert!(find_math_regions("$$\\alpha").is_empty());
        assert!(find_math_regions(r"\(\alpha").is_empty());
    }

    #[test]
    fn display_region_excludes_its_delimiters() {
        let text = "a $$x + y$$ b";
        assert_eq!(find_math_regions(text), vec![(4, 9)]);
        assert_eq!(&text[4..9], "x + y");
    }
}
