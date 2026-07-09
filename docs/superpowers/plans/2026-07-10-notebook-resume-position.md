# Notebook Resume Position Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** When a notebook is reopened, land the cursor back on the exact spot inside the cell the user last worked in, persisted across editor restarts.

**Architecture:** Persist the cell-relative cursor anchor (`cell-ordinal`, `line-offset`, `column`) that `cursor-restore.scm` already computes into a single global file `~/.local/share/nothelix/resume`, mirroring the existing `trust.rs` allowlist store. A tiny Rust module (`resume.rs`) owns the file (it gets `create_dir_all` and canonical path keys for free); a Steel module (`resume.scm`) captures the anchor on save and cell-run and restores it on open, reusing `cursor-restore.scm`'s anchor math.

**Tech Stack:** Rust (libnothelix FFI dylib), Steel (Helix plugin), `cargo nextest`, `just install`, `just check`.

## Global Constraints

- No new files in the user's project tree. State lives only in `~/.local/share/nothelix/`.
- No code comments in new/edited code — code must be self-documenting (project rule).
- Use `cargo nextest`, not `cargo test`. Clippy must pass clean (`-D warnings`).
- After any Rust FFI change, run `just install` — a stale dylib fails the FFI handshake.
- FFI version is handshaked in two files that MUST stay equal: `NOTHELIX_FFI_VERSION` in `libnothelix/src/lib.rs` and `EXPECTED-FFI-VERSION` in `plugin/nothelix/ffi-version.scm`. Current value: `18`. This feature bumps both to `19`.
- Line-based storage format only (no JSON/s-expr) — matches `trust.rs` / `project-config.scm` robustness rationale.
- Steel modules are verified by the headless plugin-load gate in `just check`, not unit tests.

---

### Task 1: `resume.rs` Rust store + FFI registration + version bump

**Files:**
- Create: `libnothelix/src/resume.rs`
- Modify: `libnothelix/src/lib.rs` (module decl ~line 47, register block ~line 197, version const line 70)
- Modify: `plugin/nothelix/ffi-version.scm:11`

**Interfaces:**
- Produces (Rust): `resume::resume_get(path: String) -> String`, `resume::resume_set(path: String, ord: isize, off: isize, col: isize) -> String`.
- Produces (FFI names for Steel): `resume-get`, `resume-set`.
- Storage line format: `<canonical-abs-path>\t<ord>\t<off>\t<col>`, one notebook per line. `resume-get` returns `"<ord>\t<off>\t<col>"` or `""`. `resume-set` returns `""` on success or `"ERROR: …"`.

- [ ] **Step 1: Write the failing tests**

Create `libnothelix/src/resume.rs` with only the test module and the `*_at` seams' signatures referenced, so tests fail to compile first. Full test block:

```rust
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
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo nextest run -p libnothelix resume`
Expected: FAIL — `get_at`/`set_at`/`list_lines` not found (compile error).

- [ ] **Step 3: Write the implementation**

Prepend to `libnothelix/src/resume.rs` (above the test module):

```rust
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
        if stored_path == key {
            if let (Some(o), Some(f), Some(c)) = (ord, off, col) {
                return format!("{o}\t{f}\t{c}");
            }
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
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo nextest run -p libnothelix resume`
Expected: PASS (6 tests).

- [ ] **Step 5: Register the FFI functions, declare the module, bump the version**

In `libnothelix/src/lib.rs`, add the module declaration next to `trust` (after line 47 `mod trust;`):

```rust
#[cfg(feature = "native")]
mod resume;
```

In the register block, after the four `nothelix-trust-*` lines (~line 200):

```rust
    m.register_fn("resume-get", resume::resume_get);
    m.register_fn("resume-set", resume::resume_set);
```

Change the version const (line 70):

```rust
pub const NOTHELIX_FFI_VERSION: u32 = 19;
```

In `plugin/nothelix/ffi-version.scm`, line 11:

```scheme
(define EXPECTED-FFI-VERSION 19)
```

- [ ] **Step 6: Verify no other hardcoded `18` version references**

