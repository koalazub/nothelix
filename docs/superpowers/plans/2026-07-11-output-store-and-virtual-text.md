# Output Store + Virtual Text Output — Implementation Plan (Foundation, Plan 1 of the output-architecture spec)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Render cell TEXT output (stdout / result repr / stderr / error) as virtual line-annotations sourced from an out-of-buffer store keyed by cell id, so executing a cell no longer inserts the `# ─── Output ───` text block into the buffer or the user's undo history.

**Architecture:** A new `output_lines` virtual-line annotation in the Helix fork (a sibling of the existing `math_lines`, monochrome) renders text rows with no backing buffer text and zero undo cost. A new `libnothelix::output_store` (mirrors `trust.rs`/`resume.rs`) is the per-cell source of truth so re-execution and reopen key off cell id instead of the removed `# ─── Output ───` text sentinel. The plugin stops inserting the text output block and renders from the store.

**Tech Stack:** Rust (libnothelix FFI dylib; Helix fork `~/projects/helix`), Steel plugin, `cargo nextest`, `just check`, `just install`.

**Scope note:** This is Plan 1 of the spec `docs/superpowers/specs/2026-07-11-output-rendering-architecture-design.md`. It handles TEXT output only. RASTER images keep their current real-blank-line reserve path in this plan (still committed to history) — taking image reserve lines out of undo needs the tagged-revision History primitive and is **Plan 2**. Multiple graphs (spec 2) and colored braille (spec 3) are later.

## Global Constraints

- No new files in the user's project tree — output store lives in `~/.local/share/nothelix/outputs/` (same global-dir precedent as trust/resume; the user's standing "no project dotfiles" rule).
- No code comments except module `//!` docs / `;;@doc` blocks — self-documenting code (project rule).
- Use `cargo nextest`, never `cargo test`. Clippy clean: `cargo clippy -p libnothelix --all-targets -- -D warnings`.
- FFI version handshake in TWO files must stay equal and both bump: `NOTHELIX_FFI_VERSION` (`libnothelix/src/lib.rs:70`) and `EXPECTED-FFI-VERSION` (`plugin/nothelix/ffi-version.scm:11`). Current value: 19 → 20.
- Fork work lands in `/Users/koalazub/projects/helix` (branch `feature/inline-image-rendering`); after it, the `flake.nix` `helix-fork` input is bumped and the hx binary rebuilt by the user (nixoala) — the plugin uses deferred-eval guards so it still loads on an un-bumped binary.
- Line-based storage only (no JSON parser dependency beyond `serde_json` already vendored).
- Steel modules verified by the headless plugin-load gate in `just check`, not unit tests.

---

### Task 1: `libnothelix::output_store` — per-cell output persistence

**Files:**
- Create: `libnothelix/src/output_store.rs`
- Modify: `libnothelix/src/lib.rs` (module decl next to `mod resume;` ~line 48; register block next to the `resume-*` fns ~line 202; version `70`)
- Modify: `plugin/nothelix/ffi-version.scm:11`

**Interfaces:**
- Produces (FFI): `output-store-put(workspace: String, cell_id: String, source_hash: String, outputs_json: String) -> String` (`""`/`"ERROR: …"`); `output-store-get(workspace: String, cell_id: String) -> String` (returns `"<source_hash>\t<outputs_json>"` or `""`); `output-store-clear(workspace: String, cell_id: String) -> String`.
- Storage: one file per cell at `~/.local/share/nothelix/outputs/<sanitized-workspace>/<sanitized-cell_id>.json`; file content = `<source_hash>\n<outputs_json>`.

- [ ] **Step 1: Write the failing tests**

Create `libnothelix/src/output_store.rs` with only this test module:

```rust
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
        assert_eq!(get_at(&root.path().to_path_buf(), "ws", "nope"), "");
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo nextest run -p libnothelix output_store`
Expected: FAIL — `get_at`/`set_at`/`clear_at` not found (compile error).

- [ ] **Step 3: Write the implementation**

Prepend above the test module in `libnothelix/src/output_store.rs`:

