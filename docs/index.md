---
title: Home
nav_order: 1
---

# Nothelix

Jupyter-style notebooks, inside the Helix editor. You execute cells, watch output land inline, and render plots directly in the buffer. No browser.

{: .note }
Julia is the only supported kernel. Inline rendering needs the [Helix fork](https://github.com/koalazub/helix/tree/feature/inline-image-rendering) and a Kitty-protocol terminal.

## What you get

- **A live Julia kernel** that persists state and outlives the buffer.
- **Plots in the buffer** through the Kitty graphics protocol.
- **Notebooks as plain text.** They are `.jl` files with cell markers, and they sync to `.ipynb`.
- **Math that reads like math.** Inline LaTeX becomes Unicode, and display equations become typeset images.
- **Export** to Markdown, Typst, or PDF in one command.

See it live in the [Playground](playground.md).

## Where to go next

| If you want to | Read |
|---|---|
| Get it running | [Installation](install.md) |
| Understand the notebook workflow | [Notebooks](notebooks.md) |
| See math, plots, and tables in the buffer | [Rendering](rendering.md) |
| Try the rendering live in your browser | [Playground](playground.md) |
| Produce a Markdown, Typst, or PDF artifact | [Export](rendering.md#export) |
| Wire up Julia code intelligence | [Language server](lsp.md) |
| Look up a command or keybinding | [Commands and keys](commands.md) |
| Fix something that broke | [Troubleshooting](troubleshooting.md) |
| Understand how it works | [Architecture](architecture.md) |
| Know what it is built on | [Credits](credits.md) |