Run: `rg -n "FFI_VERSION|EXPECTED-FFI-VERSION" libnothelix plugin`
Expected: only the two lines above show `19`; no stray `18`. If a test asserts the constant, update it to `19`.

- [ ] **Step 7: Clippy + build the dylib**

Run: `cargo clippy -p libnothelix --all-targets -- -D warnings`
Expected: clean.

- [ ] **Step 8: Install the rebuilt dylib**

Ask the user to run `just install` (release builds fail under the sandbox). Do not proceed to Steel plugin-load checks until they confirm the dylib is installed at v19.

- [ ] **Step 9: Commit**

```bash
jj describe @ -m "feat(resume): global per-notebook resume-position store (FFI v19)

Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>"
```

---

### Task 2: Expose reusable anchor primitives in `cursor-restore.scm`

**Files:**
- Modify: `plugin/nothelix/cursor-restore.scm` (provide list line 10-13; add two functions; refactor `save-cursor-for-restore!` and `restore-cursor-for!` to call them)

**Interfaces:**
- Produces: `(compute-cursor-anchor doc-id) -> (list ord offset col)` — the same triple `save-cursor-for-restore!` computes, returned instead of stored.
- Produces: `(move-cursor-to-anchor! doc-id ord offset col)` — resolves the ordinal to its current marker line, clamps offset/col, moves and collapses the cursor.
- Consumes: existing private helpers `enclosing-marker-ordinal`, `nth-marker-line`, `clamp`, `line-visible-length`.

- [ ] **Step 1: Add the two functions to the provide list**

Change the `provide` block (lines 10-13) to:

```scheme
(provide save-cursor-for-restore!
         restore-cursor-for!
         clear-cursor-restore!
         move-to-line-start-no-center!
         compute-cursor-anchor
         move-cursor-to-anchor!)
```

- [ ] **Step 2: Add `compute-cursor-anchor` and `move-cursor-to-anchor!`**

Insert before `save-cursor-for-restore!` (currently line 54):

```scheme
;;@doc
;; The (ord offset col) cursor anchor for `doc-id`'s focused cursor, anchored to
;; the enclosing cell marker so output inserted below cells does not drift it.
(define (compute-cursor-anchor doc-id)
  (define rope (editor->text doc-id))
  (define total (text.rope-len-lines rope))
  (define pos (cursor-position))
  (define line (text.rope-char->line rope pos))
  (define line-start (text.rope-line->char rope line))
  (define col (- pos line-start))
  (define ord (enclosing-marker-ordinal rope total line))
  (define offset
    (if (> ord 0)
        (let ([m (nth-marker-line rope total ord)]) (if m (- line m) line))
        line))
  (list ord offset col))

;;@doc
;; Move `doc-id`'s cursor to the (ord offset col) anchor, resolving the marker's
;; current line and clamping so a since-edited file lands nearby, not out of range.
(define (move-cursor-to-anchor! doc-id ord offset col)
  (define rope (editor->text doc-id))
  (define total (text.rope-len-lines rope))
  (define base-line
    (if (> ord 0)
        (let ([m (nth-marker-line rope total ord)]) (if m m 0))
        0))
  (define target-line (clamp 0 (+ base-line offset) (max 0 (- total 1))))
  (define line-start (text.rope-line->char rope target-line))
  (define target-col (clamp 0 col (line-visible-length rope target-line)))
  (define char (+ line-start target-col))
  (define r (helix.static.range char char))
  (define sel (helix.static.range->selection r))
  (helix.static.set-current-selection-object! sel)
  (helix.static.collapse_selection))
```

- [ ] **Step 3: Refactor `save-cursor-for-restore!` to reuse `compute-cursor-anchor`**

Replace the body of `save-cursor-for-restore!` (lines 56-69) with:

```scheme
(define (save-cursor-for-restore! doc-id)
  (set! *pending-cursor-restore*
        (hash-insert *pending-cursor-restore* doc-id (compute-cursor-anchor doc-id))))
```

