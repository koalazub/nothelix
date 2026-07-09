# Tweakable Parameters (`@param`) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Let a notebook author declare a numeric parameter in a cell via a trailing `# @param` annotation, nudge it with a keypress (instant literal edit + debounced single re-run), re-render the figure inline, and flag downstream cells stale via a dedicated, non-destructive virtual-row layer in the Helix fork.

**Architecture:** Pure-Steel parsing/nudge/stale-scan logic (unit-tested headlessly) drives the existing `execute-cell` re-run path; a new dedicated annotation layer in the Helix fork (mirroring the existing math-lines layer, but independent so the two never collide) renders the stale tags. No `libnothelix` FFI change — the new display primitives are fork builtins the plugin feature-probes, exactly like `math-render` probes `set-math-lines-*`.

**Tech Stack:** Rust (helix-view + helix-term fork crates, Steel `register_fn`), Steel/Scheme (the nothelix plugin), the existing `test-framework.scm` headless harness.

## Global Constraints

- **No `libnothelix` FFI change, no `NOTHELIX_FFI_VERSION` bump.** The stale-tag display primitives live in the Helix fork and are feature-probed by the plugin (mirror `math-render-ffi-available?`). If the fork builtins are absent, parameter nudging still works; only the stale tags silently no-op.
- **Numeric scalars only.** Grammar: `<lhs> = <literal>   # @param <lo>:<hi> [step <s>]`. `<lo>:<hi>` required; `step` optional (default `1` for int literals, `(hi−lo)/100` for float literals). No enums/lists/strings.
- **The buffer is the single source of truth.** A nudge rewrites the real Julia literal in place; the `# @param` comment is inert metadata. The cell runs identically without the plugin.
- **Re-run scope = the param's own cell only.** Downstream cells that reference a changed variable are *flagged* stale, never auto-rerun.
- **Tokenize, never regex** (project rule `feedback_no_regex_use_tokenization`).
- **Self-documenting code, minimal comments** (`feedback_no_code_comments`). The one allowed comment is the fork module's row-reservation layout header, mirroring `math_annotations.rs` — it documents a non-obvious rendering convention.
- **Clippy clean** across any crate touched (`feedback_clippy_clean`); no dead code (`feedback_no_dead_code`).
- **Default keys:** `]p` = `:param-up`, `[p` = `:param-down`, slotted under the existing `]`/`[` prefix maps in `notebook-bindings`.
- **The agent does NOT run `sudo darwin-rebuild`** (`feedback_darwin_rebuild_consent`) and does NOT run sandboxed release builds (`feedback_user_runs_install`). Fork rebuilds and `just install` are requested from the user. `cargo check`/`cargo test` on a single fork crate may be run directly.

## Running the Steel tests (headless)

There is no standalone `steel` binary; `hx` runs the Steel runtime (memory `reference_headless_steel_tests`). Use this reusable driver for every "run the Steel test" step. Save it once to the scratchpad:

`/private/tmp/claude-501/-Users-koalazub-projects-nothelix/1ab569fe-a1ee-45ce-8ced-99d05ff0b95c/scratchpad/run-steel-tests.exp`:

```tcl
#!/usr/bin/expect -f
set timeout 40
set helixscm "$env(HOME)/.config/helix/helix.scm"
set backup "$helixscm.nothelix-bak"
file copy -force $helixscm $backup
set f [open $helixscm w]
puts $f {(require "nothelix.scm")}
puts $f {(require "/Users/koalazub/projects/nothelix/plugin/tests/run-all-tests.scm")}
puts $f {(let ([out (open-output-file "/tmp/nothelix-steel-result.txt")])}
puts $f {  (parameterize ([current-output-port out]) (run-param-tweak-tests))}
puts $f {  (close-output-port out))}
close $f
spawn hx /tmp/nothelix-probe.jl
sleep 7
send "\033"
send ":q!\r"
expect eof
file copy -force $backup $helixscm
file delete $backup
```

Run with: `expect /private/tmp/claude-501/-Users-koalazub-projects-nothelix/1ab569fe-a1ee-45ce-8ced-99d05ff0b95c/scratchpad/run-steel-tests.exp && cat /tmp/nothelix-steel-result.txt`

A missing result file means the plugin failed to load (FFI mismatch or syntax error) — check `~/.cache/helix/helix.log`. The plugin is live-linked (memory `project_nixoala_plugin_linking`), so Scheme edits are picked up on each fresh `hx`.

---

# Phase A — Fork: dedicated stale-tag annotation layer

Work in `/Users/koalazub/projects/helix`. This mirrors the existing math-lines layer (`helix-view/src/annotations/math.rs`, `helix-term/src/ui/text_decorations/math_annotations.rs`) but is fully independent so stale tags and math rows never share storage or clearing.

### Task 1: `StaleTags` storage + view-side line-annotation wrapper

**Files:**
- Create: `helix-view/src/annotations/stale_tags.rs`
- Modify: `helix-view/src/annotations.rs` (add module declaration)

**Interfaces:**
- Produces: `StaleTags` with `is_empty(&self) -> bool`, `tag(&self, line_idx: usize) -> Option<&str>`, `set(&mut self, line_idx: usize, text: String)` (empty `text` removes), `clear_at(&mut self, line_idx: usize)`, `clear(&mut self)`, `rows_to_reserve_after(&self, doc_line: usize) -> usize`. Also a view-side `StaleTagAnnotations<'a>` implementing `LineAnnotation`, constructed by `StaleTagAnnotations::new(doc: &'a Document) -> Box<dyn LineAnnotation + 'a>`.

- [ ] **Step 1: Write the failing unit tests**

Append to the new file `helix-view/src/annotations/stale_tags.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::StaleTags;

    #[test]
    fn empty_by_default() {
        let t = StaleTags::default();
        assert!(t.is_empty());
        assert_eq!(t.tag(3), None);
        assert_eq!(t.rows_to_reserve_after(3), 0);
    }

    #[test]
    fn set_get_clear() {
        let mut t = StaleTags::default();
        t.set(5, "stale".to_string());
        assert!(!t.is_empty());
        assert_eq!(t.tag(5), Some("stale"));
        assert_eq!(t.rows_to_reserve_after(5), 1);
        t.clear_at(5);
        assert!(t.is_empty());
        assert_eq!(t.tag(5), None);
    }

    #[test]
    fn empty_text_removes_entry() {
        let mut t = StaleTags::default();
        t.set(2, "x".to_string());
        t.set(2, String::new());
        assert!(t.is_empty());
    }

    #[test]
    fn clear_all() {
        let mut t = StaleTags::default();
        t.set(1, "a".to_string());
        t.set(9, "b".to_string());
        t.clear();
        assert!(t.is_empty());
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cd /Users/koalazub/projects/helix && cargo test -p helix-view stale_tags 2>&1 | tail -20`
Expected: FAIL — `cannot find type StaleTags` / unresolved module.

