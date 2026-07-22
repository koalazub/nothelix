use std::collections::HashMap;

const OPEN_PREFIX: &str = "<!-- gallery:";
const OPEN_SUFFIX: &str = "-->";
const CLOSE: &str = "<!-- /gallery -->";

pub fn inject_regions(
    document: &str,
    artifacts: &HashMap<String, String>,
) -> Result<String, String> {
    let mut out = String::new();
    let mut open: Option<(String, usize)> = None;

    for (index, line) in document.lines().enumerate() {
        let number = index + 1;
        let trimmed = line.trim();

        if trimmed == CLOSE {
            if open.take().is_none() {
                return Err(format!(
                    "gallery: line {number} closes a region that was never opened by `{OPEN_PREFIX}<name> {OPEN_SUFFIX}`"
                ));
            }
            out.push_str(line);
            out.push('\n');
            continue;
        }

        if let Some(name) = open_marker(trimmed) {
            if let Some((previous, at)) = &open {
                return Err(format!(
                    "gallery: line {number} opens `{name}` while `{previous}` from line {at} is still open"
                ));
            }
            let body = artifacts.get(name).ok_or_else(|| {
                format!(
                    "gallery: line {number} asks for `{name}`, which is not a generated artifact"
                )
            })?;
            out.push_str(line);
            out.push('\n');
            push_fenced(&mut out, body);
            open = Some((name.to_string(), number));
            continue;
        }

        if open.is_none() {
            out.push_str(line);
            out.push('\n');
        }
    }

    if let Some((name, number)) = open {
        return Err(format!(
            "gallery: `{name}` opened at line {number} is never closed by `{CLOSE}`"
        ));
    }

    if !document.ends_with('\n') {
        out.pop();
    }
    Ok(out)
}

fn open_marker(trimmed: &str) -> Option<&str> {
    let name = trimmed
        .strip_prefix(OPEN_PREFIX)?
        .strip_suffix(OPEN_SUFFIX)?
        .trim();
    (!name.is_empty()).then_some(name)
}

fn push_fenced(out: &mut String, body: &str) {
    let fence = "`".repeat(fence_width(body));
    out.push_str(&fence);
    out.push_str("text\n");
    out.push_str(body);
    if !body.ends_with('\n') {
        out.push('\n');
    }
    out.push_str(&fence);
    out.push('\n');
}

fn fence_width(body: &str) -> usize {
    let mut longest = 0usize;
    let mut run = 0usize;
    for ch in body.chars() {
        if ch == '`' {
            run += 1;
            longest = longest.max(run);
        } else {
            run = 0;
        }
    }
    longest.max(2) + 1
}
