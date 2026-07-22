---
title: Architecture
nav_order: 9
---

# Architecture

This page is for contributors. None of it is needed to use nothelix. It explains how the system is shaped and why it depends on a forked editor. Deeper working notes live under `docs/dev/` in the source tree.

## Three code layers, plus the fork

The walkthrough below follows one cell, `plot(x, y)`, from keystroke to inline figure. Step through it with the arrows, or let it play.

{% include eng/styles.html %}
{% include eng/arch.html %}

Three code layers cooperate, and each does the work it is best at. Underneath them sits the Helix fork, which is not code nothelix ships but the editor it runs inside. The next section covers the fork on its own.

| Layer | Handles | Examples |
|---|---|---|
| **Rust** (libnothelix) | Everything system-shaped | Notebook JSON parsing, kernel process management, image decode and Kitty encode, Typst compilation, error enrichment |
| **Steel** | Everything editor-shaped | Commands, keymaps, the cell navigator, overlay and image placement, document lifecycle hooks |
| **Julia** (the kernel) | Everything language-shaped | Cell execution, dependency tracking, runtime type introspection for hints |

The split is deliberate. JSON parsing and image encoding were slow in Steel on large notebooks, so heavy data work lives in Rust. Steel is the caller and the UI layer, where you can iterate on the experience without recompiling anything.

## Why a fork

Stock Helix compiles and loads nothelix. Three capabilities live only on the [koalazub/helix `feature/inline-image-rendering`](https://github.com/koalazub/helix/tree/feature/inline-image-rendering) branch, which is the Helix fork.

| Fork addition | What it does |
|---|---|
| **RawContent API** | Attaches image payloads to a document. The terminal draws them through Kitty's Unicode-placeholder protocol, so images survive scrolling and edits. |
| **Math line annotations** | Adds virtual rows above and below source lines, used to stack big-operator limits without touching the text. |
| **Steel FFI surfaces** | Exposes both of the above to Scheme. |

The design is a dumb core with smart plugins. The core grew exactly one new idea. A plugin can emit raw bytes at a document position, and those bytes consume some visual rows. The core knows nothing about image formats, terminal protocols, or notebooks, and the plugin supplies all the policy. The rendering change is a small diff across the formatter, the buffer, and the terminal flush.

On stock Helix, image rendering and stacked-math limits fall back to placeholders. The plugin guards every fork FFI, so nothing crashes. You simply lose those two features.

## The kernel protocol

The kernel talks over file-based JSON. One long-lived Julia process runs per open `.ipynb` or `.jl` document, staying alive until `:kernel-shutdown`. A single execution flows in four steps.

1. Steel asks libnothelix to start the cell. The library writes the cell code and index to `input.json` in the kernel's scratch directory.
2. The kernel reads `input.json` and runs the code in a persistent module. Definitions accumulate across cells, the way they do in a REPL. It registers dependencies, captures output, and catches errors.
3. The kernel writes `output.json` with text output, base64 PNG images, structured errors, and a fresh cell-registry snapshot. It then writes an `output.json.done` sentinel file to signal completion.
4. Steel polls without blocking the editor. Once the sentinel appears it reads `output.json`, renders the output in place, and registers images through the RawContent API.

Plots travel as base64 PNGs through Julia's standard `MIME("image/png")` display system. The kernel writes pixels and libnothelix handles the wire format.

## Error enrichment

The kernel emits runtime errors as structured JSON. libnothelix folds source context, cross-cell context, and kernel-side type hints into the message you finally see. Toggle the views below to compare the raw error with the enriched one.

{% include eng/enrich.html %}

| Error | Enrichment |
|---|---|
| `UndefVarError` | Adds a note on where the variable is defined, or would be. If the kernel has not indexed it, a static `.jl` scanner in Rust catches the common case of a variable defined in a later cell that has not run yet. |
| `MethodError` | Draws on two kernel snapshots, the recorded type of every binding and the in-scope values a function could accept. It renders a variables-by-type block alongside the viable candidates. |
| `DimensionMismatch`, `BoundsError`, `ParseError` | Source-line-aware enrichers re-express the error using your real variable names rather than generic placeholders. |

Take `no method matching (Matrix)(::Vector{ComplexF64})` as an example. The enriched message points to the variable in scope that holds the stray `Vector{ComplexF64}`, and to the in-scope value `Matrix()` would have accepted.

Here is a real `MethodError` block, written straight out of the engine by `just gallery`. A signature too long to read in one line becomes a list of your own argument names against the types they actually hold, which is the difference between guessing and knowing which argument is wrong.

```text
{% include engine/error-long-message.txt %}
```

## Mathematics rendering

Inline math runs through a single-pass Unicode scanner. It walks the text inside each `$ … $` region and produces overlay pairs that swap LaTeX constructs for Unicode glyphs. The pass is linear with no pathological cases.

Display math needs real typesetting, and it takes two steps. First nothelix converts the LaTeX to Typst through [MiTeX](credits.md). Then it compiles that to SVG, rasterises it, and draws the result as a Kitty image. MiTeX's output is not standalone Typst. It targets MiTeX's own package runtime, so nothelix vendors that scope and evaluates each conversion against it, which lets bare Typst compile the result.

One detail shapes the whole approach. The fork paints images over real buffer cells and does not reserve phantom rows, because an earlier scheme that did reserve them broke scrolling and cursor navigation. The plugin therefore has to supply real blank lines equal to an image's height. Display math handles this as a dims-first cycle. It renders once to measure the natural height, performs one atomic whole-buffer rewrite that reserves exactly that many blank lines, and then places the image.

Both halves of that cycle are measurable rather than guessed at. The sizing pass turns a compiled equation's point dimensions into the terminal rows and columns it will occupy, and flags anything wider than the viewport instead of clipping it. These are the numbers the plugin acts on, written by `just gallery` at a 120 by 40 viewport.

```text
{% include engine/math-image-grid-120x40.txt %}
```

The reservation then lands in the buffer as real blank comment lines, one for every row the image needs. A least-squares document comes back like this. The tall stacked system is given dozens of rows, the one-line normal equations are given none at all, and that difference is exactly why the measurement has to come first.

```text
{% include engine/math-reserve-120x40.txt %}
```

The full reasoning lives in `docs/dev/math-rendering-strategy.md`.

## Where to read more

`docs/dev/` holds the working notes, including the math-rendering strategy, the fork notes, and the upstreaming roadmap. They are point-in-time design documents, not reference material.