- [ ] **Step 3: Write the storage struct + view-side wrapper**

Prepend to `helix-view/src/annotations/stale_tags.rs` (above the `#[cfg(test)]` block). This mirrors `math.rs` but stores one tag string per line (below-only) instead of `Vec<String>` above/below:

```rust
use std::collections::HashMap;

use helix_core::doc_formatter::FormattedGrapheme;
use helix_core::text_annotations::LineAnnotation;
use helix_core::Position;

use crate::Document;

#[derive(Debug, Default, Clone)]
pub struct StaleTags {
    tags: HashMap<usize, String>,
}

impl StaleTags {
    pub fn is_empty(&self) -> bool {
        self.tags.is_empty()
    }

    pub fn tag(&self, line_idx: usize) -> Option<&str> {
        self.tags.get(&line_idx).map(String::as_str)
    }

    pub fn set(&mut self, line_idx: usize, text: String) {
        if text.is_empty() {
            self.tags.remove(&line_idx);
        } else {
            self.tags.insert(line_idx, text);
        }
    }

    pub fn clear_at(&mut self, line_idx: usize) {
        self.tags.remove(&line_idx);
    }

    pub fn clear(&mut self) {
        self.tags.clear();
    }

    pub fn rows_to_reserve_after(&self, doc_line: usize) -> usize {
        usize::from(self.tags.contains_key(&doc_line))
    }
}

pub struct StaleTagAnnotations<'a> {
    tags: &'a StaleTags,
}

impl<'a> StaleTagAnnotations<'a> {
    pub fn new(doc: &'a Document) -> Box<dyn LineAnnotation + 'a> {
        Box::new(Self {
            tags: doc.stale_tags(),
        })
    }
}

impl LineAnnotation for StaleTagAnnotations<'_> {
    fn insert_virtual_lines(
        &mut self,
        _line_end_char_idx: usize,
        _line_end_visual_pos: Position,
        doc_line: usize,
    ) -> Position {
        Position::new(self.tags.rows_to_reserve_after(doc_line), 0)
    }
}

fn _unused(_: FormattedGrapheme) {}
```

Verify the `LineAnnotation` trait method signature against `helix-view/src/annotations/math.rs`'s view-side wrapper before finalizing — copy its exact `insert_virtual_lines` signature and drop the `_unused`/`FormattedGrapheme` import if that crate's wrapper does not use it.

Then add to `helix-view/src/annotations.rs` next to `pub mod math;`:

```rust
pub mod stale_tags;
```

Note: `doc.stale_tags()` does not exist yet (Task 2). For this task's tests to compile, the view-side wrapper can be added in Task 2 instead; if `cargo test -p helix-view stale_tags` fails only on `stale_tags()`, move the `StaleTagAnnotations` block to the end of Task 2 and keep only the `StaleTags` struct + tests here.

- [ ] **Step 4: Run tests to verify they pass**

Run: `cd /Users/koalazub/projects/helix && cargo test -p helix-view stale_tags 2>&1 | tail -20`
Expected: PASS — 4 tests.

- [ ] **Step 5: Commit**

```bash
cd /Users/koalazub/projects/helix
git add helix-view/src/annotations/stale_tags.rs helix-view/src/annotations.rs
git commit -m "feat(annotations): StaleTags storage layer

Independent from math-lines so stale-cell tags never share storage or
clearing with math virtual rows.

Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>"
```

### Task 2: `Document` field + accessor/mutator methods

**Files:**
- Modify: `helix-view/src/document.rs:166` (field), `:798` (init), `:2647-2673` (methods, add after the math-lines block)

**Interfaces:**
- Produces: `Document::set_stale_tag(&mut self, line_idx: usize, text: String)`, `Document::clear_stale_tag(&mut self, line_idx: usize)`, `Document::clear_all_stale_tags(&mut self)`, `Document::stale_tags(&self) -> &crate::annotations::stale_tags::StaleTags`.

- [ ] **Step 1: Add the field**

At `helix-view/src/document.rs:166`, after the `math_lines:` field:

```rust
    stale_tags: crate::annotations::stale_tags::StaleTags,
```

- [ ] **Step 2: Initialize it**

At `helix-view/src/document.rs:798`, after `math_lines: ...::default(),`:

```rust
            stale_tags: crate::annotations::stale_tags::StaleTags::default(),
```

- [ ] **Step 3: Add the methods**

After the `clear_all_math_lines` / `math_lines` methods (~`document.rs:2673`):

```rust
    pub fn set_stale_tag(&mut self, line_idx: usize, text: String) {
        self.stale_tags.set(line_idx, text);
    }

    pub fn clear_stale_tag(&mut self, line_idx: usize) {
        self.stale_tags.clear_at(line_idx);
    }

    pub fn clear_all_stale_tags(&mut self) {
        self.stale_tags.clear();
    }

    pub fn stale_tags(&self) -> &crate::annotations::stale_tags::StaleTags {
        &self.stale_tags
    }
```

- [ ] **Step 4: Verify it compiles**

Run: `cd /Users/koalazub/projects/helix && cargo check -p helix-view 2>&1 | tail -20`
Expected: no errors (warnings about unused methods are acceptable until Task 5 wires them).

- [ ] **Step 5: Commit**

```bash
cd /Users/koalazub/projects/helix
git add helix-view/src/document.rs
git commit -m "feat(document): stale-tag accessors mirroring math-lines

Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>"
```

### Task 3: Term-side decoration renderer

**Files:**
- Create: `helix-term/src/ui/text_decorations/stale_tag_annotations.rs`
- Modify: `helix-term/src/ui/text_decorations.rs:10,13` (mod + re-export)

**Interfaces:**
- Consumes: `Document::stale_tags()` (Task 2).
- Produces: `StaleTagAnnotations<'a>` implementing `Decoration`, constructed by `StaleTagAnnotations::new(doc: &'a Document, theme: &Theme) -> Self`.

- [ ] **Step 1: Write the renderer**

Create `helix-term/src/ui/text_decorations/stale_tag_annotations.rs`, mirroring `math_annotations.rs` but painting a single below-row per tagged line. Copy the exact trait method signatures (`Decoration::render_virt_lines`, `LineAnnotation::insert_virtual_lines`) and imports from `math_annotations.rs`:

