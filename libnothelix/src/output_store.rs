#![allow(clippy::needless_pass_by_value)]

use crate::error::{Error, Result, ffi};
use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

fn store_root() -> PathBuf {
    std::env::var("HOME")
        .map_or_else(|_| PathBuf::from("/tmp"), PathBuf::from)
        .join(".local/share/nothelix/outputs")
}

fn path_safe(key: &str) -> String {
    key.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

struct CellRecord {
    path: PathBuf,
}

impl CellRecord {
    fn locate(root: &Path, workspace: &str, cell_id: &str) -> Self {
        Self {
            path: root
                .join(path_safe(workspace))
                .join(format!("{}.json", path_safe(cell_id))),
        }
    }

    fn read(&self) -> Result<Option<String>> {
        match fs::read_to_string(&self.path) {
            Ok(stored) => Ok(stored
                .split_once('\n')
                .map(|(source_hash, outputs_json)| format!("{source_hash}\t{outputs_json}"))),
            Err(e) if e.kind() == ErrorKind::NotFound => Ok(None),
            Err(e) => Err(Error::reading(&self.path, e)),
        }
    }

    fn write(&self, source_hash: &str, outputs_json: &str) -> Result<()> {
        let parent = self
            .path
            .parent()
            .ok_or_else(|| Error::orphan(&self.path))?;
        fs::create_dir_all(parent).map_err(|e| Error::creating(parent, e))?;
        fs::write(&self.path, format!("{source_hash}\n{outputs_json}"))
            .map_err(|e| Error::writing(&self.path, e))
    }

    fn discard(&self) -> Result<()> {
        match fs::remove_file(&self.path) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == ErrorKind::NotFound => Ok(()),
            Err(e) => Err(Error::removing(&self.path, e)),
        }
    }
}

pub fn output_store_put(
    workspace: String,
    cell_id: String,
    source_hash: String,
    outputs_json: String,
) -> String {
    ffi(CellRecord::locate(&store_root(), &workspace, &cell_id)
        .write(&source_hash, &outputs_json)
        .map(|()| String::new()))
}

pub fn output_store_get(workspace: String, cell_id: String) -> String {
    ffi(CellRecord::locate(&store_root(), &workspace, &cell_id)
        .read()
        .map(Option::unwrap_or_default))
}

pub fn output_store_clear(workspace: String, cell_id: String) -> String {
    ffi(CellRecord::locate(&store_root(), &workspace, &cell_id)
        .discard()
        .map(|()| String::new()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn get_at(root: &Path, workspace: &str, cell_id: &str) -> String {
        CellRecord::locate(root, workspace, cell_id)
            .read()
            .unwrap()
            .unwrap_or_default()
    }

    fn set_at(root: &Path, workspace: &str, cell_id: &str, source_hash: &str, outputs_json: &str) {
        CellRecord::locate(root, workspace, cell_id)
            .write(source_hash, outputs_json)
            .unwrap();
    }

    #[test]
    fn put_get_roundtrip() {
        let root = tempdir().unwrap();
        let dir = root.path().to_path_buf();
        assert_eq!(get_at(&dir, "ws", "cell-1"), "");
        set_at(&dir, "ws", "cell-1", "h1", "[{\"a\":1}]");
        assert_eq!(get_at(&dir, "ws", "cell-1"), "h1\t[{\"a\":1}]");
    }

    #[test]
    fn put_overwrites_in_place() {
        let root = tempdir().unwrap();
        let dir = root.path().to_path_buf();
        set_at(&dir, "ws", "c", "h1", "[1]");
        set_at(&dir, "ws", "c", "h2", "[2]");
        assert_eq!(get_at(&dir, "ws", "c"), "h2\t[2]");
    }

    #[test]
    fn distinct_cells_and_workspaces_isolate() {
        let root = tempdir().unwrap();
        let dir = root.path().to_path_buf();
        set_at(&dir, "wsA", "c", "h", "[1]");
        set_at(&dir, "wsB", "c", "h", "[2]");
        assert_eq!(get_at(&dir, "wsA", "c"), "h\t[1]");
        assert_eq!(get_at(&dir, "wsB", "c"), "h\t[2]");
    }

    #[test]
    fn clear_removes_entry() {
        let root = tempdir().unwrap();
        let dir = root.path().to_path_buf();
        set_at(&dir, "ws", "c", "h", "[1]");
        CellRecord::locate(&dir, "ws", "c").discard().unwrap();
        assert_eq!(get_at(&dir, "ws", "c"), "");
    }

    #[test]
    fn clear_of_absent_entry_succeeds() {
        let root = tempdir().unwrap();
        CellRecord::locate(root.path(), "ws", "never-written")
            .discard()
            .unwrap();
    }

    #[test]
    fn sanitizes_path_separators_in_keys() {
        let root = tempdir().unwrap();
        let dir = root.path().to_path_buf();
        set_at(&dir, "/abs/ws/../x", "a/b", "h", "[1]");
        assert_eq!(get_at(&dir, "/abs/ws/../x", "a/b"), "h\t[1]");
    }

    #[test]
    fn missing_entry_returns_empty() {
        let root = tempdir().unwrap();
        assert_eq!(get_at(root.path(), "ws", "nope"), "");
    }

    #[test]
    fn unreadable_record_reports_the_path_instead_of_an_empty_hit() {
        let root = tempdir().unwrap();
        let occupied = root.path().join("ws").join("c.json");
        fs::create_dir_all(&occupied).unwrap();
        let failure = CellRecord::locate(root.path(), "ws", "c")
            .read()
            .unwrap_err();
        assert!(failure.to_string().contains("c.json"), "{failure}");
    }
}
