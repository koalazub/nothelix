# Fix Editor Freezing & Async Architecture Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Eliminate the 99% CPU / editor freeze caused by nothelix running expensive operations on the main thread, and convert blocking patterns to async using Steel's `spawn-native-thread` + `hx.with-context` API.

**Architecture:** The core problem is that all Steel plugin callbacks run synchronously on Helix's main thread. We fix this in three layers: (1) eliminate unnecessary work (false `$` matching in Julia code, dead pre-scan code), (2) add debouncing so work doesn't pile up, (3) move expensive operations (conceal computation, kernel startup, image loading) to background threads using `spawn-native-thread` with results delivered back via `hx.with-context`.

**Tech Stack:** Steel (Scheme), Rust FFI (`libnothelix`), Helix plugin API (`helix/ext.scm` async primitives)

---

## File Structure

| File | Responsibility | Action |
|------|---------------|--------|
| `plugin/nothelix/conceal.scm` | LaTeX concealment overlay computation | **Modify**: filter to comment lines only for `.jl` files, move FFI call to background thread |
| `plugin/nothelix.scm` | Main plugin entry, hooks, auto-conceal | **Modify**: add debounce state, pass file extension context to conceal |
| `plugin/nothelix/kernel.scm` | Kernel lifecycle management | **Modify**: replace `sleep-ms` polling loop with async `enqueue-thread-local-callback-with-delay` |
| `plugin/nothelix/execution.scm` | Cell execution, output, polling, images | **Modify**: cache spinner line, remove `sleep-ms`, move image loading to background thread |
| `libnothelix/src/unicode.rs` | Rust LaTeX parsing + overlay computation | **Modify**: add `compute_conceal_overlays_for_comments` variant, remove dead pre-scan code |

---

### Task 1: Fix false `$` math region matching in Julia files

The `$` character is Julia's string interpolation operator. `find_math_regions` in Rust treats every `$...$` as a LaTeX math region, creating huge spurious regions in `.jl` files that get fed to `mathlex::parse_latex_lenient`. This is the #1 CPU hog.

**Files:**
- Modify: `libnothelix/src/unicode.rs:2752-2786` (`compute_conceal_overlays`)
- Modify: `plugin/nothelix/conceal.scm:141-165` (`compute-conceal-overlays`)
- Modify: `plugin/nothelix.scm:75-81` (`conceal-math`)

**Strategy:** For `.jl` files, only scan lines starting with `# ` (markdown cell content). Raw Julia code lines never contain LaTeX math. We add a new Rust function `compute_conceal_overlays_for_comments` that pre-filters to comment lines before scanning for `$` regions.

- [ ] **Step 1: Add `compute_conceal_overlays_for_comments` in Rust**

In `libnothelix/src/unicode.rs`, add a new public function right after the existing `compute_conceal_overlays`:

```rust
/// Like `compute_conceal_overlays`, but only scans lines that start with "# "
/// (Julia/notebook comment lines that contain markdown with LaTeX math).
/// This avoids false-positive `$` matches from Julia string interpolation.
pub fn compute_conceal_overlays_for_comments(text: String) -> String {
    // Build a filtered string of only comment-line content, tracking the
    // original byte offsets so overlays map back to the real document.
    let mut comment_ranges: Vec<(usize, usize)> = Vec::new(); // (original_start, original_end)
    let mut filtered = String::new();

    for (line_start, line) in line_ranges(&text) {
        let trimmed = line.trim_end_matches('\n').trim_end_matches('\r');
        if let Some(content) = trimmed.strip_prefix("# ") {
            let content_start = line_start + (trimmed.len() - content.len());
            let content_end = content_start + content.len();
            comment_ranges.push((content_start, filtered.len()));
            filtered.push_str(content);
            filtered.push('\n');
        }
    }

    if filtered.is_empty() {
        return "[]".to_string();
    }

    // Run the normal overlay pipeline on the filtered text.
    let regions = find_math_regions(&filtered);
    if regions.is_empty() {
        return "[]".to_string();
    }

    let mut all_overlays: Vec<serde_json::Value> = Vec::new();

    for (region_start, region_end) in regions {
        if region_end <= region_start {
            continue;
        }
        let math_text = &filtered[region_start..region_end];
        let json_str = latex_overlays(math_text.to_string());

        if let Ok(v) = serde_json::from_str::<serde_json::Value>(&json_str) {
            if let Some(arr) = v.as_array() {
                for obj in arr {
                    if let Some(offset) = obj.get("offset").and_then(|o| o.as_i64()) {
                        let replacement = obj
                            .get("replacement")
                            .and_then(|o| o.as_str())
                            .unwrap_or("");
                        // Map filtered offset back to original document offset.
                        let filtered_pos = region_start + offset as usize;
                        let original_pos = map_filtered_to_original(&comment_ranges, filtered_pos, &filtered);
                        all_overlays.push(json!({
                            "offset": original_pos,
                            "replacement": replacement
                        }));
                    }
                }
            }
        }
    }

    json!(all_overlays).to_string()
}

/// Iterate over (byte_offset_of_line_start, line_str) pairs.
fn line_ranges(text: &str) -> Vec<(usize, &str)> {
    let mut result = Vec::new();
    let mut start = 0;
    for line in text.split('\n') {
        result.push((start, line));
        start += line.len() + 1; // +1 for the \n
    }
    result
}

/// Map a byte offset in the filtered (comments-only) string back to the
/// original document byte offset.
fn map_filtered_to_original(
    comment_ranges: &[(usize, usize)], // (original_start, filtered_start)
    filtered_pos: usize,
    _filtered: &str,
) -> usize {
    // Find which comment range this position falls in.
    // comment_ranges is sorted by filtered_start.
    let mut best_orig = 0;
    let mut best_filt = 0;
    for &(orig_start, filt_start) in comment_ranges {
        if filt_start <= filtered_pos {
            best_orig = orig_start;
            best_filt = filt_start;
        } else {
            break;
        }
    }
    best_orig + (filtered_pos - best_filt)
}
```

- [ ] **Step 2: Register the new FFI function**

In `libnothelix/src/lib.rs`, add the registration:

```rust
m.register_fn(
    "compute-conceal-overlays-for-comments",
    unicode::compute_conceal_overlays_for_comments,
);
```

- [ ] **Step 3: Remove dead pre-scan code in `latex_overlays`**

In `libnothelix/src/unicode.rs:2814-2851`, the pre-scan block collects `begins` and `ends` vectors then immediately discards them (`let _ = begins; let _ = ends;`). Delete the entire block (lines 2814-2851, the `{ ... }` block starting with `let mut begins`).

- [ ] **Step 4: Update `conceal.scm` to import and use the new FFI function**

In `plugin/nothelix/conceal.scm`, add the new import and a file-type-aware compute function:

Change the `#%require-dylib` to also import the new function:

```scheme
(#%require-dylib "libnothelix"
                 (only-in nothelix
                          compute-conceal-overlays-ffi
                          compute-conceal-overlays-for-comments
                          latex-overlays))
```

Replace `compute-conceal-overlays` (lines 141-165) with:

```scheme
;;;@doc
;;; Compute conceal overlay pairs for the current document.
;;; For .jl files, only scans comment lines (# ...) to avoid false $ matches.
;;; For other files (.md, .tex, etc.), scans the full document.
(define (compute-conceal-overlays)
  (define focus (editor-focus))
  (define doc-id (editor->doc-id focus))
  (define rope (editor->text doc-id))
  (define text (text.rope->string rope))
  (define path (editor-document->path doc-id))
  (define json-str
    (if (and path (ends-with-jl? path))
        (compute-conceal-overlays-for-comments text)
        (compute-conceal-overlays-ffi text)))

  (if (string=? json-str "[]")
      '()
      (let parse-loop ([pos 0] [result '()])
        (cond
          [(>= pos (string-length json-str)) (reverse result)]
          [(char=? (string-ref json-str pos) #\{)
           (let* ([colon1-pos (find-char json-str #\: (+ pos 1))]
                  [offset-start (skip-whitespace json-str (+ colon1-pos 1))]
                  [offset-end (find-non-digit json-str offset-start)]
                  [offset-val (string->number (substring json-str offset-start offset-end))]
                  [colon2-pos (find-char json-str #\: offset-end)]
                  [quote1-pos (find-char json-str #\" (+ colon2-pos 1))]
                  [replacement-str (extract-json-string json-str (+ quote1-pos 1))]
                  [after-str (+ quote1-pos 1 (json-string-raw-length json-str (+ quote1-pos 1)) 1)]
                  [close-pos (find-char json-str #\} after-str)])
             (parse-loop (+ close-pos 1)
                         (cons (cons offset-val replacement-str) result)))]
          [else (parse-loop (+ pos 1) result)]))))

(define (ends-with-jl? path)
  (define len (string-length path))
  (and (>= len 3)
       (string=? (substring path (- len 3) len) ".jl")))
```