```rust
// Stale-tag virtual rows. Helix only reserves rows AFTER a source line, so a
// tag for line N renders on the row immediately below line N. This layer is
// independent of math-lines and paints after it (see editor.rs decoration
// order), so it stacks below any math rows rather than over them.
use helix_core::doc_formatter::FormattedGrapheme;
use helix_core::text_annotations::LineAnnotation;
use helix_core::Position;
use helix_view::annotations::stale_tags::StaleTags;
use helix_view::graphics::Style;
use helix_view::theme::Theme;
use helix_view::Document;

use crate::ui::document::{LinePos, TextRenderer};
use crate::ui::text_decorations::Decoration;

pub struct StaleTagAnnotations<'a> {
    tags: &'a StaleTags,
    style: Style,
}

impl<'a> StaleTagAnnotations<'a> {
    pub fn new(doc: &'a Document, theme: &Theme) -> Self {
        let mut style = theme.get("ui.virtual.stale");
        if style == Style::default() {
            style = theme.get("ui.virtual.conceal");
        }
        Self {
            tags: doc.stale_tags(),
            style,
        }
    }
}

impl LineAnnotation for StaleTagAnnotations<'_> {
    fn insert_virtual_lines(
        &mut self,
        _line_end_char_idx: usize,
        _line_end_visual_pos: Position,
        doc_line: usize,
    ) -> Position {
        Position::new(self.tags.rows_to_reserve_after(doc_line), 0)
    }
}

impl Decoration for StaleTagAnnotations<'_> {
    fn render_virt_lines(
        &mut self,
        renderer: &mut TextRenderer,
        pos: LinePos,
        virt_off: Position,
    ) -> Position {
        let Some(tag) = self.tags.tag(pos.doc_line) else {
            return Position::new(0, 0);
        };
        let base_row = pos.visual_line + virt_off.row as u16;
        if base_row >= renderer.viewport.height {
            return Position::new(0, 0);
        }
        renderer.set_string(0, base_row, tag, self.style);
        Position::new(1, 0)
    }
}

fn _unused(_: FormattedGrapheme) {}
```

Match the exact `LinePos`/`TextRenderer`/`Decoration` import paths and the `render_virt_lines`/`insert_virtual_lines` signatures to `math_annotations.rs`; delete the `_unused`/`FormattedGrapheme` line if `math_annotations.rs` doesn't import it. `base_row`/`pos.visual_line`/`virt_off.row` types follow `math_annotations.rs` exactly.

- [ ] **Step 2: Add mod + re-export**

In `helix-term/src/ui/text_decorations.rs`, beside `mod math_annotations;` (line 13) and `pub use math_annotations::MathAnnotations;` (line 10):

```rust
mod stale_tag_annotations;
```
```rust
pub use stale_tag_annotations::StaleTagAnnotations;
```

- [ ] **Step 3: Verify it compiles**

Run: `cd /Users/koalazub/projects/helix && cargo check -p helix-term 2>&1 | tail -25`
Expected: no errors (an unused-`StaleTagAnnotations` warning is fine until Task 4).

- [ ] **Step 4: Commit**

```bash
cd /Users/koalazub/projects/helix
git add helix-term/src/ui/text_decorations/stale_tag_annotations.rs helix-term/src/ui/text_decorations.rs
git commit -m "feat(decorations): stale-tag virtual-row renderer

Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>"
```

### Task 4: Wire reservation + decoration into rendering

**Files:**
- Modify: `helix-view/src/view.rs:4` (import), `:546-548` (reservation)
- Modify: `helix-term/src/ui/editor.rs:225-227` (decoration, added AFTER math)

**Interfaces:**
- Consumes: `StaleTagAnnotations` (view-side, Task 1/2) + (term-side, Task 3); `Document::stale_tags()`.

- [ ] **Step 1: Reserve rows (view side)**

In `helix-view/src/view.rs`, import the view-side wrapper next to the math import (line 4):

```rust
use crate::annotations::stale_tags::StaleTagAnnotations;
```

After the math reservation block (`view.rs:546-548`):

```rust
        if !doc.stale_tags().is_empty() {
            text_annotations.add_line_annotation(StaleTagAnnotations::new(doc));
        }
```

- [ ] **Step 2: Paint (term side), AFTER math**

In `helix-term/src/ui/editor.rs`, immediately after the math decoration block (`editor.rs:225-227`) — order matters: stale must be added after math so it paints on the row below math's rows:

```rust
        if !doc.stale_tags().is_empty() {
            decorations.add_decoration(text_decorations::StaleTagAnnotations::new(doc, theme));
        }
```

- [ ] **Step 3: Verify the whole fork compiles + clippy clean**

Run: `cd /Users/koalazub/projects/helix && cargo check -p helix-term 2>&1 | tail -20 && cargo clippy -p helix-view -p helix-term 2>&1 | tail -20`
Expected: no errors, no new clippy warnings in the touched files.

- [ ] **Step 4: Commit**

```bash
cd /Users/koalazub/projects/helix
git add helix-view/src/view.rs helix-term/src/ui/editor.rs
git commit -m "feat: wire stale-tag layer into render (paints below math rows)

Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>"
```

### Task 5: Steel builtins + registration

**Files:**
- Modify: `helix-term/src/commands/engine/steel/mod.rs` (~6886-6969 impls; ~1015-1073 registration + source emit)

**Interfaces:**
- Produces Steel builtins (resolving as `helix.static.*`): `set-stale-tags-below!` (`line-idx`, `lines` list-of-strings), `clear-stale-tags!` (`line-idx`), `clear-all-stale-tags!`. List-of-strings marshalling mirrors `set_math_lines_impl` exactly (only the first string is used as the tag; extra strings ignored).

- [ ] **Step 1: Add the Rust impls**

After `clear_all_math_lines` (~`mod.rs:6969`), mirroring `set_math_lines_impl`'s focus→doc resolution and `SteelVal::ListV`/`StringV` marshalling:

```rust
pub fn set_stale_tags_below(cx: &mut Context, line_idx: usize, lines_val: steel::rvals::SteelVal) {
    let text = first_string_of(lines_val);
    if let Some(doc) = focused_doc_mut(cx) {
        doc.set_stale_tag(line_idx, text);
    }
}

pub fn clear_stale_tags(cx: &mut Context, line_idx: usize) {
    if let Some(doc) = focused_doc_mut(cx) {
        doc.clear_stale_tag(line_idx);
    }
}

pub fn clear_all_stale_tags(cx: &mut Context) {
    if let Some(doc) = focused_doc_mut(cx) {
        doc.clear_all_stale_tags();
    }
}
```

Add the two helpers next to `set_math_lines_impl`, copying its exact focus-resolution lines (`cx.editor.tree.focus`, `try_get`, `documents.get_mut`) verbatim from the math impl:

```rust
fn focused_doc_mut(cx: &mut Context) -> Option<&mut Document> {
    let view_id = cx.editor.tree.focus;
    let doc_id = cx.editor.tree.try_get(view_id)?.doc;
    cx.editor.documents.get_mut(&doc_id)
}

fn first_string_of(lines_val: steel::rvals::SteelVal) -> String {
    if let steel::rvals::SteelVal::ListV(items) = lines_val {
        for item in items.iter() {
            if let steel::rvals::SteelVal::StringV(s) = item {
                return s.to_string();
            }
        }
    }
    String::new()
}
```

If `set_math_lines_impl` already defines an equivalent focus-resolution helper, reuse it instead of adding `focused_doc_mut` (avoid the duplicate — `feedback_no_dead_code`). Confirm the `Document` import path used by the surrounding impls.

