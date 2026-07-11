---
title: Rendering
nav_order: 4
---

# Rendering

| Content | Renders as | When |
|---|---|---|
| Plots | Inline image below the cell | On cell run |
| Inline math `$…$` | Unicode overlay | On open, or `:conceal-math` |
| Display math `$$…$$` | Typeset image | On open and save, or `:render-math-at-cursor` |
| Pipe tables | Typeset image | On save, or `:render-all-tables` |

Everything renders in the buffer. Below: what each does, the commands that drive
it, and what your terminal needs.

## Plots

A cell that produces a figure renders it inline, below the cell.

| Command | Key | What it does |
|---|---|---|
| `:view-chart` | | Open the last-executed plot in the interactive chart viewer |
| `:plot-grow` | `<space>n=` | Grow the plot block under the cursor and re-render |
| `:plot-shrink` | `<space>n-` | Shrink the plot block under the cursor and re-render |
| `:insert-image` | | Drop a `# @image <path>` marker and blank canvas at the cursor |

Plots travel from Julia as PNG through the display system; the terminal draws them
over its graphics protocol. See [Architecture](architecture.md#the-kernel-protocol)
for the full path.

A `# @param` annotation turns a numeric literal into a live knob — nudge it with
`]p` / `[p` and the figure re-renders in place. Write `freq = 440   # @param 220:880 step 10`
on the same line as the parameter, and the plugin handles the instant literal edit
and debounced cell re-run.

## Inline math becomes Unicode

Inside `$ … $`, LaTeX is concealed as Unicode as you read: `\alpha` becomes α,
`\leq` becomes ≤, superscripts and subscripts shift to their Unicode forms.

| Command | What it does |
|---|---|
| `:conceal-math` | Apply LaTeX-to-Unicode concealment to the buffer |
| `:clear-conceal` | Remove the concealment overlays |
| `:julia-tab-complete` | Expand a `\<name>` shortcut at the cursor (`Tab`, insert mode, `.jl`) |

- The raw LaTeX stays in the file and reappears on the cursor's line while you edit.
- The result is plain text, so it flows with prose and works under a multiplexer.
- Applied automatically on open, unless a project sets `conceal-on-open = false` in
  `.nothelix.conf`.

Try it live on the [Playground](playground.md).

## Display math becomes a typeset image

A `$$ … $$` block renders as a typeset image: LaTeX to Typst, compiled to vector,
rasterised, drawn inline.

| Command | What it does |
|---|---|
| `:render-math-at-cursor` | Render the display block under the cursor |
| `:render-all-display-math` | Render every display block in the buffer |
| `:clear-math-images` | Remove the images and show the source again |
| `:format-math-buffer` | Expand single-line `\begin{…}` into multi-line `$$` blocks |
| `:math-render-buffer` | Stack big-operator limits (`\int`, `\sum`, `\prod`) above/below the glyph |
| `:math-render-clear` | Remove the stacked-limit annotations |

Display math renders on open and refreshes on save. The conversion uses
[MiTeX](credits.md); see [Architecture](architecture.md#mathematics-rendering) for
the layout strategy. Try it live on the [Playground](playground.md).

## Tables

A Markdown pipe table written as comments renders as a typeset image, through the
same Typst path as display math.

| Command | What it does |
|---|---|
| `:render-all-tables` | Render every Markdown pipe table in the buffer |

Tables refresh on save alongside math. The source text stays in the file and
returns when you edit.

## Graphics protocols

Run `:graphics-check` for a diagnostic, or `:graphics-protocol` for the short answer.

| Protocol | Used for | Notes |
|---|---|---|
| Kitty graphics protocol | Plots, display math, tables | Best path; full inline images. Any terminal implementing the protocol qualifies |
| iTerm2 inline images | Plots | Used where the Kitty protocol is absent |
| Text fallback | — | No image protocol: image content shows placeholders. Unicode math still works |

Sixel is not implemented.

### Caveats

- **Multiplexers.** tmux and Zellij strip the escape sequences the Kitty protocol
  relies on. Under them, all image content (plots, display math, tables) stops
  appearing; Unicode math and concealment keep working. Run Helix directly in a
  Kitty-protocol terminal.
- **The fork.** Inline rendering depends on the Helix fork. On stock Helix, images
  and stacked-math limits fall back to placeholders. See
  [Architecture](architecture.md#why-a-fork).

## Export

Turn a `.jl` notebook into a shareable document. Each export runs off the main
thread, so a large notebook does not freeze Helix.

| Command | Output | Good for |
|---|---|---|
| `:export-markdown` | `.md` | A README, wiki, or static-site generator that understands Markdown + LaTeX |
| `:export-typst` | `.typ` | A Typst source file to typeset, edit, or drop into a larger document |
| `:export-pdf` | `.pdf` | A finished, self-contained PDF, no LaTeX distribution needed |

Typst and PDF export use the same LaTeX-to-Typst machinery ([MiTeX](credits.md)) as
display math, so an equation that displays inline is the equation that lands in the
PDF. Pipe tables become native Typst `#table()` blocks; headings, inline math, and
non-ASCII text carry across verbatim. Watch it run on the
[Playground](playground.md).
