#![allow(clippy::needless_pass_by_value)]

use crate::error::{Error, Result, ffi};
use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

fn resume_file_path() -> PathBuf {
    std::env::var("HOME")
        .map_or_else(|_| PathBuf::from("/tmp"), PathBuf::from)
        .join(".local/share/nothelix/resume")
}

fn canonical(notebook: &str) -> Option<String> {
    fs::canonicalize(Path::new(notebook))
        .ok()
        .map(|p| p.to_string_lossy().into_owned())
}

struct Anchor {
    ordinal: isize,
    offset: isize,
    column: isize,
}

impl std::fmt::Display for Anchor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}\t{}\t{}", self.ordinal, self.offset, self.column)
    }
}

struct Positions {
    path: PathBuf,
    lines: Vec<String>,
}

impl Positions {
    fn load(path: &Path) -> Result<Self> {
        let lines = match fs::read_to_string(path) {
            Ok(text) => text
                .lines()
                .map(str::trim)
                .filter(|line| !line.is_empty())
                .map(str::to_string)
                .collect(),
            Err(e) if e.kind() == ErrorKind::NotFound => Vec::new(),
            Err(e) => return Err(Error::reading(path, e)),
        };
        Ok(Self {
            path: path.to_path_buf(),
            lines,
        })
    }

    fn anchor_of(&self, key: &str) -> Option<String> {
        self.lines.iter().find_map(|line| {
            let mut fields = line.splitn(4, '\t');
            if fields.next()? != key {
                return None;
            }
            let ordinal = fields.next()?;
            let offset = fields.next()?;
            let column = fields.next()?;
            Some(format!("{ordinal}\t{offset}\t{column}"))
        })
    }

    fn record(&mut self, key: &str, anchor: &Anchor) -> Result<()> {
        self.lines
            .retain(|line| line.split('\t').next() != Some(key));
        self.lines.push(format!("{key}\t{anchor}"));
        self.store()
    }

    fn store(&self) -> Result<()> {
        let parent = self
            .path
            .parent()
            .ok_or_else(|| Error::orphan(&self.path))?;
        fs::create_dir_all(parent).map_err(|e| Error::creating(parent, e))?;
        fs::write(&self.path, self.lines.join("\n")).map_err(|e| Error::writing(&self.path, e))
    }
}

fn anchor_at(path: &Path, notebook: &str) -> Result<String> {
    let Some(key) = canonical(notebook) else {
        return Ok(String::new());
    };
    Ok(Positions::load(path)?.anchor_of(&key).unwrap_or_default())
}

fn record_at(path: &Path, notebook: &str, anchor: &Anchor) -> Result<()> {
    let key = canonical(notebook).unwrap_or_else(|| notebook.to_string());
    Positions::load(path)?.record(&key, anchor)
}

pub fn resume_get(path: String) -> String {
    ffi(anchor_at(&resume_file_path(), &path))
}

pub fn resume_set(path: String, ord: isize, off: isize, col: isize) -> String {
    let anchor = Anchor {
        ordinal: ord,
        offset: off,
        column: col,
    };
    ffi(record_at(&resume_file_path(), &path, &anchor).map(|()| String::new()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn get_at(path: &Path, notebook: &str) -> String {
        anchor_at(path, notebook).unwrap()
    }

    fn set_at(path: &Path, notebook: &str, ordinal: isize, offset: isize, column: isize) {
        record_at(
            path,
            notebook,
            &Anchor {
                ordinal,
                offset,
                column,
            },
        )
        .unwrap();
    }

    fn stored_lines(path: &Path) -> Vec<String> {
        Positions::load(path).unwrap().lines
    }

    #[test]
    fn set_get_roundtrip() {
        let store = tempdir().unwrap();
        let path = store.path().join("sub/resume");
        let nb = tempdir().unwrap();
        let nb_file = nb.path().join("a.jl");
        std::fs::write(&nb_file, "@cell 0 :julia\n").unwrap();
        let nb_str = nb_file.to_string_lossy().into_owned();

        assert_eq!(get_at(&path, &nb_str), "");
        set_at(&path, &nb_str, 42, 3, 5);
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

        set_at(&path, &nb_str, 1, 1, 1);
        set_at(&path, &nb_str, 9, 8, 7);
        assert_eq!(get_at(&path, &nb_str), "9\t8\t7");
        assert_eq!(stored_lines(&path).len(), 1);
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

        set_at(&path, &a_str, 1, 0, 0);
        set_at(&path, &b_str, 2, 0, 0);
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
        set_at(&path, &canon, 7, 0, 0);

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

    #[test]
    fn unreadable_store_is_reported_instead_of_silently_emptied() {
        let store = tempdir().unwrap();
        let path = store.path().join("resume");
        std::fs::create_dir_all(&path).unwrap();
        assert!(Positions::load(&path).is_err());
    }
}