- [ ] **Step 4: Refactor `restore-cursor-for!` to reuse `move-cursor-to-anchor!`**

Replace the body of `restore-cursor-for!` (lines 73-92) with:

```scheme
(define (restore-cursor-for! doc-id)
  (when (hash-contains? *pending-cursor-restore* doc-id)
    (define entry (hash-get *pending-cursor-restore* doc-id))
    (move-cursor-to-anchor! doc-id (list-ref entry 0) (list-ref entry 1) (list-ref entry 2))))
```

- [ ] **Step 5: Verify the plugin still loads headlessly**

Run: `just check`
Expected: clippy + nextest + headless plugin-load all pass. The refactor is behaviour-preserving; existing cursor-restore consumers are unchanged.

- [ ] **Step 6: Commit**

```bash
jj describe @ -m "refactor(cursor-restore): expose compute/move anchor primitives

Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>"
```

---

### Task 3: `resume.scm` module + hook wiring

**Files:**
- Create: `plugin/nothelix/resume.scm`
- Modify: `plugin/nothelix.scm` (add require ~line 43; save capture in `nothelix-post-command-hook` save branch ~line 374; restore in `document-opened` callback ~line 396)
- Modify: `plugin/nothelix/execution.scm` (add require; capture at top of `execute-cell` line 90)

**Interfaces:**
- Consumes: `resume-get`/`resume-set` (FFI, Task 1); `compute-cursor-anchor`/`move-cursor-to-anchor!` (Task 2).
- Produces: `(save-resume-position!)` — captures the focused notebook's anchor to disk; `(restore-resume-position! doc-id)` — restores `doc-id`'s stored anchor if present.

- [ ] **Step 1: Create `resume.scm`**

```scheme
;;; resume.scm — Cross-session notebook resume position.
;;;
;;; Persists the focused notebook's cursor anchor (cell ordinal, line offset,
;;; column) to ~/.local/share/nothelix/resume via the dylib, and restores it
;;; when the notebook is reopened. Best-effort: a missing or stale entry leaves
;;; the cursor at the top.

(require "cursor-restore.scm")
(require "helix/editor.scm")
(require "helix/misc.scm")

(#%require-dylib "libnothelix"
                 (only-in nothelix
                          resume-get
                          resume-set))

(provide save-resume-position!
         restore-resume-position!)

;;@doc
;; Capture the focused notebook's cursor anchor to the resume store.
(define (save-resume-position!)
  (define doc-id (editor->doc-id (editor-focus)))
  (define path (and doc-id (editor-document->path doc-id)))
  (when path
    (define anchor (compute-cursor-anchor doc-id))
    (resume-set path (list-ref anchor 0) (list-ref anchor 1) (list-ref anchor 2))))

;;@doc
;; Restore `doc-id`'s stored cursor anchor if one exists; no-op otherwise.
(define (restore-resume-position! doc-id)
  (define path (and doc-id (editor-document->path doc-id)))
  (when path
    (define stored (resume-get path))
    (when (> (string-length stored) 0)
      (define parts (string-split stored "\t"))
      (when (>= (length parts) 3)
        (define ord (string->number (list-ref parts 0)))
        (define off (string->number (list-ref parts 1)))
        (define col (string->number (list-ref parts 2)))
        (when (and ord off col)
          (move-cursor-to-anchor! doc-id ord off col))))))
```

- [ ] **Step 2: Wire the require into `nothelix.scm`**

After line 42 (`(require "nothelix/project-config.scm")`), add:

```scheme
(require "nothelix/resume.scm")
```

- [ ] **Step 3: Capture on save**

In `nothelix-post-command-hook`, in the `*save-commands*` branch (currently lines 368-375), add `(save-resume-position!)` as the last form before `(schedule-reconceal 50)`:

```scheme
    [(member command-name *save-commands*)
       (format-math-buffer #true)
       (math-render-buffer)
       (when (not (math-image-test-mode?))
         (render-all-display-math)
         (render-all-tables))
       (renumber-cells!)
       (save-resume-position!)
       (schedule-reconceal 50)]
```