- [ ] **Step 5: Build and verify compilation**

Run: `cd /Users/koalazub/projects/nothelix && cargo build -p libnothelix 2>&1`
Expected: Compiles without errors.

- [ ] **Step 6: Commit**

```bash
git add libnothelix/src/unicode.rs libnothelix/src/lib.rs plugin/nothelix/conceal.scm
git commit -m "fix: only scan comment lines for LaTeX math in .jl files

Julia uses $ for string interpolation, which caused find_math_regions
to create huge false-positive regions that were then fed to the LaTeX
parser. For .jl files, we now pre-filter to lines starting with '# '
before scanning for math regions. Also removes dead pre-scan code."
```

---

### Task 2: Add debounce to buffer-switch auto-conceal

Every `buffer-next`/`buffer-previous` command enqueues a callback that runs `render-cached-images` + full conceal. Rapid buffer switching stacks these up. We add a generation counter so only the latest callback executes.

**Files:**
- Modify: `plugin/nothelix.scm:182-197` (post-command hook and document-opened hook)

- [ ] **Step 1: Add debounce state and guarded hook**

Replace lines 182-197 in `plugin/nothelix.scm` with:

```scheme
;; Debounce counter: each new trigger increments the generation.
;; Callbacks check their captured generation against the current one;
;; if stale, they skip execution.
(define *conceal-generation* 0)

;; Hook for kernel cleanup on exit and image rendering on buffer switch
(define (nothelix-post-command-hook command-name)
  (cond
    [(member command-name *quit-commands*)
     (stop-all-kernels)]
    [(member command-name '("buffer-next" "buffer-previous"))
     (set! *conceal-generation* (+ *conceal-generation* 1))
     (define my-gen *conceal-generation*)
     (enqueue-thread-local-callback-with-delay 150
       (lambda ()
         (when (= my-gen *conceal-generation*)
           (render-cached-images)
           (maybe-conceal-current-buffer))))]))

(register-hook! "post-command" nothelix-post-command-hook)
(register-hook! "document-opened"
  (lambda (_doc-id)
    (set! *conceal-generation* (+ *conceal-generation* 1))
    (define my-gen *conceal-generation*)
    (enqueue-thread-local-callback-with-delay 200
      (lambda ()
        (when (= my-gen *conceal-generation*)
          (maybe-conceal-current-buffer))))))
```

Key changes:
- Generation counter prevents stale callbacks from running
- Delay increased from 50ms to 150ms for buffer-switch (gives user time to settle)
- Delay increased from 100ms to 200ms for document-opened

- [ ] **Step 2: Commit**

```bash
git add plugin/nothelix.scm
git commit -m "fix: debounce auto-conceal on buffer switch and document open

Rapid buffer switching no longer stacks up multiple full-document
conceal computations. A generation counter ensures only the latest
callback runs."
```

---

### Task 3: Convert kernel startup to async (non-blocking)

`kernel-start` calls `sleep-ms 200` in a loop up to 150 times, blocking the main thread for up to 30 seconds. Convert to the same `enqueue-thread-local-callback-with-delay` polling pattern already used for execution results.

**Files:**
- Modify: `plugin/nothelix/kernel.scm:40-98` (`kernel-start`)

- [ ] **Step 1: Rewrite `kernel-start` as async with callback**

Replace the entire `kernel-start` function (lines 40-98) with:

```scheme
;;@doc
;; Start a new kernel for the given language and notebook path.
;; Non-blocking: spawns the kernel process, then polls for readiness
;; via delayed callbacks. Calls `on-ready` with the kernel-state hash
;; on success, or shows an error in the status bar on failure.
;; Returns #true if the spawn succeeded (polling has started), #false
;; if it failed immediately.
(define (kernel-start lang notebook-path on-ready)
  (define kernel-dir "/tmp/helix-kernel-1")
  (set-status! (string-append "Starting kernel in " kernel-dir "..."))

  (define result-json (kernel-start-macro kernel-dir))

  (cond
    [(string-contains? result-json "julia not found")
     (set-status! "Julia not found. Install Julia (https://julialang.org) and make sure it is on your PATH.")
     #false]

    [(string-contains? result-json "\"status\":\"error\"")
     (set-status! (string-append "Kernel failed to start: " (sanitise-error-message result-json)))
     #false]

    [(equal? (path-exists kernel-dir) "no")
     (set-status! (string-append "Kernel directory was not created at " kernel-dir ". Check file permissions."))
     #false]

    [else
     ;; Begin async polling for the ready file.
     ;; 150 attempts * 200 ms = 30 s max wait.
     (poll-kernel-ready kernel-dir lang notebook-path on-ready 150)
     #true]))

;; Internal: async poll loop for kernel readiness.
(define (poll-kernel-ready kernel-dir lang notebook-path on-ready attempts)
  (cond
    [(equal? (path-exists (string-append kernel-dir "/ready")) "yes")
     ;; Kernel is up.
     (define kernel-state
       (hash 'lang lang
             'kernel-dir kernel-dir
             'input-file (string-append kernel-dir "/input.json")
             'output-file (string-append kernel-dir "/output.json")
             'pid-file (string-append kernel-dir "/pid")
             'ready #true))

     (set! *kernels* (hash-insert *kernels* notebook-path kernel-state))
     (set-status! (string-append "Started " lang " kernel in " kernel-dir))
     (on-ready kernel-state)]

    [(<= attempts 0)
     ;; Timed out.
     (define log-tail (read-file-tail (string-append kernel-dir "/kernel.log") 3))
     (define msg (sanitise-error-message log-tail))
     (if (> (string-length msg) 0)
         (set-status! (string-append "Kernel not ready after 30 s. Julia output: " msg))
         (set-status! "Kernel not ready after 30 s. Check kernel.log in /tmp/helix-kernel-1/ for details."))
     (helix.redraw)]

    [else
     (enqueue-thread-local-callback-with-delay 200
       (lambda () (poll-kernel-ready kernel-dir lang notebook-path on-ready (- attempts 1))))]))
```

- [ ] **Step 2: Add `helix/misc.scm` require (for `enqueue-thread-local-callback-with-delay`)**

`kernel.scm` already requires `helix/misc.scm` — verify it's there. Also add the helix commands require for `helix.redraw`:

At the top of `kernel.scm`, ensure these requires exist:

```scheme
(require "helix/misc.scm")
(require (prefix-in helix. "helix/commands.scm"))
```

- [ ] **Step 3: Update `kernel-get-for-notebook` to accept a callback**

Replace `kernel-get-for-notebook` (lines 104-108) with:

```scheme
;;@doc
;; Get or start a kernel for a notebook.
;; If a kernel is already running, calls (on-ready kernel-state) immediately.
;; Otherwise starts a new one asynchronously and calls on-ready when ready.
;; Returns #false if the kernel fails to start.
(define (kernel-get-for-notebook notebook-path lang on-ready)
  (define existing (hash-try-get *kernels* notebook-path))
  (if existing
      (on-ready existing)
      (kernel-start lang notebook-path on-ready)))
```

- [ ] **Step 4: Update `execute-cell` to use the callback-based kernel API**

In `plugin/nothelix/execution.scm`, the `execute-cell` function (lines 303-401) calls `kernel-get-for-notebook` synchronously and expects a return value. Convert to callback style.

Replace lines 346-401 (from `(define kernel-state ...` to end of function) with:

```scheme
  ;; Get kernel for this notebook (async — may need to start one)
  (define notebook-path (editor-document->path doc-id))

  (kernel-get-for-notebook notebook-path lang
    (lambda (kernel-state)
      (define kernel-dir (hash-get kernel-state 'kernel-dir))

      ;; Get cell index for dependency tracking
      (define cell-info-json (get-cell-at-line path current-line))
      (define cell-index-str (json-get cell-info-json "cell_index"))
      (define cell-index (if (> (string-length cell-index-str) 0)
                              (string->number cell-index-str)
                              0))

      ;; Insert output header with spinner
      (spinner-reset)
      (define spinner-frame (spinner-next-frame))
      (helix.static.insert_string (string-append "\n\n# ─── Output ───\n# " spinner-frame " Executing...\n"))
      (helix.static.commit-changes-to-history)
      (set-status! (string-append spinner-frame " Executing cell..."))
      (helix.redraw)

      ;; Track executing kernel for cancellation
      (set! *executing-kernel-dir* kernel-dir)

      ;; Start execution (non-blocking Rust FFI call)
      (define start-result (kernel-execute-cell-start kernel-dir cell-index code))
      (define start-status (json-get start-result "status"))

      (cond
        [(equal? start-status "started")
          (enqueue-thread-local-callback-with-delay 100
            (lambda () (poll-for-result kernel-dir path cell-index)))]
        [else
         (define err (let ([e (json-get start-result "error")]) (if (> (string-length e) 0) e "Unknown error")))
         (when (or (string-contains? err "does not exist")
                   (string-contains? err "PID file missing"))
           (set! *kernels* (hash-remove *kernels* notebook-path)))
         (helix.static.insert_string (string-append "# ERROR: " err "\n"))
         (helix.static.insert_string "# ─────────────\n")
         (set-status! (string-append "✗ " err))
         (helix.static.commit-changes-to-history)
         (set! *executing-kernel-dir* #false)
         (helix.redraw)])))
```

The early part of `execute-cell` (lines 303-345: cell boundary finding, code extraction, output section deletion, cursor positioning) stays the same. Only the kernel-get and onwards changes.

- [ ] **Step 5: Update `execute-all-cells` and `execute-cells-above` similarly**

In `execute-all-cells` (lines 432-484), replace the kernel-get pattern (lines 475-484):

```scheme
  ;; Start kernel (async)
  (kernel-get-for-notebook notebook-path lang
    (lambda (kernel-state)
      (define kernel-dir (hash-get kernel-state 'kernel-dir))
      (set-status! (string-append "Executing " (number->string cell-count) " cells: " indices-str))
      (execute-cell-list doc-id notebook-path kernel-dir path cell-indices cell-indices cell-count current-line)))
```

In `execute-cells-above` (lines 622-680), replace the kernel-get pattern (lines 670-680) the same way. Also remove the `sleep-ms 100` on line 636:

```scheme
  ;; IMPORTANT: Save file first so Rust can read the latest content
  (helix.write)
  ;; Use a delayed callback instead of blocking sleep to let the write flush
  (enqueue-thread-local-callback-with-delay 100
    (lambda ()
      ;; ... rest of the function body from line 639 onwards ...
    ))
```

- [ ] **Step 6: Build and verify**

Run: `cd /Users/koalazub/projects/nothelix && cargo build -p libnothelix 2>&1`
Expected: Compiles (Rust unchanged in this task, but verify).

- [ ] **Step 7: Commit**

```bash
git add plugin/nothelix/kernel.scm plugin/nothelix/execution.scm
git commit -m "fix: convert kernel startup to async polling

kernel-start no longer calls sleep-ms in a loop on the main thread.
Instead it uses enqueue-thread-local-callback-with-delay to poll for
the ready file, keeping the editor responsive during Julia startup.
execute-cells-above also replaces its blocking sleep-ms with a
delayed callback."
```

---

### Task 4: Cache spinner line position

`update-spinner-frame` scans the entire document from line 0 every 100ms looking for "Executing...". Cache the line number and only re-scan if the cached position is stale.

**Files:**
- Modify: `plugin/nothelix/execution.scm:254-286` (`update-spinner-frame`)
- Modify: `plugin/nothelix/execution.scm:289-299` (`poll-for-result`)

- [ ] **Step 1: Add spinner line cache and update the function**

Add a module-level cache variable near the top of `execution.scm` (after `*image-id-counter*`):

```scheme
;; Cached line number of the "Executing..." spinner, or #false if unknown.
(define *spinner-line-cache* #false)
```

Replace `update-spinner-frame` (lines 257-286) with:

```scheme
;;@doc
;; Advance the spinner animation in the "Executing..." line.
;; Uses a cached line position; falls back to a linear scan only when
;; the cached position is stale or unset.
(define (update-spinner-frame)
  (define focus (editor-focus))
  (define doc-id (editor->doc-id focus))
  (define rope (editor->text doc-id))
  (define total-lines (text.rope-len-lines rope))

  ;; Check cached position first; re-scan only if stale.
  (define spinner-line
    (if (and *spinner-line-cache*
             (< *spinner-line-cache* total-lines)
             (string-contains? (doc-get-line rope total-lines *spinner-line-cache*) "Executing..."))
        *spinner-line-cache*
        ;; Cache miss — scan from the end (spinner is usually near the bottom).
        (let scan ([idx (- total-lines 1)])
          (cond
            [(< idx 0) #false]
            [(string-contains? (doc-get-line rope total-lines idx) "Executing...") idx]
            [else (scan (- idx 1))]))))

  (set! *spinner-line-cache* spinner-line)

  (when spinner-line
    (define new-frame (spinner-next-frame))
    (helix.goto (number->string (+ spinner-line 1)))
    (helix.static.goto_line_start)
    (helix.static.extend_to_line_bounds)
    (helix.static.delete_selection)
    (helix.static.insert_string (string-append "# " new-frame " Executing...\n"))
    (helix.static.collapse_selection)
    (helix.static.commit-changes-to-history)
    (set-status! (string-append new-frame " Executing cell..."))
    (helix.redraw)))
```

Key improvements:
- Checks the cached line first (O(1) hit)
- On cache miss, scans from the **bottom** (spinner is always near the end)
- Caches the found position for next tick

- [ ] **Step 2: Reset the cache when execution starts and finishes**

In `execute-cell`, right before inserting the spinner header (in the callback, after `(spinner-reset)`), add:

```scheme
(set! *spinner-line-cache* #false)
```

In `update-cell-output` (line 147), add at the start:

```scheme
(set! *spinner-line-cache* #false)
```

- [ ] **Step 3: Increase polling interval with backoff**

Replace `poll-for-result` (lines 289-299) with:

```scheme
;; Helper: Poll for execution result with exponential backoff.
;; Starts at 100ms, grows to 500ms max.
(define (poll-for-result kernel-dir jl-path cell-index)
  (poll-for-result-with-delay kernel-dir jl-path cell-index 100))

(define (poll-for-result-with-delay kernel-dir jl-path cell-index delay-ms)
  (define result-json (kernel-poll-result kernel-dir))
  (define status (json-get result-json "status"))

  (cond
    [(equal? status "pending")
     (update-spinner-frame)
     (define next-delay (min 500 (+ delay-ms 50)))
     (enqueue-thread-local-callback-with-delay next-delay
       (lambda () (poll-for-result-with-delay kernel-dir jl-path cell-index next-delay)))]
    [else
     (update-cell-output result-json jl-path cell-index)]))
```

Do the same for `poll-cell-list-result` (lines 601-613):

```scheme
(define (poll-cell-list-result doc-id notebook-path kernel-dir jl-path cell-idx cell-indices remaining-indices total-count original-line)
  (poll-cell-list-result-with-delay doc-id notebook-path kernel-dir jl-path cell-idx cell-indices remaining-indices total-count original-line 100))

(define (poll-cell-list-result-with-delay doc-id notebook-path kernel-dir jl-path cell-idx cell-indices remaining-indices total-count original-line delay-ms)
  (define result-json (kernel-poll-result kernel-dir))
  (define status (json-get result-json "status"))

  (cond
    [(equal? status "pending")
     (update-spinner-frame)
     (define next-delay (min 500 (+ delay-ms 50)))
     (enqueue-thread-local-callback-with-delay next-delay
       (lambda () (poll-cell-list-result-with-delay doc-id notebook-path kernel-dir jl-path cell-idx cell-indices remaining-indices total-count original-line next-delay)))]
    [else
     (update-cell-output result-json jl-path cell-idx)
     (enqueue-thread-local-callback-with-delay 10
       (lambda () (execute-cell-list doc-id notebook-path kernel-dir jl-path cell-indices remaining-indices total-count original-line)))]))
```

- [ ] **Step 4: Commit**

