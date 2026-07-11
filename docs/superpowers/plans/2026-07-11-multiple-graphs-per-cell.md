# Multiple Graphs Per Cell — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development or superpowers:executing-plans. Steps use checkbox (`- [ ]`) syntax.

**Goal:** Render EVERY plot a cell produces, stacked inline, instead of only the first.

**Architecture:** The kernel already emits all images as a JSON array; add a Rust `json_get_all_images`, allocate a per-cell image-id sub-band so stacked images don't collide, and loop the plugin's insertion over all images.

**Tech Stack:** Rust (libnothelix), Steel plugin, `cargo nextest`, `just check`, `just install`.

## Global Constraints

- No code comments except `//!` / `;;@doc`. `cargo nextest` not `cargo test`. Clippy `-D warnings` clean.
- FFI version: bump both handshake files (`lib.rs:70`, `ffi-version.scm:11`) 20→21 if a new FFI symbol is added.
- Per-cell image cap `SLOTS`=32, keep ids under the 4,000,000 plot band and 2^24 Kitty ceiling. Silent truncation is a defect — status-note when a cell exceeds the cap.
- Builds on Plan 1 (output store + virtual text). Image reserve lines still use the current commit path (image-out-of-undo is a separate plan).

---

### Task 1: `json_get_all_images` (Rust)

**Files:** Modify `libnothelix/src/json_utils.rs` (near `find_first_image_data:184`, `json_get_first_image:44`); `libnothelix/src/lib.rs` (register + version). Test: in `json_utils.rs` test module.

**Interfaces:** Produces FFI `json-get-all-images(json_str, kernel_dir) -> String` returning every image's data (sidecar-resolved) joined by `\n` (empty string if none); and `json-get-image-count(json_str) -> String` (decimal count). Keep `json_get_first_image*` unchanged.

- [ ] **Step 1: Failing tests** — in the `json_utils.rs` test module, mirror the existing first-image tests but assert all entries:

```rust
#[test]
fn all_images_returns_every_entry() {
    let j = r#"{"images":[{"format":"png","data":"AAA"},{"format":"png","data":"BBB"}]}"#;
    let out = json_get_all_images(j.to_string(), String::new());
    assert_eq!(out.lines().count(), 2);
    assert!(out.contains("AAA") && out.contains("BBB"));
}

#[test]
fn all_images_empty_when_none() {
    assert_eq!(json_get_all_images(r#"{"images":[]}"#.to_string(), String::new()), "");
}

#[test]
fn image_count_matches() {
    let j = r#"{"images":[{"data":"A"},{"data":"B"},{"data":"C"}]}"#;
    assert_eq!(json_get_image_count(j.to_string()), "3");
}
```

- [ ] **Step 2: Run — expect FAIL** (`cargo nextest run -p libnothelix json_utils` → functions not found).

- [ ] **Step 3: Implement** — add, mirroring `find_first_image_data`/`json_get_first_image_with_dir` (which already resolve sidecars):

```rust
fn find_all_image_data(v: &Value, kernel_dir: &str) -> Vec<String> {
    v.get("images")
        .and_then(Value::as_array)
        .map(|arr| {
            arr.iter()
                .filter_map(|img| resolve_one_image(img, kernel_dir))
                .collect()
        })
        .unwrap_or_default()
}

pub fn json_get_all_images(json_str: String, kernel_dir: String) -> String {
    let parsed: Value = match serde_json::from_str(&json_str) {
        Ok(v) => v,
        Err(_) => return String::new(),
    };
    find_all_image_data(&parsed, &kernel_dir).join("\n")
}

pub fn json_get_image_count(json_str: String) -> String {
    let parsed: Value = match serde_json::from_str(&json_str) {
        Ok(v) => v,
        Err(_) => return "0".into(),
    };
    parsed
        .get("images")
        .and_then(Value::as_array)
        .map_or(0, Vec::len)
        .to_string()
}
```

Factor the single-entry resolution (base64-or-sidecar) that `find_first_image_data`/`json_get_first_image_with_dir` already do into a shared `resolve_one_image(img, kernel_dir)` and call it from both first- and all- paths (DRY). Read those functions and reuse their exact logic.

- [ ] **Step 4: Run — expect PASS** (`cargo nextest run -p libnothelix json_utils`).

