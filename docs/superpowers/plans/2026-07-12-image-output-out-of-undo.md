# Image Output Out of Undo — Implementation Plan (Plan 2 of the output-architecture epic)

> **For agentic workers:** REQUIRED SUB-SKILL: superpowers:subagent-driven-development or superpowers:executing-plans. Steps use checkbox (`- [ ]`) syntax.

**Goal:** Cell plot rendering (the real blank reserve lines images need) no longer adds user-undo revisions, via tagged/skippable revisions in the Helix fork's undo history.

**Architecture:** Add an `output: bool` tag to `Revision`. `undo_user`/`redo_user` walk to the target revision (skipping output-tagged, stopping after one user revision) and delegate to the existing, tested `jump_to`. Non-notebook documents never create tagged revisions, so undo/redo behavior is byte-identical for them — safe editor-wide. The plugin commits image reserve-line edits via a tagged FFI.

**Tech Stack:** Rust (Helix fork `~/projects/helix`, `cargo test`), Steel plugin.

## Global Constraints

- No code comments except `//!`/`///`/`;;@doc`.
- Fork changes land in `/Users/koalazub/projects/helix` (branch `feature/inline-image-rendering`, uses **git**); after them, a fork-input bump + hx rebuild (user step). The plugin uses a deferred-eval guard so it still loads on an hx without the tagged FFI (falls back to the plain commit = current behavior).
- **DATA-LOSS RISK:** this is core undo-history logic. The fork Task's Rust unit tests are the mandatory automated gate; live undo/redo testing in a running editor is REQUIRED before merge (diff review alone is insufficient for a history state machine).
- Non-notebook editor undo/redo MUST stay byte-identical (no doc ever has tagged revisions unless the nothelix plugin tags them).

---

### Task 1: Fork — tagged (skippable) undo revisions

**Files (in `/Users/koalazub/projects/helix`):**
- Modify `helix-core/src/history.rs` (`Revision:58`, `Default:72`, `commit_revision:85`, add `undo_user`/`redo_user`, tests).
- Modify `helix-view/src/document.rs` (`undo`/`redo` at ~1816/1821 to use the `_user` variants; add `append_changes_to_history_tagged` near `append_changes_to_history:1912`).

**Interfaces:** Produces `History::commit_revision_tagged(txn, orig, output: bool)`, `History::undo_user()->Option<Vec<Transaction>>`, `History::redo_user()->Option<Vec<Transaction>>`; `Document::append_changes_to_history_tagged(view)`.