- [ ] **Step 2: Register + emit Steel wrappers**

In `load_static_commands`, after the math-lines registrations (~`mod.rs:1060`), mirror the `register_fn` + `if generate_sources` pattern exactly:

```rust
    module.register_fn("set-stale-tags-below!", set_stale_tags_below);
    module.register_fn("clear-stale-tags!", clear_stale_tags);
    module.register_fn("clear-all-stale-tags!", clear_all_stale_tags);
```

And the matching `pending_emits.push((name, source))` blocks (copy the exact macro/closure shape used for `set-math-lines-below!`):

```scheme
(provide set-stale-tags-below!)
(define (set-stale-tags-below! line-idx lines)
    (helix.static.set-stale-tags-below! *helix.cx* line-idx lines))
```
```scheme
(provide clear-stale-tags!)
(define (clear-stale-tags! line-idx)
    (helix.static.clear-stale-tags! *helix.cx* line-idx))
```
```scheme
(provide clear-all-stale-tags!)
(define (clear-all-stale-tags!)
    (helix.static.clear-all-stale-tags! *helix.cx*))
```

- [ ] **Step 3: Verify it compiles + clippy clean**

Run: `cd /Users/koalazub/projects/helix && cargo check -p helix-term 2>&1 | tail -20 && cargo clippy -p helix-term 2>&1 | tail -20`
Expected: no errors, no new warnings.

- [ ] **Step 4: Commit, then request the fork rebuild**

```bash
cd /Users/koalazub/projects/helix
git add helix-term/src/commands/engine/steel/mod.rs
git commit -m "feat(steel): set-stale-tags-below!/clear-stale-tags!/clear-all-stale-tags!

Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>"
```

**STOP — ask the user to rebuild the fork** (`darwin-rebuild` via nixoala, memory `reference_nix_helix_setup`). The agent must not run `sudo darwin-rebuild`. The plugin tasks below feature-probe these builtins and degrade gracefully, so plugin work can proceed before the rebuild; the stale tags only become visible after it.

---

# Phase B — Plugin: pure logic (TDD, headless)

All new pure functions live in `plugin/nothelix/param-tweak.scm` and are unit-tested in `plugin/tests/param-tweak-test.scm`. These functions take plain strings / accessor closures — no editor state — so they run under the headless driver.

### Task 6: `@param` line parser + test suite scaffold

**Files:**
- Create: `plugin/nothelix/param-tweak.scm`
- Create: `plugin/tests/param-tweak-test.scm`
- Modify: `plugin/tests/run-all-tests.scm` (register the suite)

**Interfaces:**
- Produces: `(parse-param-line line)` → a list `(name value-str lo hi step kind)` (`kind` is `'int` or `'float`; `lo`/`hi`/`step` are numbers) or `#false` when the line is not a valid `@param` line. `(provide parse-param-line)`.

- [ ] **Step 1: Write the test suite**

Create `plugin/tests/param-tweak-test.scm`:

```scheme
;;; param-tweak-test.scm — pure-logic tests for the @param primitive.

(require "test-framework.scm")
(require "../nothelix/param-tweak.scm")

(provide run-param-tweak-tests)

(define (run-param-tweak-tests)
  (reset-test-counters!)
  (print-test-suite-header "param-tweak")

  (assert-equal (list "freq" "440" 220 880 10 'int)
                (parse-param-line "freq = 440      # @param 220:880 step 10")
                "parse int param with step")
  (assert-equal (list "amp" "0.8" 0.0 1.0 0.05 'float)
                (parse-param-line "amp  = 0.8 # @param 0.0:1.0 step 0.05")
                "parse float param with step")
  (assert-equal "int"
                (symbol->string (list-ref (parse-param-line "n = 5 # @param 1:10") 5))
                "default int kind, no step")
  (assert-false (parse-param-line "x = 5") "no annotation -> #false")
  (assert-false (parse-param-line "# just a comment") "no assignment -> #false")
  (assert-false (parse-param-line "name = foo # @param 1:10") "non-numeric literal -> #false")

  (print-test-suite-footer "param-tweak"))
```

- [ ] **Step 2: Register the suite**

In `plugin/tests/run-all-tests.scm`, add the require after the others:

```scheme
(require "param-tweak-test.scm")
```
add to the `provide`:
```scheme
         run-param-tweak-tests
```
and add to the `suite-results` list in `run-all-nothelix-tests`:
```scheme
      (cons "param-tweak" (run-param-tweak-tests))
```

- [ ] **Step 3: Run to verify it fails**

Run: `expect /private/tmp/claude-501/-Users-koalazub-projects-nothelix/1ab569fe-a1ee-45ce-8ced-99d05ff0b95c/scratchpad/run-steel-tests.exp && cat /tmp/nothelix-steel-result.txt`
Expected: result file shows FAILs (or missing — `parse-param-line` undefined aborts load). If missing, first finish Step 4 enough to load.

- [ ] **Step 4: Implement the parser**

Create `plugin/nothelix/param-tweak.scm`:

```scheme
;;; param-tweak.scm — declare a numeric @param in a cell, nudge it, re-render.

(require "string-utils.scm")

(provide parse-param-line)

(define (split-on-first s ch)
  (let loop ([i 0])
    (cond
      [(>= i (string-length s)) #false]
      [(char=? (string-ref s i) ch)
       (cons (substring s 0 i) (substring s (+ i 1) (string-length s)))]
      [else (loop (+ i 1))])))

(define (literal-kind value-str)
  (if (string-contains? value-str ".") 'float 'int))

(define (tokens-of s)
  (filter (lambda (t) (> (string-length t) 0))
          (string-split (string-replace-all (string-trim s) "\t" " ") " ")))

(define (parse-range tok)
  (define parts (split-on-first tok #\:))
  (and parts
       (let ([lo (string->number (string-trim (car parts)))]
             [hi (string->number (string-trim (cdr parts)))])
         (and lo hi (cons lo hi)))))

(define (default-step lo hi kind)
  (if (eq? kind 'int) 1 (/ (- hi lo) 100)))

(define (parse-param-line line)
  (define halves (split-on-first line #\#))
  (and halves
       (let* ([code (car halves)]
              [comment (string-trim (cdr halves))])
         (and (string-starts-with? comment "@param")
              (let* ([spec (string-trim (substring comment 6 (string-length comment)))]
                     [code-parts (split-on-first code #\=)])
                (and code-parts
                     (let* ([name (string-trim (car code-parts))]
                            [value-str (string-trim (cdr code-parts))]
                            [toks (tokens-of spec)]
                            [rng (and (pair? toks) (parse-range (car toks)))])
                       (and rng
                            (> (string-length name) 0)
                            (string->number value-str)
                            (let* ([lo (car rng)]
                                   [hi (cdr rng)]
                                   [kind (literal-kind value-str)]
                                   [step (parse-step toks lo hi kind)])
                              (list name value-str lo hi step kind))))))))))

(define (parse-step toks lo hi kind)
  (let loop ([ts toks])
    (cond
      [(null? ts) (default-step lo hi kind)]
      [(and (equal? (car ts) "step") (pair? (cdr ts)))
       (or (string->number (cadr ts)) (default-step lo hi kind))]
      [else (loop (cdr ts))])))
```

