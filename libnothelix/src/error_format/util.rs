//! Cross-cutting helpers for the error formatter.
//!
//! Path cleaning, call-chain trimming, message tidying, and the
//! truncate/wrap utilities used by every renderer + enricher.

use super::types::ErrorFrame;

// ─── Message tidying ─────────────────────────────────────────────────────────

pub(super) fn clean_message(msg: &str) -> &str {
    let msg = msg
        .find("\nClosest candidates")
        .or_else(|| msg.find("\n\nClosest candidates"))
        .or_else(|| msg.find("Closest candidates are:"))
        .map_or(msg, |idx| msg[..idx].trim_end());
    msg.find("\nStacktrace:")
        .map_or(msg, |idx| msg[..idx].trim_end())
}

pub(super) fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        return s.to_string();
    }
    let mut end = max.min(s.len());
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}…", &s[..end])
}

pub(super) fn wrap(text: &str, width: usize) -> Vec<String> {
    if text.len() <= width {
        return vec![text.to_string()];
    }
    let mut lines = Vec::new();
    let mut cur = String::new();
    for word in text.split_whitespace() {
        if cur.is_empty() {
            cur = word.to_string();
        } else if cur.len() + 1 + word.len() > width {
            lines.push(cur);
            cur = word.to_string();
        } else {
            cur.push(' ');
            cur.push_str(word);
        }
    }
    if !cur.is_empty() {
        lines.push(cur);
    }
    lines
}

// ─── Path cleaning ───────────────────────────────────────────────────────────

pub(super) fn clean_path(path: &str) -> String {
    if path == "none" || path == "<cell>" || path.starts_with("REPL") {
        return path.to_string();
    }
    if path.contains("/nix/store/") {
        if let Some(idx) = path.find("/stdlib/") {
            let after = &path[idx + 8..];
            let cleaned = match after.find('/') {
                Some(slash) => &after[slash + 1..],
                None => after,
            };
            return format!("stdlib:{cleaned}");
        }
        if let Some(idx) = path.find("/share/julia/") {
            return path[idx + 13..].to_string();
        }
        if let Some(rest) = path.strip_prefix("/nix/store/")
            && rest.len() > 33
            && rest.as_bytes()[32] == b'-'
        {
            return rest[33..].to_string();
        }
    }
    if let Some(idx) = path.find("/.julia/packages/") {
        let after = &path[idx + 17..];
        let parts: Vec<&str> = after.splitn(3, '/').collect();
        if parts.len() == 3 {
            return format!("{}/{}", parts[0], parts[2]);
        }
    }
    let components: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
    if components.len() > 3 {
        return format!("…/{}", components[components.len() - 3..].join("/"));
    }
    path.to_string()
}

// ─── Call chain ──────────────────────────────────────────────────────────────

pub(super) fn build_call_chain(frames: &[ErrorFrame]) -> Vec<String> {
    if frames.is_empty() {
        return vec![];
    }
    let mut chain: Vec<String> = Vec::new();
    let mut collapsed = 0u32;

    for frame in frames {
        if matches!(
            frame.func.as_str(),
            "top-level scope" | "eval" | "include_string" | "exec_options" | "_start"
        ) || frame.func.starts_with('#')
        {
            continue;
        }
        if frame.is_user_code {
            if collapsed > 0 {
                chain.push(format!("[{collapsed} stdlib frames]"));
                collapsed = 0;
            }
            if frame.line > 0 {
                chain.push(format!("{}() at cell line {}", frame.func, frame.line));
            } else {
                chain.push(format!("{}()", frame.func));
            }
        } else {
            let path = clean_path(&frame.file);
            if chain.is_empty() || collapsed == 0 {
                if frame.line > 0 {
                    chain.push(format!("{}() at {}:{}", frame.func, path, frame.line));
                } else {
                    chain.push(format!("{}() at {}", frame.func, path));
                }
            }
            collapsed += 1;
        }
    }
    if collapsed > 1 {
        chain.push(format!("[{} more stdlib frames]", collapsed - 1));
    }
    chain.truncate(6);
    chain
}
