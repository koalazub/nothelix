# Output rendering architecture — design (foundation: undo isolation)

## Problem

Executing a cell (or all cells) pollutes the user's undo history: after editing
a variable definition and running the cell, undoing the edit takes several `u`
presses because execution inserts output as real buffer text committed to
history.

Root cause (verified): execution calls `commit-changes-to-history` **3× per
cell** — delete prior output (`cell-boundaries.scm:150-155`), insert the
`# ─── Output ───` header (`execution.scm:139-140`), insert the output body
(`output-insert.scm:238-239`). The save-hook that runs on the implicit `:w`
inside `with-saved-notebook` adds up to 3 more revisions
(`renumber-cells!`, `format-math-buffer`, display-math reserve). Nothing
coalesces, so one edit-then-run costs 4–7 undo revisions;
`:execute-all-cells` costs ~2–3 per cell.

The user's requirement: **fundamental, not patchwork** — output should not be
part of the edited buffer at all.

## Key enabling findings (verified against source)

1. **The inline output block is display-only, not the persistence format.**
   `.jl` parse discards it (`libnothelix/src/notebook/cells.rs:312-317`);
   `.ipynb` conversion sources outputs from the original notebook (content
   matched) + index-keyed sidecar PNGs, never from buffer text
   (`convert.rs:176,399-401`). Moving output out of the buffer does **not**
   break the `.ipynb` round-trip.
2. **The fork already renders non-buffer content with zero undo cost.** Four
   display primitives store content outside `self.text` and never build a
   Transaction: plugin overlays (grapheme replace), style-overlays (theme-scope
   highlight over a char range), raw-content (Kitty images), and **math-lines**
   — a true `LineAnnotation` (`helix-view/src/annotations/math.rs:36`,
   `insert_virtual_lines` → `rows_to_reserve_after(doc_line)`) that reserves
   real *display* rows around a source line with **no backing buffer graphemes**,
   correctly accounted for by scroll/cursor. Steel API:
   `set-math-lines-above!/below!`, `clear-all-math-lines!`.
3. **Buffer-text edits cannot bypass undo through the current Steel API.** Every
   edit FFI goes through `Document::apply` → `append_changes_to_history`
   (`document.rs:1751,1910`). Making programmatic edits non-undoable requires a
   new fork primitive.
4. **Images need real height today; there is no image-into-virtual-rows
   painter.** The fork deliberately removed phantom-row reservation for
   raw-content (`doc_formatter.rs:484-500`, `let _ = raw_content;`);
   `draw_raw_content` paints over real buffer grapheme rows
   (`ui/document.rs:364-431`). So image height still comes from real blank
   buffer lines. (This is why Option C — fully virtual images — is rejected:
   its painter does not exist and its predecessor was removed for scroll/cursor
   bugs.)

Fork: `github:koalazub/helix` branch `feature/inline-image-rendering`, checked
out at `/Users/koalazub/projects/helix` (HEAD `b666137e`).

## Chosen architecture — Option B

A per-cell **out-of-buffer output store** becomes the source of truth. The
`# ─── Output ───` block leaves the buffer; the `.jl` holds only user code +
`@cell` markers. Output renders as:

- **Text** (stdout / stderr / result repr / errors) → **virtual rows** via a
  styled `LineAnnotation` (the math-lines vehicle, extended to carry per-span
  style). Zero buffer text → structurally invisible to undo.
- **Raster images** → the **proven** raw-content path (unchanged painter), but
  the real blank reserve lines are applied through a new **non-undoable/tagged
  edit** primitive so they never enter the user's undo ring.

Net undo: **one `u` = one code edit; execution contributes nothing.**

### Non-goals (scoped out of this foundation spec)

- **Multiple graphs per cell** — spec 2. The store holds an *ordered* outputs
  array so this becomes "k display_data → k rasters" with no rework.
- **Colored braille text-plots** — spec 3. The styled `LineAnnotation` built
  here (per-span style) is the exact renderer a braille plot reuses.