```bash
git add plugin/nothelix/execution.scm
git commit -m "fix: cache spinner line position and add polling backoff

Spinner updates no longer scan the entire document from line 0 every
100ms. The line position is cached and checked first. Polling backoff
grows from 100ms to 500ms to reduce load during long executions."
```

---

### Task 5: Move conceal computation to a background thread

The conceal FFI call (`compute-conceal-overlays-ffi` / `compute-conceal-overlays-for-comments`) runs on the main thread and blocks the editor. Move it to a `spawn-native-thread` and deliver results via `hx.with-context`.

**Files:**
- Modify: `plugin/nothelix/conceal.scm`
- Modify: `plugin/nothelix.scm:73-81` (`conceal-math`)

- [ ] **Step 1: Add async conceal in `conceal.scm`**

Add `helix/ext.scm` require at the top of `conceal.scm` (after the existing requires):

```scheme
(require "helix/ext.scm")
(require "helix/misc.scm")
```

Add a new function after `compute-conceal-overlays`:

```scheme
;;;@doc
;;; Compute conceal overlays on a background thread and apply them
;;; when ready. The editor remains responsive during computation.
(define (compute-and-apply-conceal-async)
  (define focus (editor-focus))
  (define doc-id (editor->doc-id focus))
  (define rope (editor->text doc-id))
  (define text (text.rope->string rope))
  (define path (editor-document->path doc-id))
  (define is-jl (and path (ends-with-jl? path)))

  ;; Spawn background thread for the heavy FFI work.
  (spawn-native-thread
    (lambda ()
      (define json-str
        (if is-jl
            (compute-conceal-overlays-for-comments text)
            (compute-conceal-overlays-ffi text)))

      ;; Parse overlays on the background thread too (no editor access needed).
      (define overlays (parse-overlay-json json-str))

      ;; Deliver results to the main thread.
      (hx.with-context
        (lambda ()
          (if (null? overlays)
              (clear-overlays!)
              (begin
                (set-overlays! overlays)
                (set-status! (string-append "nothelix: " (number->string (length overlays)) " overlays")))))))))

;;; Parse the JSON overlay string into a list of (offset . replacement) pairs.
;;; This is pure computation — safe to run on any thread.
(define (parse-overlay-json json-str)
  (if (string=? json-str "[]")
      '()
      (let parse-loop ([pos 0] [result '()])
        (cond
          [(>= pos (string-length json-str)) (reverse result)]
          [(char=? (string-ref json-str pos) #\{)
           (let* ([colon1-pos (find-char json-str #\: (+ pos 1))]
                  [offset-start (skip-whitespace json-str (+ colon1-pos 1))]
                  [offset-end (find-non-digit json-str offset-start)]
                  [offset-val (string->number (substring json-str offset-start offset-end))]
                  [colon2-pos (find-char json-str #\: offset-end)]
                  [quote1-pos (find-char json-str #\" (+ colon2-pos 1))]
                  [replacement-str (extract-json-string json-str (+ quote1-pos 1))]
                  [after-str (+ quote1-pos 1 (json-string-raw-length json-str (+ quote1-pos 1)) 1)]
                  [close-pos (find-char json-str #\} after-str)])
             (parse-loop (+ close-pos 1)
                         (cons (cons offset-val replacement-str) result)))]
          [else (parse-loop (+ pos 1) result)]))))
```

Update the `provide` line to export the new function:

```scheme
(provide compute-conceal-overlays compute-and-apply-conceal-async
         find-math-regions build-overlays-for-region parse-overlay-json)
```

- [ ] **Step 2: Update `conceal-math` to use async path**

In `plugin/nothelix.scm`, replace `conceal-math` (lines 75-81):

```scheme
;;@doc
;; Apply LaTeX math concealment to the current buffer.
;; Runs computation on a background thread to avoid blocking the editor.
(define (conceal-math)
  (set-status! "nothelix: computing overlays...")
  (compute-and-apply-conceal-async))
```

- [ ] **Step 3: Commit**

```bash
git add plugin/nothelix/conceal.scm plugin/nothelix.scm
git commit -m "feat: move conceal computation to background thread

compute-and-apply-conceal-async uses spawn-native-thread for the
heavy FFI call and JSON parsing, then delivers overlays to the main
thread via hx.with-context. The editor stays responsive during
LaTeX parsing."
```

---