```rust
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
        .map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '_' { c } else { '_' })
        .collect()
}

fn cell_path(root: &Path, workspace: &str, cell_id: &str) -> PathBuf {
    root.join(sanitize(workspace)).join(format!("{}.json", sanitize(cell_id)))
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

fn set_at(root: &Path, workspace: &str, cell_id: &str, source_hash: &str, outputs_json: &str) -> Result<(), String> {
    let path = cell_path(root, workspace, cell_id);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("ERROR: cannot create {}: {e}", parent.display()))?;
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

pub fn output_store_put(workspace: String, cell_id: String, source_hash: String, outputs_json: String) -> String {
    match set_at(&store_root(), &workspace, &cell_id, &source_hash, &outputs_json) {
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
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo nextest run -p libnothelix output_store`
Expected: PASS (6 tests).

- [ ] **Step 5: Register FFI, declare module, bump version**

`libnothelix/src/lib.rs`: after `mod resume;`:

```rust
#[cfg(feature = "native")]
mod output_store;
```

In the register block, after the `resume-*` registrations:

```rust
    m.register_fn("output-store-put", output_store::output_store_put);
    m.register_fn("output-store-get", output_store::output_store_get);
    m.register_fn("output-store-clear", output_store::output_store_clear);
```

Bump `NOTHELIX_FFI_VERSION` (lib.rs:70) `= 20`. Bump `EXPECTED-FFI-VERSION` (`plugin/nothelix/ffi-version.scm:11`) `20`.

- [ ] **Step 6: Verify no stray old version + clippy**

Run: `rg -n "FFI_VERSION|EXPECTED-FFI-VERSION" libnothelix plugin` → both show `20`, no stray `19`.
Run: `cargo clippy -p libnothelix --all-targets -- -D warnings` → clean.
Run: `cargo nextest run -p libnothelix` → all pass.

- [ ] **Step 7: Install + commit**

Ask the user to run `! just install` (release build fails under the sandbox). Then:

```bash
jj describe @ -m "feat(output-store): per-cell out-of-buffer output store (FFI v20)

Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>"
```

---

### Task 2: Fork — `output_lines` virtual-line annotation (sibling of `math_lines`)

**Files (in `/Users/koalazub/projects/helix`):**
- Create: `helix-view/src/annotations/output.rs`
- Modify: `helix-view/src/annotations/mod.rs` (add `pub mod output;`)
- Modify: `helix-view/src/document.rs` (field near `math_lines:166`; init near `798`; methods near `2647`)
- Create: `helix-term/src/ui/text_decorations/output_annotations.rs` (copy of `math_annotations.rs`)
- Modify: `helix-term/src/ui/text_decorations.rs` (or the mod file) to expose `output_annotations`
- Modify: `helix-term/src/ui/editor.rs` (register the decoration alongside `MathAnnotations`, ~line 225)
- Modify: `helix-term/src/commands/engine/steel/mod.rs` (bindings, mirror `math-lines` at 1015-1039 + the `provide`/`define` glue)

**Interfaces:**
- Produces (Steel): `set-output-lines-below!(line-idx, lines)`, `clear-output-lines-at!(line-idx)`, `clear-all-output-lines!` — where `lines` is a list of pre-padded strings (monochrome). Same call convention as `set-math-lines-below!`.
- Rendered as virtual rows below `line-idx`, no buffer text, no history entry (same class as math-lines).

- [ ] **Step 1: Create the `OutputLines` storage type**

`helix-view/src/annotations/output.rs` — a monochrome, below-only copy of `MathLines` (`annotations/math.rs`). Output only ever renders *below* the cell's last code line, so drop the `above` bucket:

```rust
//! `LineAnnotation` that reserves virtual rows BELOW a source line for
//! notebook cell output (stdout / result / stderr / error). A sibling of
//! [`super::math::MathLines`], kept separate so math re-render
//! (`clear_all_math_lines`) never wipes output and vice-versa. Rendered by
//! `helix_term::ui::text_decorations::output_annotations::OutputAnnotations`.

use std::collections::HashMap;

use helix_core::text_annotations::LineAnnotation;
use helix_core::Position;

use crate::Document;

#[derive(Debug, Default, Clone)]
pub struct OutputLines {
    below: HashMap<usize, Vec<String>>,
}

impl OutputLines {
    pub fn is_empty(&self) -> bool {
        self.below.is_empty()
    }

    pub fn below(&self, line_idx: usize) -> Option<&[String]> {
        self.below.get(&line_idx).map(Vec::as_slice)
    }

    pub fn set_below(&mut self, line_idx: usize, lines: Vec<String>) {
        if lines.is_empty() {
            self.below.remove(&line_idx);
        } else {
            self.below.insert(line_idx, lines);
        }
    }

    pub fn clear_at(&mut self, line_idx: usize) {
        self.below.remove(&line_idx);
    }

    pub fn clear(&mut self) {
        self.below.clear();
    }

    pub fn rows_to_reserve_after(&self, doc_line: usize) -> usize {
        self.below(doc_line).map(<[String]>::len).unwrap_or(0)
    }
}

pub struct OutputAnnotations<'a> {
    lines: &'a OutputLines,
}

impl<'a> OutputAnnotations<'a> {
    #[allow(clippy::new_ret_no_self)]
    pub fn new(doc: &'a Document) -> Box<dyn LineAnnotation + 'a> {
        Box::new(OutputAnnotations {
            lines: doc.output_lines(),
        })
    }
}

impl LineAnnotation for OutputAnnotations<'_> {
    fn insert_virtual_lines(
        &mut self,
        _line_end_char_idx: usize,
        _line_end_visual_pos: Position,
        doc_line: usize,
    ) -> Position {
        Position::new(self.lines.rows_to_reserve_after(doc_line), 0)
    }
}
```

Add `pub mod output;` to `helix-view/src/annotations/mod.rs`.

- [ ] **Step 2: Add the Document field, init, accessor, and mutators**

Mirror the `math_lines` members in `helix-view/src/document.rs`. Field (near line 166):

```rust
    output_lines: crate::annotations::output::OutputLines,
```

Init (near line 798, in the same constructor block as `math_lines: … ::default()`):

```rust
            output_lines: crate::annotations::output::OutputLines::default(),
```

Methods (near the `set_math_lines_*` methods ~2647): mirror `math_lines()`, `set_math_lines_below`, `clear_math_lines`, `clear_all_math_lines` as `output_lines()`, `set_output_lines_below`, `clear_output_lines_at`, `clear_all_output_lines` (below-only):

```rust
    pub fn output_lines(&self) -> &crate::annotations::output::OutputLines {
        &self.output_lines
    }

    pub fn set_output_lines_below(&mut self, line_idx: usize, lines: Vec<String>) {
        self.output_lines.set_below(line_idx, lines);
    }

    pub fn clear_output_lines_at(&mut self, line_idx: usize) {
        self.output_lines.clear_at(line_idx);
    }

    pub fn clear_all_output_lines(&mut self) {
        self.output_lines.clear();
    }
```

- [ ] **Step 3: Copy the decoration renderer**

Copy `helix-term/src/ui/text_decorations/math_annotations.rs` → `output_annotations.rs`, renaming `MathAnnotations` → `OutputAnnotations`, `math_lines` → `output_lines`, and dropping any `above`-bucket handling (output is below-only). Register the module in `helix-term/src/ui/text_decorations.rs` (add `pub mod output_annotations;` next to `pub mod math_annotations;`). Read the source file before copying — it is short and self-contained; keep the row-painting logic identical.

- [ ] **Step 4: Register the decoration in the render pipeline**

In `helix-term/src/ui/editor.rs` where `MathAnnotations` is added to the decorations (~line 225, `decorations.add_decoration(MathAnnotations::new(doc))` or equivalent), add the sibling right after:

```rust
        decorations.add_decoration(text_decorations::output_annotations::OutputAnnotations::new(doc));
```

(Match the exact registration call shape used for `MathAnnotations` at that site.)

- [ ] **Step 5: Add the Steel bindings**

In `helix-term/src/commands/engine/steel/mod.rs`, mirror the math-lines block at 1015-1039 (the `register_fn` + doc-registration + `(provide …)` + `(define …)` wrapper) for three functions:

```rust
    module.register_fn("set-output-lines-below!", set_output_lines_below);
    module.register_fn("clear-output-lines-at!", clear_output_lines_at);
    module.register_fn("clear-all-output-lines!", clear_all_output_lines);
```