- **Image-into-virtual-rows (Option C)** — rejected; images keep real reserve
  lines (made non-undoable), not virtual rows.
- **Removing the save-hook amplifier entirely** — this spec routes the *output*
  edits out of undo; folding `renumber-cells!`/`format-math`/display-math
  reserve into the non-undoable path is a follow-up (noted, not built here).

## Components

### 1. Fork (`/Users/koalazub/projects/helix`)

**1a. Styled multi-row line annotation ("output-lines").** Extend the
math-lines mechanism so a reserved virtual row can carry **per-span style**
(today `MathAnnotations` paints one decoration style for the whole line,
`math_annotations.rs`). Two viable shapes — decide in the plan:
- extend `MathLines` entries from `Vec<String>` to `Vec<Vec<StyledSpan>>`
  where `StyledSpan = { text, style_scope: Option<String> }`, or
- add a sibling `doc.output_lines` store of the same `LineAnnotation` class.
Steel API: `set-output-lines-below!(line, rows)` / `clear-output-lines!`,
where `rows` is a list of lines, each a list of `(text scope-or-false)` spans.
Colors resolve against the active theme at render (same mechanism as
`plugin_style_highlights`). Like math-lines, entries are keyed by `usize`
source line and are **not** position-remapped in `apply_impl`, so they must be
refreshed on edit via the existing reconceal/edit hook.

**1b. Non-undoable edit primitive.** Add a way to apply buffer edits that the
undo system does not surface to the user. Preferred: **tagged/skippable
revisions** — edits commit to `History` but carry a `plugin-output` tag;
`undo`/`redo`/`earlier`/`later` auto-step past tagged-only revisions
(`document.rs:1788-1897`), so `u` never lands on an output state and, after
undoing a code edit, continues past the output that edit produced. This keeps
`History`'s position-based inversions consistent (a raw no-history apply risks
desync and is the fallback only if tagging proves impractical). Steel API:
`with-plugin-output-edits(thunk)` or a `commit-changes-to-history/tagged`
variant. Used for the image reserve blank lines.

### 2. libnothelix — output store (Rust)

A new `output_store` module (shape mirrors `trust.rs`/`resume.rs`): the source
of truth per cell, keyed by the nbformat **cell id** the code already stamps
(`convert.rs:91-98,409-420`) plus a **captured-source hash** for staleness.
Holds the ordered nbformat outputs array (`stream`/`execute_result`/
`display_data`/`error`) + `execution_count`. Persists to the **global** state
dir — `~/.local/share/nothelix/outputs/<workspace-id>/<cellid>.json` — to
avoid adding any project dotfile (same precedent as trust/resume; the user's
standing "no project `.spam` files" rule). Configurable to project-local if a
user wants output to travel with a clone. FFI: `output-store-put(workspace,
cellid, source-hash, json)`, `output-store-get(workspace, cellid) -> json`,
`output-store-clear(workspace, cellid)`.

Staleness: on reopen, if the stored `source-hash` ≠ the cell's current source
hash, the store entry is treated as stale (output not shown / marked stale),
matching today's drop-stale-when-edited behavior (`mod.rs:353-453`).
Duplicate-source cells (`mod.rs:1444-1452`) fall back to positional
disambiguation within the duplicate group.

### 3. nothelix plugin (Steel) — re-plumb

- `execution.scm` / `output-insert.scm`: on execution, write the captured
  result to the output store (by cell id) instead of inserting the
  `# ─── Output ───` text block. Render from the store: text → `set-output-lines-below!`
  on the cell's last code line; images → existing raw-content path with reserve
  lines applied via the tagged primitive.
- **Re-execution** keys off **cell id**, not the `─── Output ───` sentinel:
  clear the cell's output-lines annotation + its raw-content id band + its store
  entry, then re-render. No text delete/insert of an output block.
- **Reopen** (`document-opened` hook): for each cell with a fresh (non-stale)
  store entry, render its output-lines + images from the store. Replaces the
  current "scan `# @image` markers + count blank padding" reopen path for text;
  images still reserve rows (via the tagged primitive) sized from the store's
  recorded height.
