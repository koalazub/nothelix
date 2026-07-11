# Image output out of undo — design (Plan 2 of the output-architecture epic)

## Problem

Plan 1 took cell **text** output out of the undo history (virtual rows). Cell
**images** still reserve real blank buffer lines below the `# @image` marker,
and those inserts commit to history (`output-insert.scm` image branch +
trailing `commit-changes-to-history`). So executing a cell that produces a plot
still adds undo revisions. This spec removes that residual.

## Why images can't just become virtual rows

The fork deliberately removed phantom-row reservation for raw-content images
(`helix-core/src/doc_formatter.rs:484-500`, `let _ = raw_content;`);
`draw_raw_content` paints Kitty placeholder cells **over real buffer grapheme
rows** (`helix-term/src/ui/document.rs:364-431`). There is no painter that draws
an image into a `LineAnnotation`'s virtual rows, and the predecessor that tried
was removed for scroll/cursor bugs. So an image still needs real blank buffer
lines for its height. The only way to keep those lines out of the user's undo is
to apply them through a **history edit the undo system does not surface to the
user** — a tagged/skippable revision.

## Chosen mechanism — tagged (skippable) revisions

helix `History` (`helix-core/src/history.rs:51`) is a revision tree: each
`Revision` has `parent`, `last_child`, `transaction`, `inversion`;
`undo()` moves `current` to `parent`, `redo()` to `last_child`. Add an optional
tag so plugin-output revisions are auto-skipped by user undo/redo:

### Fork changes (`/Users/koalazub/projects/helix`)

1. **`Revision` gains a tag** (`history.rs:58`): add `output: bool` (default
   false). `commit_revision`/`commit_revision_at_timestamp` gain a variant (or
   param) that sets it true. Non-output commits stay `false` — zero behavior
   change for existing callers.
2. **Skip on undo/redo** (`history.rs` `undo`/`redo` + the document-level
   `undo_redo_impl` at `document.rs:1790`): a user `undo` must revert the
   current revision AND continue reverting while the revision it lands on is
   `output`-tagged, stopping after the first non-output (user) revision — so one
   `u` reverts a code edit *and* the output edit it produced, never landing on
   an output-only state. Symmetrically for `redo`. Implement by composing the
   inversions of the skipped-through revisions (History already composes via
   `changes_since`), or by looping `undo_redo_impl` while the newly-current
   revision is tagged. `earlier`/`later` (`document.rs:1875`, time/count jumps)
   must treat tagged revisions as non-stops too.
3. **`changes_since` unaffected** — it composes transactions between revisions
   regardless of tag, so LSP sync stays correct (the tagged edits are real
   buffer changes, just not user-undo-stops).
4. **Steel FFI**: a way to apply the pending buffer changes as a tagged commit —
   e.g. `commit-changes-to-history/output` (mirrors the existing
   `commit-changes-to-history` binding at `steel/mod.rs`), or a
   `with-output-edits(thunk)` wrapper that tags whatever the thunk commits.

### Plugin changes

- The image branch in `output-insert.scm` applies its blank reserve-line inserts
  and the `# @image` marker via the tagged commit instead of the plain
  `commit-changes-to-history`. On re-execution, the reserve-line deletion
  (image clearing) is likewise tagged.
- Deferred-eval guard (`try-commit-output-edits!`) mirroring
  `output-render.scm`, so the plugin still works on an hx without the tagged
  binding (falls back to the plain commit = current behavior).

## Undo behavior (deliverable)

After this: a cell producing a plot adds **zero** user-undo revisions. Combined
with Plan 1 (text) and the save-hook coalescing, edit-then-execute is a single
`u` regardless of output kind.

## Error handling

- Missing tagged-commit binding (older hx) → deferred guard falls back to the
  plain commit; images still render, just as an undo step (Plan-1 behavior).
- The tag is metadata only; a malformed tag can never corrupt the transaction
  (the ChangeSet is unchanged).

## Testing

- **Fork Rust tests** (`history.rs` test module): commit a user revision, then a
  tagged revision, assert `undo()` returns to the pre-user state in one logical
  step (skips the tag); redo restores both; `earlier`/`later` skip tags;
  `changes_since` is tag-agnostic. This is the critical correctness surface.
- **Manual, in a running editor (REQUIRED — not skippable):** the undo/redo
  state machine cannot be fully validated by static review. Drive real
  undo/redo/earlier/later sequences interleaving user edits and plot execution,
  and confirm no undo state is ever a bare output-only buffer and no document
  desync occurs.

## Risk note

This is a core-editor undo-History change; its failure mode is undo corruption
or document desync — **data-loss class**. It MUST be verified in a running
editor before merge; diff review alone is insufficient for a history state
machine. Sequence this as its own reviewed+live-verified change, not stacked
blind on other unverified work.