with Steel wrappers mirroring lines 1038-1039:

```scheme
(provide set-output-lines-below!)
(define (set-output-lines-below! line-idx lines)
    (helix.static.set-output-lines-below! *helix.cx* line-idx lines))
(provide clear-output-lines-at!)
(define (clear-output-lines-at! line-idx)
    (helix.static.clear-output-lines-at! *helix.cx* line-idx))
(provide clear-all-output-lines!)
(define (clear-all-output-lines!)
    (helix.static.clear-all-output-lines! *helix.cx*))
```

Define the three `set_output_lines_below` / `clear_output_lines_at` / `clear_all_output_lines` Rust glue functions mirroring the `set_math_lines_below` etc. handlers in the same file (they call `doc.set_output_lines_below(line_idx, lines)` etc. on the focused document).

- [ ] **Step 6: Build the fork**

Run (in `/Users/koalazub/projects/helix`): `cargo build` (debug is fine for the compile check).
Expected: compiles clean. Fix any mirror mismatches.

- [ ] **Step 7: Commit the fork change**

```bash
cd /Users/koalazub/projects/helix && git add -A && git commit -m "feat(annotations): output_lines virtual-line annotation for notebook cell output"
```

Report the new fork commit SHA — Task 3 bumps the flake input to it.

---

### Task 3: Bump the fork input + expose bindings to the plugin

**Files:**
- Modify: `flake.nix` (the `helix-fork` input rev, ~line 27-52) → the Task 2 commit SHA; update the steel-core `outputHashes` if the vendored rev changed.
- Modify: `plugin/nothelix/output-render.scm` (new; deferred-eval wrappers).

**Interfaces:**
- Produces (Steel, plugin-facing): `try-set-output-lines-below!(line-idx lines)`, `try-clear-output-lines-at!(line-idx)`, `try-clear-all-output-lines!` — deferred-eval guards so the plugin loads on an un-bumped hx (mirrors `math-render.scm:21-34`).

- [ ] **Step 1: Bump the helix-fork input**

Edit `flake.nix` `helix-fork.rev` to the Task 2 commit SHA and its `narHash`. Ask the user to rebuild the hx binary (nixoala owns it) — this is a user step; the plugin's deferred guards mean it keeps working on the old binary until then, just without virtual output.

- [ ] **Step 2: Create the deferred-eval wrappers**

`plugin/nothelix/output-render.scm`, mirroring `math-render.scm:21-34`:

