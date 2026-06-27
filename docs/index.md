---
title: Home
nav_order: 1
---

# Nothelix

Jupyter-style notebooks, inside the Helix editor.

Nothelix runs notebooks in Helix. You execute cells, watch their output land in
place, and see plots render right in the buffer — all from the modal editor you
already use, and without ever opening a browser.

It exists for a specific kind of person: a researcher or scientist who already
lives in a terminal, thinks in a modal editor, and resents being pulled into a
browser tab to do the computational part of their work. If you have spent a
dissertation chapter or a paper's worth of analysis switching between an editor
for writing and a notebook server for running, this is the tool that tries to
collapse that gap.

## What you get

- **A real kernel, in your editor.** Cells execute against a live Julia process
  that persists state between runs, the way a notebook should. Output appears
  inline, below the cell that produced it.
- **Plots in the buffer.** Figures render directly in the terminal through the
  Kitty graphics protocol. No image viewer, no browser, no round trip.
- **Notebooks as plain text.** A `.ipynb` becomes a readable `.jl` file with
  explicit cell markers, so it diffs cleanly, edits at the speed of modal
  motions, and syncs back to `.ipynb` when collaborators need it.
- **Mathematics that reads like mathematics.** Inline LaTeX becomes Unicode as
  you type, and display equations render as typeset images in the buffer.
- **Export when you are done.** Turn a notebook into Markdown, Typst, or a
  typeset PDF with a single command.

## Where to go next

| If you want to | Read |
|---|---|
| Get it running | [Installation](install.md) |
| Understand the notebook workflow | [Notebooks](notebooks.md) |
| See math, plots, and tables in the buffer | [Rendering](rendering.md) |
| Produce a Markdown, Typst, or PDF artifact | [Export](export.md) |
| Wire up Julia code intelligence | [Language server](lsp.md) |
| Look up a command or keybinding | [Commands and keys](commands.md) |
| Understand how it works | [Architecture](architecture.md) |
| Know what it is built on | [Credits](credits.md) |
| Fix something that broke | [Troubleshooting](troubleshooting.md) |

## A note on honesty

Nothelix is built for and around a real research workflow, and it tells you the
truth about its edges. Julia is the only supported kernel today. Inline rendering
depends on a [fork of Helix](https://github.com/koalazub/helix/tree/feature/inline-image-rendering)
and a terminal that speaks the Kitty graphics protocol. Where a feature is not
finished, the documentation says so plainly rather than implying otherwise.

Nothelix is MIT licensed and built on a great deal of open-source work. The
[credits](credits.md) page names each project it depends on and explains why.