- [ ] **Step 5: Run to verify it passes**

Run: `expect /private/tmp/claude-501/-Users-koalazub-projects-nothelix/1ab569fe-a1ee-45ce-8ced-99d05ff0b95c/scratchpad/run-steel-tests.exp && cat /tmp/nothelix-steel-result.txt`
Expected: all `param-tweak` assertions PASS.

- [ ] **Step 6: Commit**

```bash
cd /Users/koalazub/projects/nothelix
git add plugin/nothelix/param-tweak.scm plugin/tests/param-tweak-test.scm plugin/tests/run-all-tests.scm
git commit -m "feat(param): @param line parser + test suite

Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>"
```

### Task 7: Number formatting + grid-snapped nudge

**Files:**
- Modify: `plugin/nothelix/param-tweak.scm`, `plugin/tests/param-tweak-test.scm`

**Interfaces:**
- Consumes: parse output (`lo hi step kind`).
- Produces: `(decimals-of step-str-or-num)` → int decimal places; `(format-number n decimals)` → string; `(nudge-param-value current lo hi step dir)` → number snapped to the `lo + k*step` grid and clamped to `[lo, hi]` (`dir` is `1` or `-1`). `(provide format-number nudge-param-value decimals-of)`.

- [ ] **Step 1: Write the failing tests**

Add to `run-param-tweak-tests` in `param-tweak-test.scm` (before the footer):

```scheme
  (assert-equal "5" (format-number 5 0) "format int")
  (assert-equal "0.80" (format-number 0.8 2) "format float 2dp")
  (assert-equal "0.05" (format-number 0.05 2) "format small float 2dp")
  (assert-equal "1.00" (format-number 1 2) "format whole as float 2dp")
  (assert-equal 2 (decimals-of 0.05) "decimals of 0.05 is 2")
  (assert-equal 0 (decimals-of 10) "decimals of int is 0")

  (assert-equal 450 (nudge-param-value 440 220 880 10 1) "nudge int up by step")
  (assert-equal 430 (nudge-param-value 440 220 880 10 -1) "nudge int down by step")
  (assert-equal 880 (nudge-param-value 875 220 880 10 1) "nudge clamps to hi on grid")
  (assert-equal 220 (nudge-param-value 220 220 880 10 -1) "nudge clamps to lo")
  (assert-true (< (abs (- 0.85 (nudge-param-value 0.8 0.0 1.0 0.05 1))) 0.0001)
               "nudge float up by step")
```

- [ ] **Step 2: Run to verify it fails**

Run: `expect .../run-steel-tests.exp && cat /tmp/nothelix-steel-result.txt`
Expected: FAIL — `format-number`/`decimals-of`/`nudge-param-value` undefined.

- [ ] **Step 3: Implement**

Add to `param-tweak.scm` and extend the `provide`:

```scheme
(provide format-number nudge-param-value decimals-of)

(define (decimals-of step)
  (define s (if (number? step) (number->string step) step))
  (define dot (split-on-first s #\.))
  (if dot (string-length (string-trim (cdr dot))) 0))

(define (format-number n decimals)
  (if (<= decimals 0)
      (number->string (inexact->exact (round n)))
      (let* ([scale (expt 10 decimals)]
             [scaled (inexact->exact (round (* n scale)))]
             [neg (< scaled 0)]
             [mag (abs scaled)]
             [int-part (quotient mag scale)]
             [frac-part (remainder mag scale)]
             [frac-str (number->string frac-part)]
             [padded (string-append
                       (make-string (- decimals (string-length frac-str)) #\0)
                       frac-str)])
        (string-append (if neg "-" "")
                       (number->string int-part) "." padded))))

(define (nudge-param-value current lo hi step dir)
  (define steps (round (/ (- current lo) step)))
  (define next (+ steps dir))
  (define max-steps (floor (/ (- hi lo) step)))
  (define clamped (max 0 (min next max-steps)))
  (define raw (+ lo (* clamped step)))
  (if (and (integer? lo) (integer? step)) (inexact->exact (round raw)) raw))
```

- [ ] **Step 4: Run to verify it passes**

Run: `expect .../run-steel-tests.exp && cat /tmp/nothelix-steel-result.txt`
Expected: all PASS. If `format-number 1 2` is off, confirm `round` returns exact under Steel — wrap with `inexact->exact` as shown.

- [ ] **Step 5: Commit**

```bash
cd /Users/koalazub/projects/nothelix
git add plugin/nothelix/param-tweak.scm plugin/tests/param-tweak-test.scm
git commit -m "feat(param): grid-snapped nudge + fixed-decimal formatting

Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>"
```

### Task 8: Buffer-scan pure helpers — target line + stale detection

**Files:**
- Modify: `plugin/nothelix/param-tweak.scm`, `plugin/tests/param-tweak-test.scm`

**Interfaces:**
- Consumes: `parse-param-line`; `cell-marker?` (from `common.scm`).
- Produces:
  - `(find-param-target-line get-line total-lines cursor-line)` → line index of the `@param` at/above the cursor, or `#false`. `get-line` is `(lambda (idx) -> string)`.
  - `(collect-assigned-names get-line cell-start cell-end)` → list of LHS names of `@param` lines within `[cell-start, cell-end)`.
  - `(token-references? code-line name)` → bool, whole-token match (not substring).
  - `(scan-stale-lines get-line total-lines from-line names)` → list of line indices of cell-marker lines below `from-line` whose cell body references any name.
  - `(provide find-param-target-line collect-assigned-names token-references? scan-stale-lines)`.

- [ ] **Step 1: Write the failing tests**

Add to `param-tweak-test.scm`:

```scheme
  (let* ([lines (list "@cell 0 :julia"
                      "freq = 440 # @param 220:880 step 10"
                      "plot(freq)"
                      "@cell 1 :julia"
                      "y = freq * 2"
                      "@cell 2 :julia"
                      "z = frequency + 1")]
         [vec (list->vector lines)]
         [get-line (lambda (i) (if (< i (vector-length vec)) (vector-ref vec i) ""))]
         [total (vector-length vec)])
    (assert-equal 1 (find-param-target-line get-line total 2) "target = @param above cursor")
    (assert-equal 1 (find-param-target-line get-line total 1) "target = @param on cursor line")
    (assert-false (find-param-target-line get-line total 0) "no @param above cell marker -> #false")
    (assert-equal (list "freq") (collect-assigned-names get-line 0 3) "collect assigned names")
    (assert-true (token-references? "y = freq * 2" "freq") "whole-token reference")
    (assert-false (token-references? "z = frequency + 1" "freq") "substring is NOT a reference")
    (assert-equal (list 3) (scan-stale-lines get-line total 1 (list "freq"))
                  "stale cell marker line indices below"))
```