- [ ] **Step 4: Restore on open**

In the `document-opened` hook callback (lines 382-396), add `(restore-resume-position! _doc-id)` as the final form inside the delayed `when`, after `(render-all-tables)`. Rename the lambda parameter `_doc-id` to `doc-id` so it can be passed:

```scheme
(register-hook! "document-opened"
  (lambda (doc-id)
    (set! *conceal-generation* (+ *conceal-generation* 1))
    (define my-gen *conceal-generation*)
    (enqueue-thread-local-callback-with-delay 200
      (lambda ()
        (when (= my-gen *conceal-generation*)
          (maybe-apply-project-config!)
          (render-cached-images)
          (when (conceal-on-open?)
            (maybe-conceal-current-buffer))
          (when (not (math-image-test-mode?))
            (render-all-display-math)
            (render-all-tables))
          (restore-resume-position! doc-id))))))
```

- [ ] **Step 5: Capture on cell-run**

In `plugin/nothelix/execution.scm`, add near the other requires at the top of the file:

```scheme
(require "resume.scm")
```

Then in `execute-cell` (line 90), capture before saving/running:

```scheme
(define (execute-cell)
  (save-resume-position!)
  (with-saved-notebook ":execute-cell" execute-cell-under-cursor))
```

- [ ] **Step 6: Verify the plugin loads headlessly**

Run: `just check`
Expected: clippy + nextest + headless plugin-load all pass. (Requires the v19 dylib from Task 1 installed.)

- [ ] **Step 7: Manual smoke test**

Ask the user to: open a notebook, move into a lower cell, save (`:w`), quit, reopen — the cursor should land back in that cell. Confirm `~/.local/share/nothelix/resume` contains a line for the notebook path.

- [ ] **Step 8: Commit**

```bash
jj describe @ -m "feat(resume): capture on save/run, restore on open

Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>"
```

---

### Task 4: Document the behaviour

**Files:**
- Modify: `docs/notebooks.md` (add a short "Resume where you left off" note)

- [ ] **Step 1: Add the note**

Add a brief section to `docs/notebooks.md` stating that reopening a notebook returns the cursor to the last cell you worked in, that this is per-user and stored outside the project (in `~/.local/share/nothelix/`), and that nothing is written into the project directory. Keep it to a few sentences, matching the site's lean style.

- [ ] **Step 2: Commit**

```bash
jj describe @ -m "docs: note cross-session notebook resume

Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>"
```

---

## Self-Review

**Spec coverage:**
- Storage in `~/.local/share/nothelix/resume`, line format → Task 1. ✓
- Stored triple (ordinal/offset/col) → Task 1 (store) + Task 2 (compute) + Task 3 (capture). ✓
- `resume.rs` mirrors `trust.rs`, `create_dir_all`, canonical keys, FFI `resume-get`/`resume-set`, version 18→19 → Task 1. ✓
- Save trigger on `:w` → Task 3 Step 3; cell-run → Task 3 Step 5. ✓
- Restore on open after conceal/render settle → Task 3 Step 4. ✓
- Error handling (missing/malformed/stale → no-op, clamp) → Task 1 (get returns "") + Task 2 (`move-cursor-to-anchor!` clamps) + Task 3 (guards). ✓
- Tests mirroring `trust.rs` with tempdir seam → Task 1 Step 1. ✓
- Rollout: version bump + `just install`, lazy file creation → Task 1 Steps 5-8. ✓

**Type consistency:** `resume_get`/`resume_set` signatures identical across Task 1 definition and Task 3 consumption (`resume-get` returns tab string; `resume-set` takes path + three numbers). `compute-cursor-anchor` returns `(list ord offset col)` consumed positionally by `save-resume-position!`; `move-cursor-to-anchor!` takes `doc-id ord offset col`, matching both Task 2 definition and Task 3 call. ✓

**Placeholder scan:** No TBD/TODO; every code step shows complete code; the one prose step (Task 4) is genuinely a short doc paragraph, not deferred logic. ✓
