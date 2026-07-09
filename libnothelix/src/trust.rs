// Steel's `register_fn` requires owned argument types; see kernel.rs.
#![allow(clippy::needless_pass_by_value)]

//! Trust allowlist for per-project *executable* settings (`julia-bin`,
//! `julia-project` in `.nothelix.conf`).
//!
//! A project directory must be added here, by explicit user action, before
//! nothelix will launch a kernel with that project's configured interpreter or
//! environment. Without this gate, merely opening a downloaded notebook could
//! run an attacker-controlled binary. Stored paths are canonicalized so `..`
//! and symlink variants cannot slip a malicious directory past the check.
//!
//! The list lives at `~/.local/share/nothelix/trusted-dirs`, one canonical
//! absolute path per line.

use std::fs;
use std::path::{Path, PathBuf};

fn home_dir() -> PathBuf {
    std::env::var("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("/tmp"))
}

fn trusted_dirs_path() -> PathBuf {
    home_dir().join(".local/share/nothelix/trusted-dirs")
}

/// Canonical absolute form of `dir`, or None if it cannot be resolved
/// (e.g. the directory does not exist — which must never be trusted).
fn canonical(dir: &str) -> Option<String> {
    fs::canonicalize(Path::new(dir))
        .ok()
        .map(|p| p.to_string_lossy().into_owned())
}

fn list_at(path: &Path) -> Vec<String> {
    match fs::read_to_string(path) {
        Ok(s) => s
            .lines()
            .map(str::trim)
            .filter(|l| !l.is_empty())
            .map(str::to_string)
            .collect(),
        Err(_) => Vec::new(),
    }
}

fn write_at(path: &Path, lines: &[String]) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| format!("ERROR: cannot create {}: {e}", parent.display()))?;
    }
    fs::write(path, lines.join("\n"))
        .map_err(|e| format!("ERROR: cannot write {}: {e}", path.display()))
}

fn contains_at(path: &Path, dir: &str) -> bool {
    match canonical(dir) {
        Some(c) => list_at(path).iter().any(|l| l == &c),
        None => false,
    }
}

fn add_at(path: &Path, dir: &str) -> Result<(), String> {
    let c = canonical(dir).ok_or_else(|| format!("ERROR: directory does not exist: {dir}"))?;
    let mut lines = list_at(path);
    if !lines.iter().any(|l| l == &c) {
        lines.push(c);
    }
    write_at(path, &lines)
}

fn remove_at(path: &Path, dir: &str) -> Result<(), String> {
    // Match on the canonical form, but also tolerate the raw string so a
    // since-deleted project (canonicalize fails) can still be revoked.
    let canon = canonical(dir);
    let lines: Vec<String> = list_at(path)
        .into_iter()
        .filter(|l| canon.as_deref() != Some(l.as_str()) && l.as_str() != dir)
        .collect();
    write_at(path, &lines)
}

// ─── FFI surface ────────────────────────────────────────────────────────────

/// Newline-joined list of trusted directories ("" if none).
pub fn trust_list() -> String {
    list_at(&trusted_dirs_path()).join("\n")
}

/// "yes" if `dir` (canonicalized) is trusted, else "no".
pub fn trust_contains(dir: String) -> String {
    if contains_at(&trusted_dirs_path(), &dir) {
        "yes".into()
    } else {
        "no".into()
    }
}

/// Add `dir` to the allowlist. "" on success, "ERROR: …" otherwise.
pub fn trust_add(dir: String) -> String {
    match add_at(&trusted_dirs_path(), &dir) {
        Ok(()) => String::new(),
        Err(e) => e,
    }
}

/// Remove `dir` from the allowlist. "" on success, "ERROR: …" otherwise.
pub fn trust_remove(dir: String) -> String {
    match remove_at(&trusted_dirs_path(), &dir) {
        Ok(()) => String::new(),
        Err(e) => e,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn add_contains_remove_roundtrip() {
        let store = tempdir().unwrap();
        let list_path = store.path().join("sub/trusted-dirs"); // also exercises mkdir
        let proj = tempdir().unwrap();
        let proj_str = proj.path().to_string_lossy().into_owned();

        assert!(!contains_at(&list_path, &proj_str));
        add_at(&list_path, &proj_str).unwrap();
        assert!(contains_at(&list_path, &proj_str));

        // idempotent — adding again does not duplicate.
        add_at(&list_path, &proj_str).unwrap();
        assert_eq!(list_at(&list_path).len(), 1);

        remove_at(&list_path, &proj_str).unwrap();
        assert!(!contains_at(&list_path, &proj_str));
    }

    #[test]
    fn canonicalizes_away_dot_dot_bypass() {
        let store = tempdir().unwrap();
        let list_path = store.path().join("trusted-dirs");
        let proj = tempdir().unwrap();
        let sub = proj.path().join("inner");
        fs::create_dir_all(&sub).unwrap();

        let proj_canon = fs::canonicalize(proj.path())
            .unwrap()
            .to_string_lossy()
            .into_owned();
        add_at(&list_path, &proj_canon).unwrap();

        // ".../inner/.." resolves to the trusted project dir.
        let bypass = format!("{}/..", sub.to_string_lossy());
        assert!(
            contains_at(&list_path, &bypass),
            "a dot-dot path must canonicalize to the trusted dir"
        );
    }

    #[test]
    fn nonexistent_dir_is_never_trusted() {
        let store = tempdir().unwrap();
        let list_path = store.path().join("trusted-dirs");
        assert!(!contains_at(&list_path, "/no/such/dir/anywhere"));
        assert!(add_at(&list_path, "/no/such/dir/anywhere").is_err());
    }
}
