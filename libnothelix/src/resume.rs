#![allow(clippy::needless_pass_by_value)]

//! Per-notebook resume position store.
//!
//! One line per notebook at `~/.local/share/nothelix/resume`:
//! `<canonical-abs-path>\t<cell-ordinal>\t<line-offset>\t<column>`. Keyed by
//! the canonical path so `./a.jl` and its absolute form are one entry. A
//! missing file, missing entry, or malformed line yields the empty string —
//! resume is best-effort and never blocks opening a notebook.

use std::fs;
use std::path::{Path, PathBuf};

fn home_dir() -> PathBuf {
    std::env::var("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("/tmp"))
}

fn resume_file_path() -> PathBuf {
    home_dir().join(".local/share/nothelix/resume")
}

fn canonical(path: &str) -> Option<String> {
    fs::canonicalize(Path::new(path))
        .ok()
        .map(|p| p.to_string_lossy().into_owned())
}

fn list_lines(path: &Path) -> Vec<String> {
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

fn write_lines(path: &Path, lines: &[String]) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| format!("ERROR: cannot create {}: {e}", parent.display()))?;
    }
    fs::write(path, lines.join("\n"))
        .map_err(|e| format!("ERROR: cannot write {}: {e}", path.display()))
}

fn get_at(path: &Path, nb: &str) -> String {
    let key = match canonical(nb) {
        Some(k) => k,
        None => return String::new(),
    };
    for line in list_lines(path) {
        let mut parts = line.splitn(4, '\t');
        let stored_path = parts.next().unwrap_or("");
        let ord = parts.next();
        let off = parts.next();
        let col = parts.next();
        if stored_path == key
            && let (Some(o), Some(f), Some(c)) = (ord, off, col)
        {
            return format!("{o}\t{f}\t{c}");
        }
    }
    String::new()
}

fn set_at(path: &Path, nb: &str, ord: isize, off: isize, col: isize) -> Result<(), String> {
    let key = canonical(nb).unwrap_or_else(|| nb.to_string());
    let entry = format!("{key}\t{ord}\t{off}\t{col}");
    let mut lines: Vec<String> = list_lines(path)
        .into_iter()
        .filter(|l| l.split('\t').next() != Some(key.as_str()))
        .collect();
    lines.push(entry);
    write_lines(path, &lines)
}

pub fn resume_get(path: String) -> String {
    get_at(&resume_file_path(), &path)
}

pub fn resume_set(path: String, ord: isize, off: isize, col: isize) -> String {
    match set_at(&resume_file_path(), &path, ord, off, col) {
        Ok(()) => String::new(),
        Err(e) => e,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn set_get_roundtrip() {
        let store = tempdir().unwrap();
        let path = store.path().join("sub/resume");
        let nb = tempdir().unwrap();
        let nb_file = nb.path().join("a.jl");
        std::fs::write(&nb_file, "@cell 0 :julia\n").unwrap();
        let nb_str = nb_file.to_string_lossy().into_owned();

        assert_eq!(get_at(&path, &nb_str), "");
        set_at(&path, &nb_str, 42, 3, 5).unwrap();
        assert_eq!(get_at(&path, &nb_str), "42\t3\t5");
    }

    #[test]
    fn set_updates_in_place() {
        let store = tempdir().unwrap();
        let path = store.path().join("resume");
        let nb = tempdir().unwrap();
        let nb_file = nb.path().join("a.jl");
        std::fs::write(&nb_file, "x\n").unwrap();
        let nb_str = nb_file.to_string_lossy().into_owned();

        set_at(&path, &nb_str, 1, 1, 1).unwrap();
        set_at(&path, &nb_str, 9, 8, 7).unwrap();
        assert_eq!(get_at(&path, &nb_str), "9\t8\t7");
        assert_eq!(list_lines(&path).len(), 1);
    }

    #[test]
    fn distinct_notebooks_are_separate_lines() {
        let store = tempdir().unwrap();
        let path = store.path().join("resume");
        let nb = tempdir().unwrap();
        let a = nb.path().join("a.jl");
        let b = nb.path().join("b.jl");
        std::fs::write(&a, "x\n").unwrap();
        std::fs::write(&b, "x\n").unwrap();
        let a_str = a.to_string_lossy().into_owned();
        let b_str = b.to_string_lossy().into_owned();

        set_at(&path, &a_str, 1, 0, 0).unwrap();
        set_at(&path, &b_str, 2, 0, 0).unwrap();
        assert_eq!(get_at(&path, &a_str), "1\t0\t0");
        assert_eq!(get_at(&path, &b_str), "2\t0\t0");
    }

    #[test]
    fn canonicalizes_key() {
        let store = tempdir().unwrap();
        let path = store.path().join("resume");
        let nb = tempdir().unwrap();
        let sub = nb.path().join("inner");
        std::fs::create_dir_all(&sub).unwrap();
        let file = sub.join("a.jl");
        std::fs::write(&file, "x\n").unwrap();

        let canon = file.to_string_lossy().into_owned();
        set_at(&path, &canon, 7, 0, 0).unwrap();

        let dotted = format!("{}/../inner/a.jl", sub.to_string_lossy());
        assert_eq!(get_at(&path, &dotted), "7\t0\t0");
    }

    #[test]
    fn missing_file_and_entry_return_empty() {
        let store = tempdir().unwrap();
        let path = store.path().join("resume");
        assert_eq!(get_at(&path, "/no/such/file.jl"), "");
    }

    #[test]
    fn malformed_line_is_skipped() {
        let store = tempdir().unwrap();
        let path = store.path().join("resume");
        std::fs::create_dir_all(store.path()).unwrap();
        std::fs::write(&path, "garbage-without-tabs\n").unwrap();
        assert_eq!(get_at(&path, "/anything.jl"), "");
    }
}
