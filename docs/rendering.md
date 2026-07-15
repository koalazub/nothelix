---
title: Rendering
nav_order: 4
---

# Rendering

The [Notebooks](notebooks.md) page covers writing and running cells. This page
picks up where the run finishes and follows what becomes of the results. Every
figure, equation, and table nothelix produces is drawn in the buffer itself
through inline rendering, never in a separate pane or a browser. Here is what
each content type turns into, and when.

| Content | Renders as | When |
|---|---|---|
| Plots | Inline image(s) below the cell, stacked | On cell run |
| Inline math `$…$` | Unicode overlay | On open, or `:conceal-math` |
| Display math `$$…$$` | Typeset image | On open and save, or `:render-math-at-cursor` |
| Pipe tables | Typeset image | On save, or `:render-all-tables` |

The rest of this page walks each path in turn. It states what the render does,
the commands that drive it, and what your terminal needs to show it.

## Plots

A code cell that produces a figure renders it inline, below the cell. If a cell
produces several plots, every one renders, stacked in order, not just the first.
Re-running a cell replaces its plots. The stack is capped at `plots-per-cell`, a
`.nothelix.conf` key that defaults to `32` and clamps into the range `1..256`. A
cell that exceeds the cap logs a note rather than silently dropping the rest.

<!-- SCREENSHOT NEEDED: a stacked multi-plot cell showing several images below one code cell -->

| Command | Key | What it does |
|---|---|---|
| `:view-chart` | | Open the last-executed plot in the interactive chart viewer |
| `:plot-grow` | `<space>n=` | Grow the plot block under the cursor and re-render |
| `:plot-shrink` | `<space>n-` | Shrink the plot block under the cursor and re-render |
| `:insert-image` | | Drop a `# @image <path>` marker and blank canvas at the cursor |