- Row cap: reserved rows for large stdout / tall output are capped
  (`*plot-max-rows*`, existing = 60; add `*text-max-rows*`) and scroll within
  the region, since `rows_to_reserve_after` is otherwise unbounded.

## Data flow

```
execute cell
  -> kernel captures nbformat outputs (unchanged: output_capture.jl)
  -> plugin writes store[cellid] = {outputs, exec_count, source_hash}
  -> render:
       text/errors -> set-output-lines-below!(codeLine, styledRows)   [no undo]
       image       -> reserve height blank lines via tagged edit       [no user undo]
                      + add-raw-content-with-placeholders! (existing)   [no undo]
  -> undo history: unchanged (only the user's prior code edit)

reopen (.jl)
  -> for each cell: store[cellid] fresh? -> render output-lines + images
  -> stale (source changed) -> skip / mark stale

re-execute
  -> clear-output-lines!(cellId) + clear raw-content id band + store.clear(cellId)
  -> re-render from fresh capture
```

## Persistence & `.ipynb`

- `.jl` on disk: **code + `@cell` markers only.** No output text, no
  `# ─── Output ───`, no `# @image` markers. Cleaner code diffs (no output
  churn), no stale-on-reopen text.
- Output durability: the global store (survives reopen on the same machine);
  the `.ipynb` remains the portable record via `:sync-to-ipynb`.
- `.ipynb` round-trip: **unaffected** — conversion already ignores the buffer
  block and sources outputs from the `.ipynb` + sidecars. Keying the store on
  the nbformat cell id makes reattachment more robust than positional matching.

## Undo behavior (the deliverable)

- Text/braille output: **no history entries at all** (virtual rows).
- Image reserve lines: committed as **tagged** revisions that `u` steps past.
- Result: after edit-then-run, a single `u` reverts the code edit; execution
  adds nothing the user must undo. `:execute-all-cells` adds nothing per cell.

## Error handling

- Store read/write failures are best-effort: a failed `output-store-put` logs
  to status and the cell still renders from the in-memory capture; a failed
  `output-store-get` on reopen simply shows no output (never blocks open) —
  consistent with the resume feature's best-effort contract.
- Missing/renamed cell id → treat as no stored output (no crash).
- Fork primitive absent (older hx) → deferred-eval guard like
  `try-set-style-overlays!` (`markdown-render.scm:18-21`): fall back to the
  current in-buffer insertion so the plugin still loads/works on an un-bumped
  binary.

## Testing

- **libnothelix `output_store`**: unit tests mirroring `trust.rs` — put/get
  roundtrip, per-cell isolation, source-hash staleness, missing entry → empty,
  malformed file skipped, tempdir seam.
- **Fork**: Rust tests for the styled `LineAnnotation` row reservation
  (`rows_to_reserve_after`) and the tagged-revision skip logic in
  `undo_redo_impl`/`earlier_later_impl`.
- **Plugin**: headless load gate (`just check`) that the new modules load and
  FFI symbols resolve; a headless driver (per `reference_headless_steel_tests`)
  that executes a fixture cell, asserts the output renders (annotation present)
  and the undo depth after edit+run is exactly 1.
- **Manual smoke** (needs live hx): edit a var, run cell, one `u` reverts the
  edit; reopen shows output; re-run replaces it.

## Rollout / sequencing

This is **spec 1 of 3** (undo isolation foundation). Spec 2 = multiple raster
graphs per cell (ordered store array → k rasters). Spec 3 = colored braille
text-plots (styled output-lines with per-span color). Both build on the store
+ styled-annotation substrate created here, with no rework.

The fork changes ship in `/Users/koalazub/projects/helix`, then the `flake.nix`
`helix-fork` input is bumped and the hx binary rebuilt (nixoala); the
libnothelix FFI version bumps (both handshake sites) and `just install` rebuilds
the dylib.