### Task 6: Move image cache loading to a background thread

`render-cached-images` loads images from disk synchronously for every `# @image` marker. Move the file I/O to a background thread.

**Files:**
- Modify: `plugin/nothelix/execution.scm:689-723` (`render-cached-images`)

- [ ] **Step 1: Add async image rendering**

Add `helix/ext.scm` require at the top of `execution.scm` (if not already present):

```scheme
(require "helix/ext.scm")
```

Replace `render-cached-images` (lines 689-723) with:

```scheme
;;@doc
;; Scan the current buffer for `# @image <path>` markers and re-render
;; the cached images via RawContent. Runs file I/O on a background thread.
(define (render-cached-images)
  (define focus (editor-focus))
  (define doc-id (editor->doc-id focus))
  (define path (editor-document->path doc-id))

  (when (and path (string-suffix? path ".jl"))
    (define rope (editor->text doc-id))
    (define total-lines (text.rope-len-lines rope))

    ;; Collect image markers and their line info on the main thread (fast).
    (define markers
      (let loop ([line-idx 0] [acc '()])
        (if (>= line-idx total-lines) (reverse acc)
            (let ([line (doc-get-line rope total-lines line-idx)])
              (if (string-starts-with? line "# @image ")
                  (let ([rel-path (string-trim (substring line 9 (string-length line)))])
                    (loop (+ line-idx 1) (cons (list line-idx rel-path) acc)))
                  (loop (+ line-idx 1) acc))))))

    (when (not (null? markers))
      ;; Load images on a background thread.
      (spawn-native-thread
        (lambda ()
          ;; Load all images from disk (file I/O, no editor access needed).
          (define loaded
            (map (lambda (marker)
                   (define line-idx (car marker))
                   (define rel-path (car (cdr marker)))
                   (define image-b64 (load-image-from-cache path rel-path))
                   (list line-idx rel-path image-b64))
                 markers))

          ;; Deliver to main thread for rendering.
          (hx.with-context
            (lambda ()
              (define current-rope (editor->text doc-id))
              (for-each
                (lambda (entry)
                  (define line-idx (car entry))
                  (define image-b64 (car (cdr (cdr entry))))
                  (when (> (string-length image-b64) 0)
                    (define image-id *image-id-counter*)
                    (set! *image-id-counter* (+ *image-id-counter* 1))
                    (when (> *image-id-counter* 16777200)
                      (set! *image-id-counter* 1))
                    (define image-rows 12)
                    (define escape-seq (kitty-display-image-bytes image-b64 image-id image-rows))
                    (when (not (string-starts-with? escape-seq "ERROR:"))
                      (define char-pos (text.rope-line->char current-rope line-idx))
                      (helix.static.add-raw-content! escape-seq image-id image-rows char-pos))))
                loaded))))))))
```

- [ ] **Step 2: Commit**

```bash
git add plugin/nothelix/execution.scm
git commit -m "feat: load cached images on background thread

render-cached-images now collects image markers on the main thread,
loads file data on a background thread via spawn-native-thread, and
renders via hx.with-context. No more blocking file I/O on the main
thread during buffer switch."
```

---

### Task 7: Final verification and cleanup

- [ ] **Step 1: Build the Rust library**

Run: `cd /Users/koalazub/projects/nothelix && cargo build -p libnothelix 2>&1`
Expected: Clean compilation.

- [ ] **Step 2: Verify no remaining `sleep-ms` calls on the main thread**

Search all `.scm` files for `sleep-ms`:

Run: `grep -rn "sleep-ms" plugin/`

Expected: No occurrences in code paths that run on the main thread. (It's OK if `sleep-ms` appears in imports or in background thread code.)

- [ ] **Step 3: Verify all async patterns are correct**

Check that:
- Every `spawn-native-thread` lambda does NOT call any `helix.static.*`, `editor-focus`, `editor->doc-id`, or `set-status!` directly — only through `hx.with-context` or `hx.block-on-task`
- Every `hx.with-context` callback does NOT do file I/O or call `sleep-ms`
- Every `enqueue-thread-local-callback-with-delay` callback is fast (no loops over all lines without caching)

- [ ] **Step 4: Commit final state**

```bash
git add -A
git commit -m "chore: final cleanup after async architecture migration"
```
