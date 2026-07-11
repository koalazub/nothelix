# Tweakable parameters (`@param`) — design

**Date:** 2026-06-30
**Status:** approved, ready for implementation plan
**Scope:** the tweakable-parameter primitive only. Audio playback and Strudel-style
live coding are explicitly **out of scope** here — they are separate, later specs
that build on this primitive.

## Motivation

nothelix already has the seed of inline interactivity: `:plot-grow` / `:plot-shrink`
mutate an `@image` plot's canvas in the buffer and repaint it without leaving Helix.
What it cannot do is change the *value* a plot is computed from. This design adds a
"knob" — a way to declare a numeric parameter in a cell, nudge it with a keypress,
and have the figure re-render — expressed entirely through the fork's existing
"dumb core, smart plugin, buffer-is-truth" model. It is the unlock for the broader
interactivity direction (live graphs now; audio later).

## Non-goals

- No reactive dataflow engine. Cells stay linear text; there is no dependency graph.
- No real-time (60 fps) dragging. A nudge triggers a kernel re-run, which is a *beat*,
  not a frame.
- No new parameter types beyond numeric scalars (no enums, lists, colours, strings).
- No `libnothelix` FFI change. The stale-tag display uses a dedicated annotation
  layer added to the Helix fork (feature-probed by the plugin), parallel to
  math-lines so the two never collide.

## 1. Syntax & semantics

A parameter is a **real Julia assignment** carrying a trailing annotation comment:

```julia
freq = 440      # @param 220:880 step 10
amp  = 0.8      # @param 0.0:1.0 step 0.05

plot(sin.(2pi*freq .* t) .* amp)
# @image plots/wave.png
████████████
████████████
```

- The numeric literal on the left-hand side (`440`) is the **single source of truth**.
  Stripped of the plugin, the line is ordinary Julia and runs identically; the
  `# @param` comment is inert.
- Grammar (parsed by tokenization, not regex — per project rule):
  - `<lo>:<hi>` — required. Defines the clamp range.
  - `step <s>` — optional. Default `1` for integer literals; `(hi − lo) / 100` for
    float literals.
- The literal's numeric kind is inferred from its text: an integer literal stays an
  integer; a float literal is formatted to the step's decimal precision (e.g.
  `step 0.05` → two decimal places).
- Only numeric scalar literals are supported. A malformed annotation or a
  non-numeric right-hand side is ignored (the line is treated as ordinary code), with
  a one-line status warning when the user tries to nudge it.

## 2. Interaction

- **Target selection:** a nudge acts on the `@param` on the cursor's current line; if
  the cursor is not on a `@param` line, it acts on the nearest `@param` line *above*
  the cursor — the same upward walk `plot-resize.scm`'s `find-image-marker-above`
  uses.
- **Nudge keys:** `]p` (increase) and `[p` (decrease), siblings of the existing
  `]l` / `[l` cell-navigation bindings. Bound in `notebook-bindings` in
  `plugin/nothelix.scm`. Tunable; these are the defaults.
- **Commands:** `:param-up` and `:param-down`, registered and documented in
  `nothelix-command-docs` alongside `plot-grow` / `plot-shrink`.
- **Active-parameter readout:** a calm, always-there statusline element (built on the
  `lsp-statusline.scm` pattern — focused view, `.jl` files only) showing the param the
  cursor is on: `freq = 440  [220:880 step 10]`. Renders nothing when the cursor is
  not on / below a `@param`.

## 3. Latency model — "renders quickly enough"

Split fast feedback from slow feedback:

- **Instant (every keypress):** rewrite the literal in the buffer (`440` → `450`) and
  refresh the statusline readout. This is a pure buffer edit and a status update — no
  kernel involved — so the value visibly moves immediately.
- **Debounced (after ~150 ms of quiet):** fire **one** coalesced kernel re-run of the
  param's own cell, then repaint the resulting `@image` via `plot-resize`'s
  `sync-images-to-markers!`. Rapid or held presses coalesce into a single run using a
  generation counter (increment on each nudge; a scheduled callback runs the re-run
  only if its captured generation still matches) — the same callback-scheduling shape
  as the execution poll backoff (`enqueue-thread-local-callback-with-delay`).

Holding `]p` from 220 → 440 thus produces ~22 instant literal edits but exactly one
kernel re-run, at the final value.

## 4. Re-run scope & stale-cell detection

