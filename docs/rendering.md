---
title: Rendering
nav_order: 4
---

# Rendering

The point of nothelix is that the visual results of your work — plots, equations,
tables — appear where you are looking, in the buffer, rather than in a separate
window. This page covers what renders, how, and what your terminal needs to make
it happen.

## Plots

When a cell produces a figure, it renders inline below the cell. Plots travel
from Julia as PNG images through the standard display system, and the terminal
draws them through its graphics protocol. The kernel never thinks about terminals;
it writes pixels, and nothelix handles the wire format. See
[Architecture](architecture.md#the-kernel-protocol) for the full path.

A few commands help you work with figures:

- `:view-chart` opens the last-executed plot in an interactive chart viewer.
- `:plot-grow` (`<space>n=`) and `:plot-shrink` (`<space>n-`) resize the plot
  block under the cursor and re-render it at the new size.
- `:insert-image` drops a `# @image <path>` marker and a blank canvas at the
  cursor, so you can place an existing image file in a notebook.

## Mathematics

Nothelix renders mathematics in two complementary ways, depending on whether it
is inline with your prose or set off on its own.

### Inline math becomes Unicode

Inside `$ … $`, LaTeX is replaced with Unicode as you read it. `\alpha` becomes
α, `\leq` becomes ≤, superscripts and subscripts shift to their Unicode forms,
and matrix and cases environments get their bracket fences. Because the result is
plain text — one glyph per source construct — inline math flows with your writing
and shows up anywhere, including under a multiplexer.

This is concealment: the raw LaTeX is still in the file, and it reappears on the
cursor's line while you edit, so you are never editing blind. `:conceal-math` and
`:clear-conceal` turn it on and off for a buffer; it is applied automatically when
you open a notebook.

While writing, press `Tab` (in insert mode, in a `.jl` file) to expand a Julia
LaTeX shortcut at the cursor — type `\alpha`, press `Tab`, get α — the same
mechanism Julia's REPL uses.

### Display math becomes a typeset image

A display block set off with `$$ … $$` renders as a properly typeset image in the
buffer. The LaTeX is converted to Typst, compiled to a vector image, rasterised,
and drawn inline. The result looks like real mathematics, not an ASCII
approximation.

| Command | What it does |
|---|---|
| `:render-math-at-cursor` | Render the display block under the cursor |
| `:render-all-display-math` | Render every display block in the buffer |
| `:clear-math-images` | Remove the rendered images and show the source again |
| `:format-math-buffer` | Expand single-line `\begin{…}` environments into multi-line `$$` blocks |
| `:math-render-buffer` | Stack big-operator limits (`\int`, `\sum`, `\prod`) above and below the glyph |
| `:math-render-clear` | Remove the stacked-limit annotations |

Display math is rendered automatically when you open a notebook and refreshed when
you save. The conversion uses the [MiTeX](credits.md) project's LaTeX support inside
Typst; the [architecture](architecture.md#mathematics-rendering) page and the design
note in `docs/dev/math-rendering-strategy.md` explain the layout strategy, which
has a few real subtleties around reserving vertical space for the image.

## Tables

A Markdown pipe table written as comments renders as a typeset image too, through
the same Typst path as display math. `:render-all-tables` renders every table in
the buffer; they refresh on save alongside the math. As with plots and equations,
the source text stays in the file and comes back when you need to edit it.

## Graphics protocols

Nothelix detects your terminal's graphics capability and uses the best available
path. Run `:graphics-check` to see what it found, and `:graphics-protocol` for the
short answer.

| Protocol | Used for | Notes |
|---|---|---|
| Kitty graphics protocol | Plots, display math, tables | The best path; full inline images. Kitty is the reference implementation, and other terminals that implement the protocol qualify. |
| iTerm2 inline images | Plots | Supported where the Kitty protocol is not present. |
| Text fallback | — | When no image protocol is available, image-based content falls back to placeholders. Inline Unicode math still works, since it is plain text. |

Sixel is not implemented. There is no per-terminal quality score here on purpose;
what matters is whether your terminal speaks an image protocol, which
`:graphics-check` tells you directly.

### The multiplexer caveat

Terminal multiplexers — tmux and Zellij — intercept escape sequences and strip
the ones the Kitty protocol relies on. Under them, anything image-based stops
appearing: plots, typeset display math, and rendered tables. Inline Unicode math
and concealment keep working, because they are plain text. Run Helix directly in a
Kitty-protocol terminal, or use a multiplexer that forwards the protocol
untouched.

Inline rendering also depends on the Helix fork; on stock Helix, images and
stacked-math limits fall back to placeholders. See
[Architecture](architecture.md#why-a-fork) for why.
