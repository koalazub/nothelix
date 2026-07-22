#![allow(clippy::needless_pass_by_value)]

use crate::error::{Error, Result, ffi};
use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

fn allowlist_path() -> PathBuf {
    std::env::var("HOME")
        .map_or_else(|_| PathBuf::from("/tmp"), PathBuf::from)
        .join(".local/share/nothelix/trusted-dirs")
}

fn resolved(dir: &str) -> Result<String> {
    match fs::canonicalize(Path::new(dir)) {
        Ok(path) => Ok(path.to_string_lossy().into_owned()),
        Err(e) if e.kind() == ErrorKind::NotFound => Err(Error::absent(dir)),
        Err(e) => Err(Error::resolving(dir, e)),
    }
}

struct Allowlist {
    path: PathBuf,
    entries: Vec<String>,
}

impl Allowlist {
    fn load(path: &Path) -> Result<Self> {
        let entries = match fs::read_to_string(path) {
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
            entries,
        })
    }

    fn holds(&self, canonical: &str) -> bool {
        self.entries.iter().any(|entry| entry == canonical)
    }

    fn admit(&mut self, dir: &str) -> Result<()> {
        let canonical = resolved(dir)?;
        if !self.holds(&canonical) {
            self.entries.push(canonical);
        }
        self.store()
    }

    fn revoke(&mut self, dir: &str) -> Result<()> {
        let canonical = resolved(dir).ok();
        self.entries
            .retain(|entry| canonical.as_deref() != Some(entry.as_str()) && entry != dir);
        self.store()
    }

    fn store(&self) -> Result<()> {
        let parent = self
            .path
            .parent()
            .ok_or_else(|| Error::orphan(&self.path))?;
        fs::create_dir_all(parent).map_err(|e| Error::creating(parent, e))?;
        fs::write(&self.path, self.entries.join("\n")).map_err(|e| Error::writing(&self.path, e))
    }
}

fn is_trusted(path: &Path, dir: &str) -> bool {
    match (resolved(dir), Allowlist::load(path)) {
        (Ok(canonical), Ok(list)) => list.holds(&canonical),
        _ => false,
    }
}

fn add_to(path: &Path, dir: &str) -> Result<()> {
    Allowlist::load(path)?.admit(dir)
}

fn remove_from(path: &Path, dir: &str) -> Result<()> {
    Allowlist::load(path)?.revoke(dir)
}

pub fn trust_list() -> String {
    ffi(Allowlist::load(&allowlist_path()).map(|list| list.entries.join("\n")))
}

pub fn trust_contains(dir: String) -> String {
    if is_trusted(&allowlist_path(), &dir) {
        "yes".into()
    } else {
        "no".into()
    }
}

pub fn trust_add(dir: String) -> String {
    ffi(add_to(&allowlist_path(), &dir).map(|()| String::new()))
}

pub fn trust_remove(dir: String) -> String {
    ffi(remove_from(&allowlist_path(), &dir).map(|()| String::new()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn entries_at(path: &Path) -> Vec<String> {
        Allowlist::load(path).unwrap().entries
    }

    #[test]
    fn add_contains_remove_roundtrip() {
        let store = tempdir().unwrap();
        let list_path = store.path().join("sub/trusted-dirs");
        let proj = tempdir().unwrap();
        let proj_str = proj.path().to_string_lossy().into_owned();

        assert!(!is_trusted(&list_path, &proj_str));
        add_to(&list_path, &proj_str).unwrap();
        assert!(is_trusted(&list_path, &proj_str));

        add_to(&list_path, &proj_str).unwrap();
        assert_eq!(entries_at(&list_path).len(), 1);

        remove_from(&list_path, &proj_str).unwrap();
        assert!(!is_trusted(&list_path, &proj_str));
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
        add_to(&list_path, &proj_canon).unwrap();

        let bypass = format!("{}/..", sub.to_string_lossy());
        assert!(
            is_trusted(&list_path, &bypass),
            "a dot-dot path must canonicalize to the trusted dir"
        );
    }

    #[test]
    fn nonexistent_dir_is_never_trusted() {
        let store = tempdir().unwrap();
        let list_path = store.path().join("trusted-dirs");
        assert!(!is_trusted(&list_path, "/no/such/dir/anywhere"));
        assert!(add_to(&list_path, "/no/such/dir/anywhere").is_err());
    }

    #[test]
    fn refusing_an_absent_dir_names_it() {
        let store = tempdir().unwrap();
        let list_path = store.path().join("trusted-dirs");
        let failure = add_to(&list_path, "/no/such/dir/anywhere").unwrap_err();
        assert!(failure.to_string().contains("/no/such/dir/anywhere"));
    }

    #[test]
    fn revoking_a_deleted_project_still_drops_its_raw_entry() {
        let store = tempdir().unwrap();
        let list_path = store.path().join("trusted-dirs");
        fs::write(&list_path, "/gone/project\n").unwrap();
        remove_from(&list_path, "/gone/project").unwrap();
        assert!(entries_at(&list_path).is_empty());
    }
}
