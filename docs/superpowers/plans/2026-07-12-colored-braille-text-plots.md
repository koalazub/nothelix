# Colored Braille Text-Plots — Implementation Plan (spec 3 of the output-architecture epic)

> **For agentic workers:** REQUIRED SUB-SKILL: superpowers:subagent-driven-development or superpowers:executing-plans. Steps use checkbox (`- [ ]`) syntax.

**Goal:** Render UnicodePlots (braille/Unicode) plots inline as COLORED text — terminal-native, selectable, no raster — using the store + virtual-row substrate from Plan 1.

**Architecture:** Kernel detects a UnicodePlots plot, captures its `show` output, and parses the ANSI SGR codes into braille TEXT + a color-span map (`row, start, end, ansi-color`). The fork's `output_lines` annotation is extended to carry per-span style; the plugin maps ANSI colors → theme scopes and renders via the styled annotation. A text-plot is another store output kind that stacks alongside raster images and text.

**Tech Stack:** Julia (kernel `output_capture.jl`), Rust (Helix fork `~/projects/helix`), Steel plugin.

## Global Constraints

- No code comments except `//!`/`///`/`;;@doc` (and Julia's `#` where the file already uses them — match the file's convention).
- Fork changes land in `/Users/koalazub/projects/helix` (git); need a fork-input bump + hx rebuild (user). Plugin uses deferred-eval guards.
- Backward compat: the styled `set-output-lines-below!` MUST still accept plain-string rows (Plan 1's monochrome text output) — a plain string = one span, no scope. Non-braille output is unaffected.
- Activation implicit (a UnicodePlots plot → braille), with a `.nothelix.conf` `plot-mode = auto|raster|braille` override (default `auto`).
- The kernel env needs `UnicodePlots`; `runner.jl` self-heals via `Pkg.add` (as it does for JSON3 — see kernel-env-rot reference).

---

### Task 1: Fork — per-span styled `output_lines`

**Files (`/Users/koalazub/projects/helix`):**
- Modify `helix-view/src/annotations/output.rs` (`OutputLines` row type).
- Modify `helix-view/src/document.rs` (`set_output_lines_below` signature).
- Modify `helix-term/src/ui/text_decorations/output_annotations.rs` (paint per-span style).
- Modify `helix-term/src/commands/engine/steel/mod.rs` (the `set-output-lines-below!` glue: accept rows-of-spans).

**Interfaces:** `set-output-lines-below!(line, rows)` where each row is either a plain string OR a list of `(text scope-or-false)` spans. `scope` is a theme scope name (e.g. `"ui.virtual.output.series0"`); `#false` = default.

- [ ] **Step 1: Data model + failing test** — In `output.rs`, change the stored row from `String` to `Vec<StyledSpan>` where `StyledSpan { text: String, scope: Option<String> }`. Add a `From<String>` (whole string, no scope) so existing callers/tests still compile. Add a unit test in `output.rs` asserting a mixed styled row round-trips (text concatenation == expected, scopes preserved) and a plain-string row becomes one no-scope span. Run `cargo test -p helix-view output` → FAIL (type not present), then implement, then PASS.

- [ ] **Step 2: Decoration paints per-span** — In `output_annotations.rs`, when rendering a row, iterate spans: for each span resolve `scope` against the theme (mirror how `plugin_style_highlights`/`doc_plugin_style_highlights_into` resolves a scope to a `Style` in `ui/editor.rs`), and paint that span's text with that style; `None` scope → the default output style. Read the current single-style paint and generalize it. `cargo build` clean.

- [ ] **Step 3: Steel binding accepts spans** — In `steel/mod.rs`, the `set-output-lines-below!` glue currently takes a list of strings. Extend it: each row element may be a string (→ one no-scope span) or a list of `(text scope)` pairs (→ styled spans). Read the current extraction and add the list-of-spans branch. Keep the string branch identical (Plan 1 backward compat). `cargo build` clean.

- [ ] **Step 4: Commit (git)** — `git add -A && git commit -m "feat(annotations): per-span styled output_lines for colored text output"`. Report SHA.

---

### Task 2: Kernel — UnicodePlots detection + ANSI→spans

**Files:** `kernel/output_capture.jl` (detection + capture + ANSI parse); `kernel/runner.jl` (ensure `UnicodePlots` available, mirroring the JSON3 self-heal).

**Interfaces:** Produces a `text_plot` output object in the result JSON: `{"text_plot": {"rows": ["…braille…", …], "spans": [[row, start, end, color], …]}}` where `color` is an ANSI palette index (0–7, 8–15 bright); `start`/`end` are 0-based char offsets within `rows[row]` (post-ANSI-strip).

- [ ] **Step 1: Detect a UnicodePlots plot** — In `capture_toplevel`'s per-expression loop (near `is_displayable_plot`), add `is_unicode_plot(x)` = the value's type is from the `UnicodePlots` module (check `parentmodule(typeof(x)) === UnicodePlots` via `Base.invokelatest`/`nameof`, guarded so it's false when UnicodePlots isn't loaded). Read the existing `is_displayable_plot` for the idiom.

- [ ] **Step 2: Capture + parse ANSI** — When detected (or `plot-mode=braille`): `sprint(show, MIME"text/plain"(), plot)` (or `show(io, plot)`) to get the ANSI string. Write a Julia ANSI-SGR tokenizer (NOT regex — a scanner over the escape sequences) that walks the string, tracking the current foreground color from `\e[...m` codes (30–37, 90–97, and reset 0/39), stripping the escapes, and recording, per output row (split on `\n`), the `(start, end, color)` runs of non-default color. Emit `rows` (stripped braille lines) + `spans`. Handle the common UnicodePlots SGR subset; unknown codes → ignore (default color).

- [ ] **Step 3: Emit into the result JSON** — Add the `text_plot` object to the ordered outputs the runner returns (alongside `images`), so multiple text-plots (and mixed raster+braille) stack. Ensure the existing single-return + images paths are unaffected.

- [ ] **Step 4: Env self-heal** — In `runner.jl`, ensure `UnicodePlots` is available the same way JSON3 is ensured (Pkg.add on missing). Only when a braille render is actually requested (don't force-install for every kernel).

- [ ] **Step 5: Verify + commit** — If a Julia runtime is available, exercise the ANSI tokenizer on a captured `lineplot` sample and assert rows/spans; else document the manual check. Commit: `jj describe @ -m "feat(kernel): capture UnicodePlots as braille text + ANSI color spans"`.

---

### Task 3: Plugin — render colored braille from the store

**Files:** `plugin/nothelix/output-store.scm` (store the text_plot rows+spans); `plugin/nothelix/output-insert.scm` (render); `plugin/nothelix/output-render.scm` (color-scope mapping); a theme-scope set.

**Interfaces:** Consumes the `text_plot` JSON (Task 2) + the styled `set-output-lines-below!` (Task 1). Produces styled rows: ANSI color index → `ui.virtual.output.seriesN` scope.

- [ ] **Step 1: ANSI-color → theme-scope map** — A pure helper `(ansi-color->scope idx)` mapping 0–15 → a small fixed set of scope names (e.g. `"ui.virtual.output.series0".."series7"`, bright reusing the same 8). Document the scopes so a theme can style them; fall back to a sensible default.

- [ ] **Step 2: Build styled rows** — From the `text_plot` `{rows, spans}`, build the per-row list of `(text scope)` spans: split each row at span boundaries, tagging colored runs with their scope and gaps with `#false`. A pure function `(text-plot->styled-rows rows spans)` — unit-test it in `plugin/tests/` (mirror the multi-graph/image-cache test pattern; assert span segmentation for a 2-color row).

- [ ] **Step 3: Render + store** — In `update-cell-output` / the store, when the result carries a `text_plot`, `store-put!` it (extend the encoded blob) and render via `try-set-output-lines-below!` with the styled rows. Text-plots stack with raster images and plain text (they're virtual rows, zero undo). Reopen restores them from the store like plain text rows.

- [ ] **Step 4: `plot-mode` config** — Read `plot-mode` (`auto|raster|braille`) in `project-config.scm` (mirror an existing enum-ish/string key); pass it to the kernel request so `braille` forces braille and `raster` forces PNG. Default `auto`.

- [ ] **Step 5: Verify + commit** — `just check` (after fork rebuild + `just install`); the pure `text-plot->styled-rows` test runs headless. Manual (live): `using UnicodePlots; lineplot(1:10)` renders colored braille inline; colors track series; reopen restores; stacks with a raster plot. Commit: `jj describe @ -m "feat(output): render colored braille text-plots from the store"`.

---

## Self-Review

- Spec: per-span styled annotation (T1), kernel braille+spans (T2), plugin render+config (T3). ✓
- Placeholders: T1 gives the data-model change + tests; T2 specifies the ANSI scanner (no regex, per repo rule) + JSON shape; T3 names the pure helpers + tests. The fork decoration/steel-glue and kernel steps describe transformations of existing code with cited sites (adapt-to-real-code — the surrounding code must be read).
- Type consistency: `StyledSpan{text,scope}` + `set-output-lines-below!(rows-of-spans)` (T1) ← `text_plot{rows,spans}` (T2) ← `ansi-color->scope`/`text-plot->styled-rows` (T3).
- Backward compat: plain-string rows still work (Plan 1 monochrome) — T1 Step 1/3 keep the string branch. Non-braille output unaffected.
- Deferred/dependent: needs Plan 1 (store + output_lines) — landed; composes with multi-graph (ordered outputs) + Plan 2 (undo — braille is virtual, no dependency). Live verification required (fork rebuild + Julia UnicodePlots).