- [ ] **Step 1: Write the failing Rust tests** (in `history.rs`'s `#[cfg(test)] mod test`):

```rust
#[test]
fn undo_user_skips_output_revisions() {
    use crate::Rope;
    let mut history = History::default();
    let doc = Rope::from("");
    let mut state = State { doc: doc.clone(), selection: Selection::point(0) };

    // user edit A (untagged)
    let t_a = Transaction::change(&state.doc, [(0, 0, Some("A".into()))].into_iter());
    history.commit_revision(&t_a, &state);
    t_a.apply(&mut state.doc);
    // output edit B (tagged)
    let t_b = Transaction::change(&state.doc, [(1, 1, Some("B".into()))].into_iter());
    history.commit_revision_tagged(&t_b, &state, true);
    t_b.apply(&mut state.doc);

    // one undo_user reverts BOTH B (output) and A (user) -> back to ""
    let txns = history.undo_user().expect("something to undo");
    for t in &txns { t.apply(&mut state.doc); }
    assert_eq!(state.doc.to_string(), "");
    assert!(history.at_root());
}

#[test]
fn undo_user_stops_after_one_user_revision() {
    use crate::Rope;
    let mut history = History::default();
    let mut state = State { doc: Rope::from(""), selection: Selection::point(0) };
    for ch in ["A", "B"] {
        let len = state.doc.len_chars();
        let t = Transaction::change(&state.doc, [(len, len, Some(ch.into()))].into_iter());
        history.commit_revision(&t, &state);
        t.apply(&mut state.doc);
    }
    // "AB"; one undo_user reverts only "B" (both untagged, stops after one)
    let txns = history.undo_user().unwrap();
    for t in &txns { t.apply(&mut state.doc); }
    assert_eq!(state.doc.to_string(), "A");
}

#[test]
fn redo_user_restores_user_edit_and_following_output() {
    use crate::Rope;
    let mut history = History::default();
    let mut state = State { doc: Rope::from(""), selection: Selection::point(0) };
    let t_a = Transaction::change(&state.doc, [(0, 0, Some("A".into()))].into_iter());
    history.commit_revision(&t_a, &state); t_a.apply(&mut state.doc);
    let t_b = Transaction::change(&state.doc, [(1, 1, Some("B".into()))].into_iter());
    history.commit_revision_tagged(&t_b, &state, true); t_b.apply(&mut state.doc);
    // undo both, then one redo_user restores "AB"
    for t in &history.undo_user().unwrap() { t.apply(&mut state.doc); }
    assert_eq!(state.doc.to_string(), "");
    let txns = history.redo_user().unwrap();
    for t in &txns { t.apply(&mut state.doc); }
    assert_eq!(state.doc.to_string(), "AB");
}

#[test]
fn untagged_history_undo_user_equals_undo() {
    // A document that never tags behaves exactly like plain undo (one revision).
    use crate::Rope;
    let mut history = History::default();
    let mut state = State { doc: Rope::from(""), selection: Selection::point(0) };
    let t = Transaction::change(&state.doc, [(0, 0, Some("X".into()))].into_iter());
    history.commit_revision(&t, &state); t.apply(&mut state.doc);
    let txns = history.undo_user().unwrap();
    assert_eq!(txns.len(), 1);
    for t in &txns { t.apply(&mut state.doc); }
    assert_eq!(state.doc.to_string(), "");
}
```

Read the existing `history.rs` test module for the exact `State`/`Transaction`/`Selection` construction idiom and match it (the snippets above are the intent; adapt to the real test helpers).

- [ ] **Step 2: Run — expect FAIL** (`cd /Users/koalazub/projects/helix && cargo test -p helix-core history::` → `commit_revision_tagged`/`undo_user`/`redo_user` not found).

- [ ] **Step 3: Implement** in `history.rs`:

Add the tag field to `Revision` (line 58) — `output: bool,` — set `output: false` in `Default` (line 72) and in `commit_revision_at_timestamp` (line 102). Then:

```rust
    pub fn commit_revision_tagged(&mut self, transaction: &Transaction, original: &State, output: bool) {
        self.commit_revision_at_timestamp(transaction, original, Instant::now());
        if let Some(rev) = self.revisions.last_mut() {
            rev.output = output;
        }
    }

    /// Undo revisions up to and including the first non-output revision,
    /// skipping output-tagged revisions. Returns the composed inversions
    /// (empty None if already at root). Reuses `jump_to`.
    pub fn undo_user(&mut self) -> Option<Vec<Transaction>> {
        if self.at_root() {
            return None;
        }
        let mut node = self.current;
        loop {
            let output = self.revisions[node].output;
            node = self.revisions[node].parent;
            if !output || node == 0 {
                break;
            }
        }
        Some(self.jump_to(node))
    }

    /// Redo one non-output revision plus any output revisions that follow it,
    /// stopping before the next non-output revision. Reuses `jump_to`.
    pub fn redo_user(&mut self) -> Option<Vec<Transaction>> {
        let mut node = self.current;
        let mut target = None;
        let mut seen_user = false;
        loop {
            let child = match self.revisions[node].last_child {
                Some(c) => c.get(),
                None => break,
            };
            let child_output = self.revisions[child].output;
            if seen_user && !child_output {
                break;
            }
            node = child;
            target = Some(node);
            if !child_output {
                seen_user = true;
            }
        }
        target.map(|t| self.jump_to(t))
    }
```

- [ ] **Step 4: Run — expect PASS** (`cargo test -p helix-core history::`).

- [ ] **Step 5: Wire the document layer** — in `helix-view/src/document.rs`:
  - `undo` (~1816) and `redo` (~1821): change `undo_redo_impl` (or the `undo`/`redo` methods) to apply `history.undo_user()`/`redo_user()` (a `Vec<Transaction>` — apply each in order) instead of the single `history.undo()`/`redo()`. Read `undo_redo_impl:1790` and adapt it to iterate the Vec. Non-tagged docs get a 1-element Vec → identical behavior.
  - Add `append_changes_to_history_tagged(&mut self, view: &mut View)` mirroring `append_changes_to_history:1912` but calling `history.commit_revision_tagged(&transaction, &old_state, true)`.
  - `cargo build` the fork clean.

- [ ] **Step 6: Commit (git)** — `cd /Users/koalazub/projects/helix && git add -A && git commit -m "feat(history): tagged (skippable) undo revisions for plugin output"`. Report the SHA.

---

### Task 2: Fork — Steel binding for tagged commit

**Files:** `helix-term/src/commands/engine/steel/mod.rs` (mirror the existing `commit-changes-to-history` binding).

**Interfaces:** Produces Steel `commit-output-changes-to-history!` → `doc.append_changes_to_history_tagged(view)`.

- [ ] **Step 1:** Find the existing `commit-changes-to-history` registration + its Rust glue (it calls `doc.append_changes_to_history(view)`). Mirror it as `commit-output-changes-to-history!` calling `append_changes_to_history_tagged`. `cargo build` clean.

- [ ] **Step 2: Commit (git)** — `git commit -m "feat(steel): commit-output-changes-to-history! binding"`. Report SHA. (Amend into Task 1's commit if preferred to keep the fork change one unit.)

---

### Task 3: Plugin — commit image reserve edits as tagged

**Files:** `plugin/nothelix/output-render.scm` (add a deferred wrapper); `plugin/nothelix/output-insert.scm` (image branch commit).

**Interfaces:** Consumes `commit-output-changes-to-history!`. Produces `try-commit-output-changes!` (deferred guard; falls back to plain `commit-changes-to-history` on an hx without the binding).

- [ ] **Step 1:** In `output-render.scm`, add mirroring the existing wrappers:

```scheme
(provide try-commit-output-changes!)
(define (try-commit-output-changes!)
  (with-handler
    (lambda (_) (helix.static.commit-changes-to-history))
    (eval '(helix.static.commit-output-changes-to-history!))))
```

- [ ] **Step 2:** In `output-insert.scm`, the image branch currently reserves blank lines + inserts the `# @image` markers and (with the rest of the cell) commits via `commit-changes-to-history`. Route the image-reserve buffer edits so they are committed via `try-commit-output-changes!` instead — so plots add no user-undo revision. Read the actual current commit structure post-multi-graph; if text output already commits nothing (Plan 1, virtual rows), then the only remaining execute-path buffer edits ARE the image reserve lines + markers, so the single trailing commit for a plot cell becomes the tagged one. Verify the non-image path still commits normally (or not at all, if it makes no buffer edits).

- [ ] **Step 3: Verify** — `just check` (after user hx rebuild + `just install`). Manual (live, REQUIRED): edit a var, run a cell that plots, one `u` reverts only the code edit (the plot's reserve lines are skipped); redo restores both; undo/redo on a plain (non-notebook) file is unchanged.

- [ ] **Step 4: Commit** — `jj describe @ -m "feat(output): commit plot reserve lines as tagged (out of user undo)"`.

---

## Self-Review

- Spec: tagged Revision + undo/redo skip (T1), tagged commit binding (T2), plugin uses it for image reserve (T3). ✓
- Placeholders: T1 carries full Rust for the tag + `undo_user`/`redo_user` + 4 unit tests; T3 shows the deferred wrapper. T1 Step 5 / T2 Step 1 / T3 Step 2 describe transformations of existing code with exact cited sites (adapt-to-real-code, since the surrounding helix/plugin code must be read) — acceptable for a core-editor change where the exact lines must be read.
- Type consistency: `commit_revision_tagged`/`undo_user`/`redo_user`/`append_changes_to_history_tagged` (T1) → `commit-output-changes-to-history!` (T2) → `try-commit-output-changes!` (T3).
- Safety: non-tagged docs get 1-element undo/redo Vecs → identical behavior (T1 test `untagged_history_undo_user_equals_undo`). Live verification REQUIRED (data-loss class).
