use super::extract_var_name;

pub(super) fn enrich(message: &str, source: &str) -> Option<String> {
    let count = scan_element_count(message)?;
    let indexed = scan_indexed_var(source)?;
    Some(format!(
        "   = note: `{indexed}` has {count} elements (valid indices: 1 to {count})\n"
    ))
}

fn scan_element_count(msg: &str) -> Option<String> {
    let idx = msg.find("-element")?;
    let before = &msg[..idx];
    let start = before
        .rfind(|c: char| !c.is_ascii_digit())
        .map_or(0, |i| i + 1);
    let count = &before[start..];
    (!count.is_empty()).then(|| count.to_string())
}

fn scan_indexed_var(source: &str) -> Option<String> {
    let bracket = source.find('[')?;
    let name = extract_var_name(source[..bracket].trim());
    (!name.is_empty()).then_some(name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn enrich_names_indexed_var_and_length() {
        let out = enrich(
            "BoundsError: attempt to access 5-element Vector{Int64} at index [9]",
            "arr[9]",
        )
        .expect("message and source both parse");
        assert!(out.contains("`arr` has 5 elements"), "got:\n{out}");
        assert!(out.contains("valid indices: 1 to 5"), "got:\n{out}");
    }

    #[test]
    fn enrich_returns_none_without_indexed_var() {
        assert!(enrich("5-element Vector at index [1]", "println(\"hi\")").is_none());
    }

    #[test]
    fn enrich_returns_none_without_element_count() {
        assert!(enrich("some other error", "arr[1]").is_none());
    }
}