- [ ] **Step 5: Register + version** — in `lib.rs`, register `json-get-all-images`, `json-get-image-count`; bump `NOTHELIX_FFI_VERSION` 20→21 and `EXPECTED-FFI-VERSION` 21. Verify no stray 20. Clippy clean. Full `cargo nextest run -p libnothelix`.

- [ ] **Step 6: Commit** — `jj describe @ -m "feat(images): json-get-all-images/count (FFI v21)"` (+ Co-Authored-By).

---

### Task 2: Per-cell image-id sub-band (`image-cache.scm`)

**Files:** Modify `plugin/nothelix/image-cache.scm` (`cell-index->image-id:42`, `path->image-id:57`, `extract-cell-index-from-path`).

**Interfaces:** Produces `cell-img->image-id(cell-index, img-index) -> int`; `path->image-id` parses `cell-<idx>-<img>.png` → the same id. `*plots-per-cell*` = 32 (overridable via `.nothelix.conf`).

- [ ] **Step 1: Sub-band id + config** — replace the one-id-per-cell scheme:

```scheme
(define *plots-per-cell* (box 32))
(define (plots-per-cell) (unbox *plots-per-cell*))

;;@doc
;; Distinct kitty image id for the (cell, image) pair, inside the plot band.
(define (cell-img->image-id cell-index img-index)
  (+ 1000 (modulo (+ (* cell-index (plots-per-cell)) img-index)
                  (- 3999000 (plots-per-cell)))))
```

Keep `cell-index->image-id` as `(cell-img->image-id cell-index 0)` for existing single-image callers (backward compatible). Add a `.nothelix.conf` reader for `plots-per-cell` in `project-config.scm`'s `apply-project-config!` (mirror an existing numeric key like `render-width`), calling `set-plots-per-cell!`.

- [ ] **Step 2: `path->image-id` parses both indices** — extend `extract-cell-index-from-path` (or add `extract-cell-and-img-from-path`) to read `cell-<idx>-<img>` (falling back to `cell-<idx>` → img 0), and `path->image-id` returns `cell-img->image-id`.

- [ ] **Step 3: Cache file naming** — where images are saved (`save-image-to-cache!`), name `cell-<idx>-<img>.png`. Read the current naming and thread `img-index`.

- [ ] **Step 4: Verify** — `just check` (after user `just install` of v21). Manual deferred.

- [ ] **Step 5: Commit** — `jj describe @ -m "feat(images): per-cell image-id sub-band + plots-per-cell config"`.

---

### Task 3: Render all images stacked (`output-insert.scm`, reopen)

**Files:** Modify `plugin/nothelix/output-insert.scm` (image branch — replace `json-get-first-image` with a loop over `json-get-all-images`); confirm `image-cache.scm` reopen (`render-cached-images`) handles multiple `# @image` markers per cell.

**Interfaces:** Consumes Task 1 FFI + Task 2 ids.

- [ ] **Step 1: Loop the insertion** — in the image branch, get the list from `json-get-all-images`; for each `(img-index, b64)`, `save-image-to-cache!` (indexed name), insert its `# @image <path>` marker + reserve height rows, register `add-raw-content-with-placeholders!` with `cell-img->image-id`. Stack vertically. Cap at `*plots-per-cell*` and `set-status!` a note if exceeded. Keep `plot_data` overlay first-image-only.

- [ ] **Step 2: Reopen** — verify `render-cached-images` re-registers every `# @image` marker with its `path->image-id` (Task 2 makes ids distinct). If it dedupes by cell, fix to per-marker.

- [ ] **Step 3: Verify** — `just check`. Manual (live): 3 plots in a cell → 3 stacked; reopen restores all; re-exec replaces; cap truncates with note.

- [ ] **Step 4: Commit** — `jj describe @ -m "feat(images): render all cell plots stacked; reopen restores each"`.

---

## Self-Review

- Spec: all-images read (T1), sub-band ids (T2), stacked render + reopen + cap (T3). ✓
- Placeholder scan: T1 full Rust+tests; T2/T3 cite exact sites + show the id/loop code. Config reader mirrors an existing key (named). No "handle edge cases".
- Type consistency: `json-get-all-images`/`json-get-image-count` T1↔T3; `cell-img->image-id` T2↔T3; FFI v21 both files (T1 S5).
- Deferred: image-out-of-undo (separate plan) — reserve lines still commit here; colored braille (spec 3).
