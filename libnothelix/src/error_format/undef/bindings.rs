use super::super::scan::find_matching_paren;
use crate::notebook::ScanCell;

pub(super) fn defining_cell<'a>(
    cells: &'a [ScanCell],
    error_cell: i64,
    symbol: &str,
) -> Option<&'a ScanCell> {
    cells
        .iter()
        .filter(|cell| cell.index != error_cell && defines(&cell.code, symbol))
        .min_by_key(|cell| cell.index)
}

pub(super) fn importing_cell<'a>(cells: &'a [ScanCell], package: &str) -> Option<&'a ScanCell> {
    cells
        .iter()
        .filter(|cell| imports(&cell.code, package))
        .min_by_key(|cell| cell.index)
}

fn defines(code: &str, symbol: &str) -> bool {
    code.lines()
        .any(|line| line_defines(without_comment(line), symbol))
}

fn line_defines(line: &str, symbol: &str) -> bool {
    let words: Vec<&str> = line.split_whitespace().collect();
    match words.as_slice() {
        [
            "const" | "global" | "local" | "function" | "macro" | "struct",
            name,
            ..,
        ]
        | ["abstract" | "primitive", "type", name, ..]
        | ["mutable", "struct", name, ..] => head(name) == symbol,
        ["using" | "import", ..] => import_list_binds(line, symbol),
        _ => assignment_binds(line, symbol),
    }
}

fn assignment_binds(line: &str, symbol: &str) -> bool {
    let bytes = line.as_bytes();
    let name = symbol.as_bytes();
    let start = bytes.iter().take_while(|c| c.is_ascii_whitespace()).count();
    if start + name.len() > bytes.len() || &bytes[start..start + name.len()] != name {
        return false;
    }

    let mut at = start + name.len();
    if bytes.get(at).copied().is_some_and(is_name_byte) {
        return false;
    }
    at = skip_blanks(bytes, at);
    if bytes.get(at) == Some(&b'(') {
        let Some(close) = find_matching_paren(bytes, at) else {
            return false;
        };
        at = skip_blanks(bytes, close + 1);
    }

    match bytes.get(at) {
        None => false,
        Some(b'=') => bytes.get(at + 1) != Some(&b'='),
        Some(b'+' | b'-' | b'*' | b'/' | b'^' | b'%') => bytes.get(at + 1) == Some(&b'='),
        Some(_) => false,
    }
}

fn skip_blanks(bytes: &[u8], mut at: usize) -> usize {
    while matches!(bytes.get(at), Some(b' ' | b'\t')) {
        at += 1;
    }
    at
}

fn import_list_binds(line: &str, symbol: &str) -> bool {
    let Some((_, names)) = line.split_once(':') else {
        return false;
    };
    names.split(',').any(|item| {
        item.split_whitespace()
            .next()
            .is_some_and(|name| head(name) == symbol)
    })
}

fn imports(code: &str, package: &str) -> bool {
    code.lines().any(|raw| {
        let line = without_comment(raw);
        let Some(keyword) = line.split_whitespace().next() else {
            return false;
        };
        if keyword != "using" && keyword != "import" {
            return false;
        }
        let rest = line[keyword.len()..].trim();
        let modules = rest.split_once(':').map_or(rest, |(modules, _)| modules);
        modules.split(',').any(|module| {
            let module = module.trim();
            let root = module.split_once('.').map_or(module, |(root, _)| root);
            root.trim() == package
        })
    })
}

fn head(token: &str) -> &str {
    let end = token.bytes().take_while(|c| is_name_byte(*c)).count();
    &token[..end]
}

fn is_name_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_' || b == b'!'
}

fn without_comment(line: &str) -> &str {
    line.split_once('#').map_or(line, |(code, _)| code)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_definition_form_binds() {
        for code in [
            "const K = 3",
            "global K = 3",
            "local K = 3",
            "function K(x)\n    x\nend",
            "K(x) = x + 1",
            "K = 1",
            "K += 1",
            "struct K end",
            "mutable struct K\n    v\nend",
            "abstract type K end",
            "primitive type K 8 end",
            "macro K(x) end",
            "using LinearAlgebra: K",
        ] {
            assert!(defines(code, "K"), "did not bind: {code:?}");
        }
    }

    #[test]
    fn equality_and_lookalikes_do_not_bind() {
        for code in ["if K == 3\nend", "Kappa = 1", "f(K)", "# K = 1"] {
            assert!(!defines(code, "K"), "wrongly bound: {code:?}");
        }
    }

    #[test]
    fn imports_match_any_position_in_a_comma_list() {
        assert!(imports("using Statistics, LinearAlgebra", "LinearAlgebra"));
        assert!(imports("import LinearAlgebra: eigen", "LinearAlgebra"));
        assert!(imports("using LinearAlgebra.BLAS", "LinearAlgebra"));
        assert!(!imports("using Statistics", "LinearAlgebra"));
    }

    #[test]
    fn head_takes_the_leading_identifier() {
        assert_eq!(head("foo(x,"), "foo");
        assert_eq!(head("Bar{T}"), "Bar");
    }
}
