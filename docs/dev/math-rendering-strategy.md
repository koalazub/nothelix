# Nothelix math rendering strategy

Goal: render math and rich symbols fully inside Helix, terminal-native, so a
researcher opens a notebook and goes — no browser, no Jupyter, no app-switching.

## The constraint

A terminal is a fixed grid of monospace cells. There is no proportional inline
layout, so we cannot match a browser's MathJax/KaTeX flow. Two primitives exist:

- **Unicode glyph substitution** — one grapheme per cell, flows with text. Good
  for symbols (`α ω ∈ ∑`), powerless for 2D structure (real fractions, matrices).
- **Kitty graphics image** (KGP) — high fidelity, but a rectangular block that
  cannot flow inline. The only image backend nothelix targets.

The Helix fork paints Kitty images **over real buffer cells**. Its earlier
phantom-row auto-reservation was removed because it broke scroll, cursor nav and
page-up/down (`helix-term/src/ui/document.rs`). Consequence, and the load-bearing
rule for everything below:

> **The plugin must supply real blank buffer lines equal to an image's height.**
> An image is never taller than the lines beneath its anchor; to render bigger,
> add lines.

## Prior art

- **euporie** (Jupyter notebooks in the terminal): tiered — terminal graphics
  (kitty/sixel/iterm) when available, Unicode text otherwise.
- **latex-terminal, termtex, Terminatex, mdmath.nvim**: LaTeX → image (matplotlib
  or MathJax) → Kitty graphics. Same pipeline shape as our display path.
- **SymPy pretty-print**: true 2D math in pure monospace — stacked fractions with
  `─`, `⎡ ⎤` matrix brackets, `∑ ∫` with limits above/below. Terminal-native math
  with no image at all. This is our missing fallback tier.
- **Protocol**: nothelix targets the Kitty graphics protocol (KGP) **only** — it
  is the trend-leader and what the fork's image path already speaks. No Sixel or
  iTerm2 backend; non-Kitty terminals fall back to the T3 Unicode tier.

## Tiered model

- **T0 — capability detect** (`graphics.scm`): Kitty graphics available, or not.
- **T1 — inline `$...$`**: Unicode overlays. One row, flows with prose. Keep inline
  strictly single-row; anything needing 2D is promoted to display.
- **T2 — display `$$...$$`, graphics available**: Typst → SVG → raster → Kitty
  image, rendered at the equation's **natural height** by reserving blank buffer
  lines. Width clamped to the view.
- **T3 — display `$$...$$`, no Kitty graphics**: 2D Unicode art (SymPy-pretty
  style) via multi-line overlays / the fork's math-line annotations. Makes
  nothelix usable under tmux and on non-Kitty terminals.

## Space reservation (the core mechanism)

Because images paint over real lines, the plugin owns vertical space explicitly.

- Blank `# ` lines (empty body) **inside** a `$$` block are renderer-owned
  reservation space — a user never writes meaningful blank lines there.
- Render cycle:
  1. Render the batch (async) to obtain each block's intrinsic size.
  2. Per block, `desired = natural rows`, capped by `*math-image-target-rows*`
     and the view height.
  3. One **atomic whole-buffer rewrite** (same approach as `format-math-buffer`):
     strip renderer-owned blank lines in each block, then insert `desired` blank
     lines. A single replace avoids anchor-shift races between blocks.
  4. Re-collect blocks and place each image over its now-reserved lines.
- Single-line `$$ X $$` is normalized to multi-line by `format_math` first, so it
  has interior space to reserve.
- Idempotent: blank-in-block lines are regenerated every cycle, so re-render and
  reopen converge.
- Width: cols clamped to `view_width - margin`, rows scaled to preserve aspect
  (shipped).

## LaTeX → Typst conversion: MiTeX (de-risked, NOT a drop-in)

The hand-rolled 1,331-line `typst_export.rs` converter (`latex_to_typst_math`)
works and passes the corpus. The hope was to replace it with the
[`mitex`](https://crates.io/crates/mitex) crate (Apache-2.0). A spike pinned
`mitex = "0.2.4"` and exercised `convert_math(&str, Option<CommandSpec>) ->
Result<String, String>` against our corpus. Findings:

- Builds clean, parses LaTeX correctly, pulls a moderate dep tree
  (rkyv/rowan/logos/mitex-spec). One incidental fix needed:
  `graphics.rs` `Option<&[u8]> == Some(b"WEBP")` became ambiguous under rkyv's
  blanket `PartialEq`; use `Some(b"WEBP".as_slice())`.
- **Output is NOT standalone Typst.** It targets the MiTeX Typst *package*
  runtime — `mitexsqrt`, `bmatrix`, `zws`, `thin`, `tfrac`, `#textmath`, `wide`,
  etc. Compiled under our bare typst, ~8/13 simple cases passed; matrices,
  `cases`, `\sqrt[3]`, `\tfrac`, `\qquad` all failed.
- Adopting it therefore means **vendoring the MiTeX Typst scope tree**
  (`packages/mitex/specs/mod.typ` → `latex/standard.typ` + helpers), evaluating
  conversions with that scope (`typst::eval` with a prelude module), and
  verifying the whole thing compiles under the pinned typst 0.15. That is a
  coupled sequential integration, not a parallel fan-out or a drop-in.

**Status: spike reverted, tree clean.** Decision deferred behind T2 (below),
which is the bigger user-visible win. Revisit MiTeX only if converter edge-case
churn becomes the bottleneck; if adopted, keep `build_typst_document` (page,
colour, frame) and parity-gate against an expanded `math_render_corpus.rs`.

## Implementation order

1. ~~**T2 natural height via reservation**~~ — DONE (see below).
2. **T3 2D Unicode fallback** — math for no-graphics / tmux users.
3. Inline-promotion rule (T1 → T2 when a `$...$` needs 2D).
4. **MiTeX conversion swap** (deferred) — only if converter edge-case churn
   becomes the bottleneck; needs the vendored MiTeX Typst scope (see above).

## Shipped this session

- O(n²) Steel string freeze on the batch reply (→ native `split-many`/`split-once`).
- Display-math horizontal overflow (cols clamped to view width).
- Codebase comment sweep.
- **T2 natural-height reservation.** Two new pure Rust FFI transforms in
  `math_format.rs`: `math_block_latex_batch` (canonicalise → enumerate blocks →
  pad-invariant LaTeX blob for the renderer) and `reserve_math_lines` (canonicalise
  → strip the trailing renderer-owned blank run → expand single-line `$$…$$` →
  repad to `nat − 2` interior lines). The render cycle (`math-image.scm`) is now
  dims-first: render the blob → measure each block's natural rows → one atomic
  whole-buffer reserve (mirrors `format-math-buffer`, skipped when unchanged so a
  converged reopen makes zero edits) → re-collect anchors → place over the now
  full-height block. Auto-on-open per the chosen UX. Idempotent: emit/strip share
  one empty-body predicate; LaTeX is pad-invariant so the spec is stable. Both
  transforms share one `enumerate_blocks`, so the k-th rendered result, spec
  entry, and reserved block stay aligned. FFI version 12 → 13. Covered by 14
  nextest cases (padding, idempotency, single-line expansion, mid-block blank
  preservation, bare-`#` detection, CRLF, garbage spec).
