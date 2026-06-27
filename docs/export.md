---
title: Export
nav_order: 5
---

# Export

A notebook is where you do the work; an export is what you hand to someone else.
Nothelix turns a `.jl` notebook into three formats, each with a single command.

| Command | Output | Good for |
|---|---|---|
| `:export-markdown` | `.md` | A readable Markdown document with your prose, code, and math |
| `:export-typst` | `.typ` | A Typst source file you can typeset or edit further |
| `:export-pdf` | `.pdf` | A finished, typeset PDF |

Each export runs off the editor's main thread, so a large notebook does not freeze
Helix while it is produced.

## What each format gives you

**Markdown** is the lowest-friction option. Your markdown cells become Markdown,
your code cells become fenced code blocks, and math is preserved as LaTeX. It is
the right choice when the destination is a README, a wiki, a static-site
generator, or anything that already understands Markdown and LaTeX.

**Typst** is the intermediate form, and the interesting one. Nothelix converts the
LaTeX in your notebook to [Typst](credits.md) — a modern typesetting system — so the
mathematics is expressed natively rather than embedded as raw LaTeX. The result is
a `.typ` file you can typeset yourself, drop into a larger Typst document, or keep
editing. The LaTeX-to-Typst conversion is handled by the
[MiTeX](credits.md) project.

**PDF** is the same Typst pipeline taken one step further: the converted document
is compiled straight to a typeset PDF. This is the path to a self-contained
artifact — a figure-and-equation-bearing document you can attach to an email or a
submission — without leaving the editor or installing a LaTeX distribution.

## How the math survives

The reason export is more than a text dump is that nothelix already understands
your mathematics. The same LaTeX-to-Typst machinery that
[renders display math in the buffer](rendering.md#display-math-becomes-a-typeset-image)
drives Typst and PDF export, so an equation that displays correctly inline is the
same equation that lands in your PDF. Display blocks, inline math, headings, and
emphasis are all carried across rather than flattened.

For the pipeline internals — the Typst compilation path, the embedded fonts, and
why MiTeX needs a vendored scope to compile — see
[Architecture](architecture.md#mathematics-rendering).