- [ ] **Step 2: Run to verify it fails**

Run: `expect .../run-steel-tests.exp && cat /tmp/nothelix-steel-result.txt`
Expected: FAIL — functions undefined.

- [ ] **Step 3: Implement**

Add to `param-tweak.scm` (it already `(require "string-utils.scm")`; add `(require "common.scm")` at top for `cell-marker?`), extend `provide`:

```scheme
(provide find-param-target-line collect-assigned-names token-references? scan-stale-lines)

(define (find-param-target-line get-line total-lines cursor-line)
  (let loop ([i (min cursor-line (- total-lines 1))])
    (cond
      [(< i 0) #false]
      [(cell-marker? (string-trim (get-line i))) #false]
      [(parse-param-line (get-line i)) i]
      [else (loop (- i 1))])))

(define (collect-assigned-names get-line cell-start cell-end)
  (let loop ([i cell-start] [acc '()])
    (if (>= i cell-end)
        (reverse acc)
        (let ([p (parse-param-line (get-line i))])
          (loop (+ i 1) (if p (cons (car p) acc) acc))))))

(define (ident-char? c)
  (or (char-alphabetic? c)
      (char-numeric? c)
      (char=? c #\_)
      (char=? c #\!)))

(define (token-references? code-line name)
  (define nlen (string-length name))
  (define llen (string-length code-line))
  (let loop ([i 0])
    (cond
      [(> (+ i nlen) llen) #false]
      [(and (equal? (substring code-line i (+ i nlen)) name)
            (or (= i 0) (not (ident-char? (string-ref code-line (- i 1)))))
            (or (= (+ i nlen) llen) (not (ident-char? (string-ref code-line (+ i nlen))))))
       #true]
      [else (loop (+ i 1))])))

(define (any-name-referenced? code-line names)
  (cond
    [(null? names) #false]
    [(token-references? code-line (car names)) #true]
    [else (any-name-referenced? code-line (cdr names))]))

(define (scan-stale-lines get-line total-lines from-line names)
  (let loop ([i (+ from-line 1)] [current-marker #false] [hit #false] [acc '()])
    (cond
      [(>= i total-lines)
       (reverse (if (and current-marker hit) (cons current-marker acc) acc))]
      [(cell-marker? (string-trim (get-line i)))
       (loop (+ i 1) i #false
             (if (and current-marker hit) (cons current-marker acc) acc))]
      [(and current-marker (any-name-referenced? (get-line i) names))
       (loop (+ i 1) current-marker #true acc)]
      [else (loop (+ i 1) current-marker hit acc)])))
```

- [ ] **Step 4: Run to verify it passes**

Run: `expect .../run-steel-tests.exp && cat /tmp/nothelix-steel-result.txt`
Expected: all PASS.

- [ ] **Step 5: Commit**

```bash
cd /Users/koalazub/projects/nothelix
git add plugin/nothelix/param-tweak.scm plugin/tests/param-tweak-test.scm
git commit -m "feat(param): target-line resolution + token-based stale scan

Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>"
```

---

# Phase C — Plugin: editor-effecting commands + wiring

These touch live editor state, so they are verified manually (headless driver loads them; behaviour confirmed in a live `hx`), not unit-tested. Each task ends with a concrete manual check.

### Task 9: Rewrite the literal in the buffer + active-param statusline readout

**Files:**
- Modify: `plugin/nothelix/param-tweak.scm`

**Interfaces:**
- Consumes: `parse-param-line`, `nudge-param-value`, `format-number`, `decimals-of`, `find-param-target-line`; `doc-get-line`, `current-line-number` (common.scm); editor/text/static builtins; `helix/components` (`span`, `status-element`, `push-status-element!`, `theme-scope-ref`).
- Produces: `(rewrite-param-literal! doc-id line-idx new-str)`, and a registered statusline element `param-readout-element`.

- [ ] **Step 1: Add editor requires + literal rewrite + readout**

Add to the top requires of `param-tweak.scm`:

```scheme
(require "helix/editor.scm")
(require "helix/misc.scm")
(require "helix/components.scm")
(require-builtin helix/core/text as text.)
(require (prefix-in helix.static. "helix/static.scm"))
```

Add (these reuse the `delete-line-range`/`insert_string` idiom from `plot-resize.scm`):

```scheme
(define (param-line-char-range rope line-idx)
  (define start (text.rope-line->char rope line-idx))
  (define end (text.rope-line->char rope (+ line-idx 1)))
  (cons start end))

(define (rewrite-param-literal! doc-id line-idx new-line-text)
  (define rope (editor->text doc-id))
  (define rng (param-line-char-range rope line-idx))
  (define r (helix.static.range (car rng) (cdr rng)))
  (define sel (helix.static.range->selection r))
  (helix.static.set-current-selection-object! sel)
  (helix.static.insert_string new-line-text)
  (helix.static.collapse_selection)
  (helix.static.commit-changes-to-history))

(define (build-param-line name new-value-str spec-suffix)
  (string-append name " = " new-value-str spec-suffix))
```

For the readout, mirror `lsp-statusline.scm` exactly:

```scheme
(define (param-readout-style) (theme-scope-ref "ui.statusline"))

(define (param-readout-element view-id focused)
  (if (not focused)
      '()
      (let* ([doc-id (editor->doc-id view-id)]
             [path (and doc-id (editor-document->path doc-id))])
        (if (not (and path (string-suffix? path ".jl")))
            '()
            (let* ([rope (editor->text doc-id)]
                   [total (text.rope-len-lines rope)]
                   [cl (current-line-number)]
                   [tgt (find-param-target-line
                          (lambda (i) (doc-get-line rope total i)) total cl)])
              (if (not tgt)
                  '()
                  (let ([p (parse-param-line (doc-get-line rope total tgt))])
                    (if (not p)
                        '()
                        (list (span (string-append
                                      " " (car p) "=" (cadr p)
                                      " [" (number->string (list-ref p 2))
                                      ":" (number->string (list-ref p 3)) "] ")
                                    (param-readout-style)))))))))))

(push-status-element! 'right (status-element param-readout-element))
```

- [ ] **Step 2: Manual verify (after Task 12 wires the module load)**

