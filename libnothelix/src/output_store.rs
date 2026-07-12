#![allow(clippy::needless_pass_by_value)]

//! Per-cell notebook output store — the out-of-buffer source of truth for
//! cell output. One file per cell at
//! `~/.local/share/nothelix/outputs/<workspace>/<cell_id>.json`, holding
//! `<source_hash>\n<outputs_json>` (the nbformat outputs array captured
//! against `source_hash`). Best-effort: read/write failures never block
//! execution or opening a notebook.

use std::fs;
use std::path::{Path, PathBuf};

fn home_dir() -> PathBuf {
    std::env::var("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("/tmp"))
}

fn store_root() -> PathBuf {
    home_dir().join(".local/share/nothelix/outputs")
}

fn sanitize(key: &str) -> String {
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

fn cell_path(root: &Path, workspace: &str, cell_id: &str) -> PathBuf {
    root.join(sanitize(workspace))
        .join(format!("{}.json", sanitize(cell_id)))
}

fn get_at(root: &Path, workspace: &str, cell_id: &str) -> String {
    match fs::read_to_string(cell_path(root, workspace, cell_id)) {
        Ok(s) => match s.split_once('\n') {
            Some((hash, json)) => format!("{hash}\t{json}"),
            None => String::new(),
        },
        Err(_) => String::new(),
    }
}

fn set_at(
    root: &Path,
    workspace: &str,
    cell_id: &str,
    source_hash: &str,
    outputs_json: &str,
) -> Result<(), String> {
    let path = cell_path(root, workspace, cell_id);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| format!("ERROR: cannot create {}: {e}", parent.display()))?;
    }
    fs::write(&path, format!("{source_hash}\n{outputs_json}"))
        .map_err(|e| format!("ERROR: cannot write {}: {e}", path.display()))
}

fn clear_at(root: &Path, workspace: &str, cell_id: &str) -> Result<(), String> {
    match fs::remove_file(cell_path(root, workspace, cell_id)) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(format!("ERROR: cannot remove: {e}")),
    }
}

pub fn output_store_put(
    workspace: String,
    cell_id: String,
    source_hash: String,
    outputs_json: String,
) -> String {
    match set_at(
        &store_root(),
        &workspace,
        &cell_id,
        &source_hash,
        &outputs_json,
    ) {
        Ok(()) => String::new(),
        Err(e) => e,
    }
}

pub fn output_store_get(workspace: String, cell_id: String) -> String {
    get_at(&store_root(), &workspace, &cell_id)
}

pub fn output_store_clear(workspace: String, cell_id: String) -> String {
    match clear_at(&store_root(), &workspace, &cell_id) {
        Ok(()) => String::new(),
        Err(e) => e,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn put_get_roundtrip() {
        let root = tempdir().unwrap();
        let dir = root.path().to_path_buf();
        assert_eq!(get_at(&dir, "ws", "cell-1"), "");
        set_at(&dir, "ws", "cell-1", "h1", "[{\"a\":1}]").unwrap();
        assert_eq!(get_at(&dir, "ws", "cell-1"), "h1\t[{\"a\":1}]");
    }

    #[test]
    fn put_overwrites_in_place() {
        let root = tempdir().unwrap();
        let dir = root.path().to_path_buf();
        set_at(&dir, "ws", "c", "h1", "[1]").unwrap();
        set_at(&dir, "ws", "c", "h2", "[2]").unwrap();
        assert_eq!(get_at(&dir, "ws", "c"), "h2\t[2]");
    }

    #[test]
    fn distinct_cells_and_workspaces_isolate() {
        let root = tempdir().unwrap();
        let dir = root.path().to_path_buf();
        set_at(&dir, "wsA", "c", "h", "[1]").unwrap();
        set_at(&dir, "wsB", "c", "h", "[2]").unwrap();
        assert_eq!(get_at(&dir, "wsA", "c"), "h\t[1]");
        assert_eq!(get_at(&dir, "wsB", "c"), "h\t[2]");
    }

    #[test]
    fn clear_removes_entry() {
        let root = tempdir().unwrap();
        let dir = root.path().to_path_buf();
        set_at(&dir, "ws", "c", "h", "[1]").unwrap();
        clear_at(&dir, "ws", "c").unwrap();
        assert_eq!(get_at(&dir, "ws", "c"), "");
    }

    #[test]
    fn sanitizes_path_separators_in_keys() {
        let root = tempdir().unwrap();
        let dir = root.path().to_path_buf();
        set_at(&dir, "/abs/ws/../x", "a/b", "h", "[1]").unwrap();
        assert_eq!(get_at(&dir, "/abs/ws/../x", "a/b"), "h\t[1]");
    }

    #[test]
    fn missing_entry_returns_empty() {
        let root = tempdir().unwrap();
        assert_eq!(get_at(root.path(), "ws", "nope"), "");
    }
}