```scheme
;;; output-render.scm — Deferred wrappers over the fork's output-lines
;;; virtual-line annotation, so the plugin loads on an hx without it.

(require "helix/editor.scm")
(require "helix/misc.scm")

(provide try-set-output-lines-below!
         try-clear-output-lines-at!
         try-clear-all-output-lines!
         output-lines-ffi-available?)

(define (try-set-output-lines-below! line-idx lines)
  (with-handler
    (lambda (_) #false)
    (eval `(helix.static.set-output-lines-below! ,line-idx ',lines))))

(define (try-clear-output-lines-at! line-idx)
  (with-handler
    (lambda (_) #false)
    (eval `(helix.static.clear-output-lines-at! ,line-idx))))

(define (try-clear-all-output-lines!)
  (with-handler
    (lambda (_) #false)
    (eval '(helix.static.clear-all-output-lines!))))

(define (output-lines-ffi-available?)
  (with-handler
    (lambda (_) #false)
    (eval '(helix.static.clear-all-output-lines!))
    #true))
```

- [ ] **Step 3: Wire the require + verify load**

Add `(require "nothelix/output-render.scm")` in `plugin/nothelix.scm` after the math-render require. Run `just check` (after the user's `just install` from Task 1 and hx rebuild) → clippy + nextest + headless plugin load pass.

- [ ] **Step 4: Commit**

```bash
jj describe @ -m "feat(output): bump helix-fork for output-lines; deferred plugin wrappers

Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>"
```

---

### Task 4: Plugin — render text output virtually from the store on execute

**Files:**
- Modify: `plugin/nothelix/output-insert.scm` (the text-insertion branches: stdout `118-119`, output-repr `167-168`, stderr `188-190`, error `107-108`, footer `192`; the header insert `execution.scm:139`)
- Modify: `plugin/nothelix/execution.scm` (header insertion + store write)
- Create/Modify: `plugin/nothelix/output-store.scm` (thin Steel layer over the FFI + workspace/cell-id/source-hash helpers)

**Interfaces:**
- Consumes: `output-store-put`/`get`/`clear` (FFI, Task 1); `try-set-output-lines-below!`/`try-clear-output-lines-at!` (Task 3).
- Produces: `store-cell-output!(cell-index outputs-json source-hash)`, `render-cell-text-output!(cell-index)` — write the store and paint text rows below the cell's last code line.

- [ ] **Step 1: Create `output-store.scm`**

```scheme
;;; output-store.scm — Steel layer over the per-cell output store FFI.

(require "helix/editor.scm")
(require "string-utils.scm")
(#%require-dylib "libnothelix"
                 (only-in nothelix output-store-put output-store-get output-store-clear))

(provide workspace-id cell-source-hash
         store-put! store-get store-clear!)

(define (workspace-id)
  (define p (editor-document->path (editor->doc-id (editor-focus))))
  (if p p "unknown"))

(define (cell-source-hash code) (number->string (djb2-hash code)))

(define (store-put! cell-id source-hash outputs-json)
  (output-store-put (workspace-id) cell-id source-hash outputs-json))
(define (store-get cell-id) (output-store-get (workspace-id) cell-id))
(define (store-clear! cell-id) (output-store-clear (workspace-id) cell-id))
```

(`djb2-hash` already exists in `image-cache.scm:47`; export it there or duplicate the tiny loop — prefer exporting.)

- [ ] **Step 2: Replace text insertion with a virtual-row render**

In `output-insert.scm` `update-cell-output`, replace the `helix.static.insert_string` calls that write commentified text (`stdout` 118-119, `output-repr` 167-168, `stderr` 188-190, footer 192, and the error branch 107-108) with: build a list of output text lines (uncommentified — the virtual rows are display-only, they do not need `# `), call `try-set-output-lines-below!` at the cell's last code line with that list, and `store-put!` the nbformat outputs JSON. Keep the IMAGE path (`image-ready`, `add-raw-content-with-placeholders!`, blank reserve lines) UNCHANGED for this plan. Remove the `# ─── Output ───` header insert (`execution.scm:139`) and its `commit-changes-to-history` — text output no longer occupies buffer lines. (The image reserve lines + their single commit remain until Plan 2.)

- [ ] **Step 3: Locate the cell's last code line for the annotation anchor**

Add a helper in `execution.scm` (or reuse `find-cell-code-end`): the annotation anchors below the last non-blank code line of the cell (`cell-code-end - 1`). Pass that line index to `try-set-output-lines-below!`.

- [ ] **Step 4: Verify headless + manual**

Run `just check` → passes. Manual (after hx rebuild): run a cell → text output appears as virtual rows below the code; the buffer text is unchanged (no `# ─── Output ───`); one `u` reverts only the prior code edit.

- [ ] **Step 5: Commit**

```bash
jj describe @ -m "feat(output): render cell text output as virtual rows from the store

Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>"
```

---

### Task 5: Plugin — re-execution and reopen keyed off cell id (no text sentinel)

**Files:**
- Modify: `plugin/nothelix/execution.scm` (`execute-cell-under-cursor` prior-output clearing `116-121`; `execute-single-cell-async` `259`)
- Modify: `plugin/nothelix.scm` (the `document-opened` delayed callback ~line 382-396)

**Interfaces:**
- Consumes: `store-get`/`store-clear!` (Task 4), `try-clear-output-lines-at!` (Task 3), `clear-raw-content-in-range!` (existing).
- Produces: `clear-cell-output!(cell-index)` — clears the virtual rows + image band + store entry for a cell; `restore-cell-outputs-on-open!` — renders stored, non-stale output for every cell on open.

- [ ] **Step 1: Replace sentinel-based clearing with id-based clearing**

In `execute-cell-under-cursor`/`execute-single-cell-async`, replace the `find-output-start`/`find-output-end-line`/`delete-line-range` block (`execution.scm:116-121`, `259`) with `clear-cell-output!`: `try-clear-output-lines-at!` at the cell's anchor line, `clear-raw-content-in-range!` for the cell's image id band, and `store-clear!` the cell id. The image reserve blank lines (if any) are still deleted by the existing image path in this plan.

- [ ] **Step 2: Render stored output on reopen**

In the `document-opened` delayed callback, after `render-cached-images`, add `restore-cell-outputs-on-open!`: for each cell marker, compute the cell id + current source hash, `store-get` it, and if the stored hash matches, `try-set-output-lines-below!` with the stored text rows (and let `render-cached-images` continue handling image markers for this plan). A mismatch → skip (stale).

- [ ] **Step 3: Verify**

Run `just check`. Manual: run a cell (output shows), reopen the file (output re-renders from store), edit the cell + re-run (old output replaced, no stale rows), edit code then reopen (stale output not shown).

- [ ] **Step 4: Commit**

```bash
jj describe @ -m "feat(output): re-execute and reopen key off cell id, not the output sentinel

Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>"
```

---

### Task 6: Plugin — coalesce the save-hook undo churn

**Files:**
- Modify: `plugin/nothelix.scm` (`nothelix-post-command-hook` save branch, 368-375)

**Interfaces:**
- No new interface — reduces the number of `commit-changes-to-history` calls the save hook produces.

- [ ] **Step 1: Coalesce commits in the save branch**

`renumber-cells!` (`scaffold.scm:340-350`), `format-math-buffer` (`math-format.scm:43-45`), and the display-math reserve (`math-image.scm:445-453`) each commit their own revision on `:w`. Restructure so that when they run as part of an execution-driven save, their edits collapse into a single `commit-changes-to-history` at the end of the save branch (apply all, commit once) rather than each committing independently. Where a helper commits internally, add a variant that defers the commit to the caller.

- [ ] **Step 2: Verify**

Run `just check`. Manual: on a notebook where a save triggers renumber + math-format, edit-then-run adds at most one extra undo step from the save (ideally zero for a clean notebook), not three.

- [ ] **Step 3: Commit**

```bash
jj describe @ -m "fix(undo): coalesce save-hook edits into one revision on execution

Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>"
```

---

## Self-Review

**Spec coverage (against the foundation spec):**
- Out-of-buffer store keyed by cell id + source hash → Task 1 (store) + Task 4/5 (keying). ✓
- Text output as virtual rows, zero undo → Task 2 (annotation) + Task 4 (render). ✓
- `# ─── Output ───` block leaves the buffer → Task 4 (remove header + text inserts). ✓
- Re-execution/reopen key off cell id, not the text sentinel → Task 5. ✓
- Best-effort store; deferred-eval fork guard → Task 1 (empty on miss) + Task 3 (guards). ✓
- Global store dir, no project dotfile → Task 1. ✓
- Save-hook amplifier reduced → Task 6 (spec listed this as a follow-up; included here as the cheap win). ✓
- **Deferred to Plan 2 (documented, not a gap):** image reserve lines out of undo via tagged-revision History primitive; images stay on the real-blank-line path here.
- **Deferred to specs 2/3:** multiple graphs; colored braille (per-span style annotation).

**Placeholder scan:** Task 1 carries full Rust + tests. Task 2 carries the full `OutputLines` type; the decoration is a cited mechanical copy of `math_annotations.rs` (Step 3 names the exact renames) — acceptable because the source file is short, present, and identical in shape. Fork wiring steps cite exact mirror sites (`document.rs:166,798,2647`; `steel/mod.rs:1015-1039`; `editor.rs:~225`). Plugin tasks cite exact lines to modify. No "TBD"/"handle edge cases".

**Type consistency:** FFI names `output-store-put/get/clear` consistent Task 1 ↔ Task 4. Steel `set-output-lines-below!`/`clear-output-lines-at!`/`clear-all-output-lines!` consistent Task 2 (register) ↔ Task 3 (wrap) ↔ Tasks 4/5 (use). `store-put!`/`store-get`/`store-clear!` consistent Task 4 ↔ Task 5. FFI version 20 in both handshake files (Task 1 Step 5).