This step has no standalone check; it is exercised by the Task 12 manual test. Confirm only that the module loads (the headless driver's result file is produced, proving no syntax/require error).

Run: `expect .../run-steel-tests.exp && cat /tmp/nothelix-steel-result.txt`
Expected: result file present, param-tweak suite still PASS (no regression).

- [ ] **Step 3: Commit**

```bash
cd /Users/koalazub/projects/nothelix
git add plugin/nothelix/param-tweak.scm
git commit -m "feat(param): in-place literal rewrite + active-param statusline readout

Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>"
```

### Task 10: Debounced single re-run scheduler

**Files:**
- Modify: `plugin/nothelix/param-tweak.scm`

**Interfaces:**
- Consumes: `execute-cell` (execution.scm), `enqueue-thread-local-callback-with-delay` (helix/misc).
- Produces: `(schedule-param-rerun!)` — coalesces rapid calls via a generation counter, fires `execute-cell` once after 150 ms of quiet.

- [ ] **Step 1: Implement the scheduler**

Add `(require "execution.scm")` to `param-tweak.scm` requires, and:

```scheme
(define *param-rerun-generation* (box 0))

(define (schedule-param-rerun!)
  (define gen (+ 1 (unbox *param-rerun-generation*)))
  (set-box! *param-rerun-generation* gen)
  (enqueue-thread-local-callback-with-delay 150
    (lambda ()
      (when (= gen (unbox *param-rerun-generation*))
        (execute-cell)))))
```

- [ ] **Step 2: Manual verify**

Deferred to Task 12 (needs the commands + a live kernel). Confirm no load regression:
Run: `expect .../run-steel-tests.exp && cat /tmp/nothelix-steel-result.txt`
Expected: result file present, suites PASS.

- [ ] **Step 3: Commit**

```bash
cd /Users/koalazub/projects/nothelix
git add plugin/nothelix/param-tweak.scm
git commit -m "feat(param): debounced coalesced re-run scheduler

Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>"
```

### Task 11: Stale-tag staging (feature-probed) + clear-on-execute

**Files:**
- Modify: `plugin/nothelix/param-tweak.scm`, `plugin/nothelix/execution.scm`

**Interfaces:**
- Consumes: `scan-stale-lines`, `collect-assigned-names`; the fork builtins `set-stale-tags-below!`/`clear-stale-tags!`/`clear-all-stale-tags!` (Phase A, feature-probed).
- Produces: `(stage-stale-tags! doc-id param-line names)`; `(clear-stale-tag-for-line! line-idx)`; `(provide clear-stale-tag-for-line!)` so execution.scm can call it.

- [ ] **Step 1: Add feature-probe + staging (mirror `math-render`'s `try-*`/`*-available?`)**

In `param-tweak.scm`:

```scheme
(provide clear-stale-tag-for-line!)

(define (stale-tags-ffi-available?)
  (with-handler (lambda (_) #false)
    (eval '(helix.static.clear-all-stale-tags!)) #true))

(define (try-set-stale-tag! line-idx text)
  (with-handler (lambda (_) #false)
    (eval `(helix.static.set-stale-tags-below! ,line-idx ',(list text)))))

(define (clear-stale-tag-for-line! line-idx)
  (with-handler (lambda (_) #false)
    (eval `(helix.static.clear-stale-tags! ,line-idx))))

(define (stage-stale-tags! doc-id param-line names)
  (when (and (stale-tags-ffi-available?) (pair? names))
    (define rope (editor->text doc-id))
    (define total (text.rope-len-lines rope))
    (define get-line (lambda (i) (doc-get-line rope total i)))
    (define stale-lines (scan-stale-lines get-line total param-line names))
    (define label (string-append "  ○ stale · " (string-join names ", ") " changed"))
    (for-each (lambda (ln) (try-set-stale-tag! ln label)) stale-lines)))
```

- [ ] **Step 2: Clear a cell's tag when it executes**

In `plugin/nothelix/execution.scm`, add `(require "param-tweak.scm")` to its requires. In `execute-cell` (after `cell-start` is known, ~line 78) and in `execute-single-cell-async` (after `cell-marker-line` is known, ~line 236), call:

```scheme
(clear-stale-tag-for-line! cell-start)
```
```scheme
(clear-stale-tag-for-line! cell-marker-line)
```

Guard against a require cycle: `param-tweak.scm` requires `execution.scm` (for `execute-cell`) and `execution.scm` would now require `param-tweak.scm`. Break it by moving `clear-stale-tag-for-line!` into a tiny leaf module `plugin/nothelix/stale-tags.scm` (no `execution.scm` require) that both `param-tweak.scm` and `execution.scm` require. Put `stale-tags-ffi-available?`, `try-set-stale-tag!`, `clear-stale-tag-for-line!`, and `stage-stale-tags!`'s tag-setting there; `param-tweak.scm` keeps the scan/label logic and calls into it. Verify there is no cycle: `stale-tags.scm` requires only `string-utils.scm` + editor builtins.

- [ ] **Step 3: Manual verify**

Deferred to Task 12. Confirm no load regression:
Run: `expect .../run-steel-tests.exp && cat /tmp/nothelix-steel-result.txt`
Expected: result file present, suites PASS.

- [ ] **Step 4: Commit**

```bash
cd /Users/koalazub/projects/nothelix
git add plugin/nothelix/param-tweak.scm plugin/nothelix/stale-tags.scm plugin/nothelix/execution.scm
git commit -m "feat(param): stage stale tags on nudge, clear on cell execute

Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>"
```

### Task 12: `:param-up` / `:param-down` commands + full wiring

**Files:**
- Modify: `plugin/nothelix/param-tweak.scm` (commands), `plugin/nothelix.scm` (require, provide, keymap, docs)

**Interfaces:**
- Consumes: everything above.
- Produces: `(param-up)`, `(param-down)`; `(provide param-up param-down)`.

- [ ] **Step 1: Implement the commands**

In `param-tweak.scm`:

```scheme
(provide param-up param-down)

(define (nudge-param! dir)
  (define focus (editor-focus))
  (define doc-id (editor->doc-id focus))
  (define path (editor-document->path doc-id))
  (cond
    [(not (and path (string-suffix? path ".jl")))
     (set-status! "param: only runs on .jl notebook files")]
    [else
     (define rope (editor->text doc-id))
     (define total (text.rope-len-lines rope))
     (define get-line (lambda (i) (doc-get-line rope total i)))
     (define cl (current-line-number))
     (define tgt (find-param-target-line get-line total cl))
     (cond
       [(not tgt) (set-status! "param: no @param at or above the cursor")]
       [else
        (define line (get-line tgt))
        (define p (parse-param-line line))
        (define name (car p))
        (define cur (string->number (cadr p)))
        (define lo (list-ref p 2))
        (define hi (list-ref p 3))
        (define step (list-ref p 4))
        (define kind (list-ref p 5))
        (define next (nudge-param-value cur lo hi step dir))
        (define dec (if (eq? kind 'int) 0 (decimals-of step)))
        (define new-str (format-number next dec))
        (define comment-half (split-on-first line #\#))
        (define spec-suffix (if comment-half (string-append "  #" (cdr comment-half)) ""))
        (define newline-suffix (if (string-suffix? line "\n") "\n" ""))
        (rewrite-param-literal! doc-id tgt
          (string-append (build-param-line name new-str (string-trim-right spec-suffix)) newline-suffix))
        (define cell-start (find-cell-start-line get-line tgt))
        (define cell-end (find-cell-code-end get-line total (+ cell-start 1)))
        (define names (collect-assigned-names get-line cell-start cell-end))
        (stage-stale-tags! doc-id cell-start names)
        (schedule-param-rerun!)
        (set-status! (string-append name " = " new-str))])]))

(define (param-up) (nudge-param! 1))
(define (param-down) (nudge-param! -1))
```

Add `(require "cell-boundaries.scm")` to `param-tweak.scm` for `find-cell-start-line`/`find-cell-code-end`. If `string-trim-right` is not available, trim trailing whitespace inline with a small helper (Steel's `trim-end` is global; wrap it like `string-trim` does `trim`).

- [ ] **Step 2: Wire into the plugin**

In `plugin/nothelix.scm`:
- Add the require after `plot-resize` (line 29): `(require "nothelix/param-tweak.scm")`
- Add to `provide` (near `plot-grow plot-shrink`, line 57): `param-up param-down`
- In `notebook-bindings`, add under the `]` and `[` prefix maps (lines 208-209):
```scheme
      ("]" ("l" ":next-cell") ("p" ":param-up"))
      ("[" ("l" ":previous-cell") ("p" ":param-down"))
```
- In `nothelix-command-docs`, next to the plot-resize docs (line 255):
```scheme
    "param-up"   "Increase the @param at/above the cursor by one step and re-render."
    "param-down" "Decrease the @param at/above the cursor by one step and re-render."
```

- [ ] **Step 3: Verify plugin loads + `just check` gate**

Run: `expect .../run-steel-tests.exp && cat /tmp/nothelix-steel-result.txt`
Expected: result file present (plugin loaded with the new commands), all suites PASS.

Then the project gate (memory `reference_just_check_gate`):
Run: `cd /Users/koalazub/projects/nothelix && just check 2>&1 | tail -25`
Expected: clippy + nextest + headless plugin-load sentinel all pass. (No `libnothelix` change, so nextest should be unaffected.)

- [ ] **Step 4: Manual end-to-end (live `hx`, after the user has rebuilt the fork)**

Ask the user to run `just install` is NOT needed (no dylib change), but the fork rebuild from Phase A must be live. Then, in a live `hx`, open a `.jl` notebook with:
```julia
@cell 0 :julia
freq = 5 # @param 1:20 step 1
@show freq
@cell 1 :julia
freq * 10
```
- Put the cursor on the `freq` line, press `]p` a few times: the literal increments instantly, the statusline shows `freq=N [1:20]`, and after ~150 ms the cell re-runs once.
- Confirm cell 1 shows the `○ stale · freq changed` tag on the row below its `@cell 1` header.
- Execute cell 1 (`space n r`): the stale tag clears.

Report results to the user; do not self-certify the visual behaviour.

- [ ] **Step 5: Commit**

```bash
cd /Users/koalazub/projects/nothelix
git add plugin/nothelix/param-tweak.scm plugin/nothelix.scm
git commit -m "feat(param): :param-up/:param-down commands + keymap + docs

Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>"
```

---

# Phase D — Documentation

### Task 13: Update spec non-goal + user docs + README

**Files:**
- Modify: `docs/superpowers/specs/2026-06-30-tweakable-parameters-design.md` (revise the "No FFI changes" non-goal)
- Modify: `docs/commands.md`, `docs/rendering.md`, `README.md`

- [ ] **Step 1: Revise the spec non-goal**

In the design spec's Non-goals, change the "No FFI changes" line to reflect the approved decision:

```markdown
- No `libnothelix` FFI change. The stale-tag display uses a dedicated annotation
  layer added to the Helix fork (feature-probed by the plugin), parallel to
  math-lines so the two never collide.
```

- [ ] **Step 2: Document the commands**

In `docs/commands.md`, in the plot/render command section, add `:param-up` / `:param-down` with the `]p` / `[p` keys and a one-paragraph explanation of the `# @param` annotation grammar (`<lhs> = <literal>  # @param <lo>:<hi> [step <s>]`), the instant-literal + debounced-rerun behaviour, the same-cell re-run scope, and the stale-tag heuristic with its honest limitation (name-based, no dataflow; reassignment/shadowing can mis-flag).

- [ ] **Step 3: Mention it in the rendering page + README**

In `docs/rendering.md`, add a short paragraph after the plot-resize mention: a `# @param` annotation turns a numeric literal into a live knob — `]p`/`[p` nudge it and re-render the figure in place.

In `README.md`, extend the rendering synopsis sentence that already mentions `:plot-grow`/`:plot-shrink` with: "and a `# @param` annotation makes a numeric literal a live knob — nudge it with `]p`/`[p` to re-compute the figure in place."

- [ ] **Step 4: Commit**

```bash
cd /Users/koalazub/projects/nothelix
git add docs/superpowers/specs/2026-06-30-tweakable-parameters-design.md docs/commands.md docs/rendering.md README.md
git commit -m "docs: document tweakable @param primitive

Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>"
```

---

## Self-Review

**Spec coverage:** §1 syntax → Task 6. §1 float precision/step defaults → Tasks 6,7. §2 target selection → Task 8. §2 keys/commands → Task 12. §2 statusline readout → Task 9. §3 instant literal → Task 9. §3 debounced coalesced re-run → Task 10. §4 same-cell re-run → Task 12. §4 stale detection (assigned names + token scan) → Task 8. §4 non-destructive overlay tag → Phase A (revised to dedicated fork layer per approved decision) + Task 11. §4 clear-on-execute → Task 11. §4 state keyed per cell → fork `HashMap<usize,String>` (Task 1) + plugin staging (Task 11). §5 reuse execute-cell/sync/cell-boundaries/debounce → Tasks 10,11,12. §6 edge cases (malformed/clamp/hand-edit/convert-survival/undo chatter/empty) → parser returns #false (Task 6), clamp (Task 7), buffer-is-truth re-read (Task 12). §7 testing → Tasks 6,7,8 (pure TDD) + Tasks 9-12 (manual). §8 future hooks → untouched.

**Placeholder scan:** No TBD/TODO. Every code step shows real code. Two steps say "verify against math_annotations.rs and copy exact signatures" — that is a deliberate guard because the exact `LineAnnotation`/`Decoration` trait signatures must match the fork's current trait definitions, which the implementer reads at that moment; the surrounding code is complete.

**Type consistency:** `parse-param-line` returns `(name value-str lo hi step kind)` consistently consumed in Tasks 7-12 by position. `find-param-target-line` arg order `(get-line total-lines cursor-line)` matches all call sites. Fork: `StaleTags`/`stale_tags()`/`set_stale_tag`/`clear_stale_tag`/`clear_all_stale_tags` names consistent across Tasks 1,2,5. Steel builtin names `set-stale-tags-below!`/`clear-stale-tags!`/`clear-all-stale-tags!` consistent across Task 5 and Task 11's probes. Require-cycle risk between `param-tweak.scm` and `execution.scm` is called out and resolved via the `stale-tags.scm` leaf module in Task 11 Step 2.