Plots travel from Julia as PNG through the display system, and the terminal
draws them over its graphics protocol. See
[Architecture](architecture.md#the-kernel-protocol) for the full path.

A `# @param` annotation turns a numeric literal into a live knob. Nudge it with
`]p` or `[p` and the figure re-renders in place. Write the annotation as a
trailing comment on the same line as the parameter, using the form
`freq = 440   # @param 220:880 step 10`. The range is `<lo>:<hi>` and the
`step <s>` clause is optional, defaulting to `1` for an integer and to a
hundredth of the span for a float. The plugin handles the instant literal edit
and the debounced cell re-run.

## Braille plots

A cell that uses the UnicodePlots.jl backend, such as
`using UnicodePlots; lineplot(...)`, renders as coloured braille text inline
instead of a raster image. The braille output is selectable text and needs no
image protocol at all. Activation is implicit. Nothelix detects a UnicodePlots
value and switches to the braille path automatically. Braille and raster plots
can stack together in the same cell. Per-series colour needs a current fork
build. On an older build the braille glyphs still render, just in monochrome.

The `.nothelix.conf` key `plot-mode` takes `auto`, `raster`, or `braille` and
defaults to `auto`. The kernel acts on it every time a cell produces a plot.
Under `raster` every plot renders as a raster image, so even a UnicodePlots
value is drawn through the image protocol instead of as braille. Under `auto`
or `braille` a UnicodePlots value renders as braille text and any other figure
renders as a raster image.

<!-- SCREENSHOT NEEDED: a braille UnicodePlots cell rendered as coloured text -->

## Inline math becomes Unicode

Inside a `$ … $` span, LaTeX is concealed as Unicode while you read. Conceal
means the raw LaTeX is replaced in place by its Unicode form, so `\alpha`
becomes α, `\leq` becomes ≤, and superscripts and subscripts shift to their
Unicode variants. The Fourier transform shows the before and after, as typed and
as read.

![Fourier transform LaTeX before and after conceal](assets/img/fourier-conceal.png)

The concealed pane is the engine's exact output, and those lines are pinned
byte-for-byte by unit tests. For camera-ready typesetting, put the same formula
in a `$$ … $$` block and it renders as a Typst image instead.

| Command | What it does |
|---|---|
| `:conceal-math` | Apply LaTeX-to-Unicode concealment to the buffer |
| `:clear-conceal` | Remove the concealment overlays |
| `:julia-tab-complete` | Expand a `\<name>` shortcut at the cursor (`Tab`, insert mode, `.jl`) |

Beyond the Julia REPL names, the unicode-math `\Bbb*` names such as `\BbbR` and
`\Bbbzero` expand, as do friendly set aliases such as `\Reals` and `\Integers`.
The raw LaTeX stays in the file and reappears on the cursor's line while you
edit. The concealed result is plain text, so it flows with prose and keeps
working under a multiplexer. Concealment applies automatically on open, unless a
project sets `conceal-on-open = false` in `.nothelix.conf`.

Try it live on the [Playground](playground.md).

## Display math becomes a typeset image

A `$$ … $$` block renders as a typeset image. Nothelix converts the LaTeX to
Typst, compiles it to vector, rasterises it, and draws it inline.

<!-- SCREENSHOT NEEDED: a $$ block before-and-after, source and its typeset image, parallel to the inline-math figure -->

| Command | What it does |
|---|---|
| `:render-math-at-cursor` | Render the display block under the cursor |
| `:render-all-display-math` | Render every display block in the buffer |
| `:clear-math-images` | Remove the images and show the source again |
| `:format-math-buffer` | Expand single-line `\begin{…}` into multi-line `$$` blocks |
| `:math-render-buffer` | Stack big-operator limits (`\int`, `\sum`, `\prod`) above and below the glyph |
| `:math-render-clear` | Remove the stacked-limit annotations |

Display math renders on open and refreshes on save. The conversion uses
[MiTeX](credits.md), and [Architecture](architecture.md#mathematics-rendering)
covers the layout strategy. Try it live on the [Playground](playground.md).

## Tables

A Markdown pipe table written as comment lines renders as a typeset image,
through the same Typst path as display math.

| Command | What it does |
|---|---|
| `:render-all-tables` | Render every Markdown pipe table in the buffer |

Tables refresh on save alongside math. The source text stays in the file and
returns when you edit.

## Graphics protocols

Run `:graphics-check` for a diagnostic, or `:graphics-protocol` for the short
answer.

| Protocol | Used for | Notes |
|---|---|---|
| Kitty graphics protocol | Plots, display math, tables | Best path, full inline images. Any terminal implementing the protocol qualifies |
| iTerm2 inline images | Plots | Used where the Kitty protocol is absent |
| Text fallback | Placeholders | With no image protocol, image content shows placeholders. Unicode math still works |

Sixel is not implemented.

### Caveats

- **Multiplexers.** tmux and Zellij strip the escape sequences the Kitty
  protocol relies on. Under them, all image content stops appearing, so plots,
  display math, and tables go blank, while Unicode math and concealment keep
  working. Run Helix directly in a Kitty-protocol terminal.
- **The fork.** Inline rendering depends on the Helix fork. On stock Helix,
  images and stacked-math limits fall back to placeholders. See
  [Architecture](architecture.md#why-a-fork).

## Export

Turn a `.jl` notebook into a shareable document. Each export runs off the main
thread, so a large notebook does not freeze Helix.

| Command | Output | Good for |
|---|---|---|
| `:export-markdown` | `.md` | A README, wiki, or static-site generator that understands Markdown and LaTeX |
| `:export-typst` | `.typ` | A Typst source file to typeset, edit, or drop into a larger document |
| `:export-pdf` | `.pdf` | A finished, self-contained PDF, with no LaTeX distribution needed |

Typst and PDF export use the same LaTeX-to-Typst machinery
([MiTeX](credits.md)) as display math, so an equation that displays inline is
the equation that lands in the PDF. Pipe tables become native Typst `#table()`
blocks, and headings, inline math, and non-ASCII text carry across verbatim.
Watch it run on the [Playground](playground.md).
