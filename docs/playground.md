---
title: Playground
nav_order: 5
---

# Playground

Everything nothelix renders in the buffer runs through one Rust core,
`libnothelix`. That core also compiles to WebAssembly, so the widgets below run
the actual nothelix code in your browser. The text you type is handed to the
exact functions the editor calls, not a reimplementation of them.

There is no server and no round trip. About 400&nbsp;KB of WebAssembly loads
once when you open the page, and everything after that runs locally.

{% include eng/styles.html %}
{% include eng/wasm-loader.html %}

## Math reads like mathematics

In a notebook, inline LaTeX inside `$…$` is concealed to Unicode as you type.
`\alpha` becomes α, `\mathbb{R}` becomes ℝ, and `x_1` becomes a real subscript.
The source on disk stays plain LaTeX, and only the display changes. Type into
the box and watch the buffer line update the way the editor would.

{% include eng/conceal.html %}

## Display equations, typeset

A `$$…$$` block is laid out as a typeset equation. Type LaTeX into the box and
watch it set.

{% include eng/mathrender.html %}

## Backslash completion

Start a symbol with a backslash and nothelix offers the Unicode it maps to. This
is the same list the completion popup shows in the editor.

{% include eng/completion.html %}

## Export to Typst

`:export-typst` turns a markdown cell into Typst source, carrying headings,
emphasis, and LaTeX math across. Tables become native Typst tables, and headings
keep their math.

{% include eng/mdtypst.html %}

The equation above is typeset by KaTeX for this page. In the editor the same
LaTeX is rendered by Typst into an inline image. See [Rendering](rendering.md)
for how that reaches the buffer.
