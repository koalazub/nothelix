# Colored braille text-plots — design (spec 3 of the output-architecture epic)

## Problem

The user wants terminal-native plots — braille/Unicode rendering (à la
UnicodePlots.jl) instead of raster PNGs — rendered inline as colored text, so
plots look native to the editor, are selectable, and carry no raster payload.

## Approach (decided in earlier design)

- **Activation is implicit**: when a cell's value is a **UnicodePlots** plot
  (via `unicodeplots()` backend or `using UnicodePlots`), the kernel renders
  braille instead of a PNG. A `.nothelix.conf` `plot-mode = auto|raster|braille`
  knob (default `auto`) can force either regardless of backend.
- **Color model — kernel pre-segments** (chosen): the kernel parses the ANSI SGR
  codes UnicodePlots emits and produces braille **text + a parallel color-span
  map** (`offsets → color`), so the plugin applies overlays without parsing ANSI
  itself.

## Design

### Kernel (`kernel/output_capture.jl`)

- Detect a UnicodePlots plot in `capture_toplevel`'s per-expression loop
  (alongside `is_displayable_plot`). When detected (or `plot-mode = braille`):
  `show(io, plot)` capturing the ANSI text; parse SGR runs into
  `{ "text_plot": { "rows": ["…braille…"], "spans": [[row, start, end, color], …] } }`
  where `color` is an ANSI palette index (0-7 + bright). Emit as a new output
  kind in the result JSON, added to the ordered outputs array.
- UnicodePlots must be in the kernel env (`runner.jl` self-heals via `Pkg.add`
  as it does for JSON3 — see the kernel-env-rot reference).

### Fork — styled `output_lines` (`/Users/koalazub/projects/helix`)

Plan 1's `output_lines` annotation renders monochrome `Vec<String>` rows and the
`OutputAnnotations` decoration paints one style. Extend it to carry **per-span
style**:

- `OutputLines` rows become `Vec<Vec<StyledSpan>>` where
  `StyledSpan = { text: String, scope: Option<String> }` (or add a parallel
  `Vec<Vec<(range, scope)>>` store keyed the same way), plus a dedicated
  `ui.virtual.output` theme scope (Plan 1 reused `ui.virtual.math`).
- The decoration resolves each span's scope against the active theme at render
  (same mechanism as `plugin_style_highlights`).
- Steel binding extended: `set-output-lines-below!(line, rows)` where each row is
  a list of `(text scope-or-false)` spans (backward-compatible: a plain-string
  row = one span, no scope).

### Plugin

- `output-store.scm`: store the braille rows + spans (extend the encoded blob).
- On execute/reopen: map each ANSI palette index → a nothelix plot-color theme
  scope (a fixed set `ui.virtual.output.series0..7`), build the styled rows, and
  `set-output-lines-below!`. A text-plot is just another store output kind whose
  renderer is the styled annotation — it stacks alongside raster images and text
  like any other output.

### Interaction

- Multi-graph (spec 2) + braille compose: k text-plots are k styled-row entries
  in the ordered outputs array; raster and braille plots can coexist in one cell.
- Undo: braille is virtual rows → zero undo cost (like Plan-1 text), no
  dependency on Plan 2's tagged primitive.

## Non-goals

- Arbitrary RGB series palettes — the theme-scope model covers the ANSI 8/16
  palette UnicodePlots uses; true-color would need the annotation to accept raw
  colors (a later extension), noted but not built.
- Converting Plots.jl/Makie raster plots to braille — the user opts into braille
  by using a unicode backend.

## Testing

- Kernel: a UnicodePlots plot yields `text_plot` with rows + non-empty spans;
  ANSI stripped from `text`; palette indices in range.
- Fork: styled `output_lines` reserves rows and paints per-span scopes; a
  plain-string row still renders (backward compat).
- Manual (live): `unicodeplots(); plot(x,y)` renders colored braille inline;
  colors track series; reopen restores; stacks with a raster plot in the same
  cell.