- **Re-run scope:** the debounced re-run executes **only the param's own cell**. This
  assumes the common "tweak this figure" case where the parameter and the plot live in
  the same cell. It is the fastest, least surprising choice and re-runs no
  side-effecting downstream cells.
- **Stale flagging:** because other cells may reference the changed variable, after a
  re-run nothelix flags downstream staleness without auto-rerunning:
  1. Collect the assigned name(s) of the param cell's `@param` lines (`freq`, `amp`).
  2. Token-scan the code of cells *below* the param cell for those identifiers
     (tokenized word-boundary match, not regex).
  3. Each matching cell gets a **non-destructive overlay tag** near its `@cell`
     header — e.g. `○ stale · freq changed` — painted over the buffer like the image
     layer, never editing the user's text.
  4. A cell's stale tag clears when that cell is executed (`execute-cell` /
     `execute-all-cells` / `execute-cells-above` clear the flag for the cells they
     run).
- **State:** stale flags live in a Steel hash keyed by `(doc-id, cell-index)`; the
  overlay render reads it. Recomputed on each debounced re-run; cleared per-cell on
  execution.
- **Honest limitation (stated in user-facing docs):** this is a name-based heuristic
  with no real dataflow analysis. Reassignment, shadowing, or a name used as an
  unrelated local can cause over- or under-flagging. It is an *alert*, not a guarantee.

## 5. Architecture & components

- **New module `plugin/nothelix/param-tweak.scm`:**
  - `@param` line parser: line text → `{name, value, lo, hi, step, kind}` or `#false`.
  - Nudge logic: clamp to `[lo, hi]`, apply `step`, format per `kind`, rewrite the
    literal token in place.
  - Debounced re-run scheduler (generation counter).
  - Stale computation (assigned-name collection + downstream token-scan).
  - `:param-up` / `:param-down` commands; active-param statusline element.
- **Stale overlay:** reuse the existing paint-over overlay layer (the same primitive
  the conceal / image layers use). The exact Steel entry point is confirmed during
  planning; the design commits only to "non-destructive paint-over, not a buffer edit."
- **Reused, unchanged:**
  - `execute-cell` (the re-run path) — `execution.scm`.
  - `sync-images-to-markers!` (repaint) — `plot-resize.scm` / `image-cache.scm`.
  - cell range / boundary helpers — `cell-boundaries.scm`, `execution.scm`.
  - `enqueue-thread-local-callback-with-delay` (debounce) — Helix misc.
  - `find-…-above` upward-walk pattern — `plot-resize.scm`.
- **Registration:** `param-tweak.scm` required from `plugin/nothelix.scm`; commands +
  keybindings + command-docs added there. **No `libnothelix` change, no FFI version
  bump.**

## 6. Edge cases

- Malformed annotation / non-numeric RHS → treated as ordinary code; status warning on
  nudge attempt.
- Nudge past range → clamp to `lo`/`hi`, status "at min/max".
- User hand-edits the literal → next nudge reads the current literal from the buffer
  (buffer is truth); no drift.
- `@param` is a trailing comment on a code line, so it survives `.ipynb`
  convert / sync round-trips like any other code text.
- Each instant literal edit commits to undo history (a held nudge produces several undo
  steps) — accepted as minor chattiness in v1; coalescing undo is a possible later
  refinement.
- Empty / no `@param` in scope → status "no @param at or above the cursor".

## 7. Testing

Headless Steel tests via the existing temp-`helix.scm`-driver harness:

- **Unit:** parser (`line → {name,value,lo,hi,step,kind}`, including malformed inputs);
  nudge/clamp/format (int vs float precision, range edges); stale token-scan
  (matches a referenced name, ignores a substring/unrelated token).
- **Behaviour (public API):** `:param-up` / `:param-down` move the literal and clamp;
  a debounced re-run is scheduled once for a burst; executing a flagged cell clears its
  stale tag. Tests exercise the commands' observable buffer/state effects, not private
  helpers.

## 8. Future hooks (not built here)

- A transient "tweak mode" (press once, then bare `+`/`-`/arrows until `Esc`) for
  knob-mashing ergonomics — deferred; `]p`/`[p` + debounce suffices for v1.
- Audio: the same nudge → re-run loop drives kernel-side Julia audio synthesis. Its
  own spec.
- Strudel-as-external-engine for live-coding patterns. Its own spec.
