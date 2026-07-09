---
title: Architecture
nav_order: 9
---

# Architecture

For contributors. None of this is needed to use nothelix. Deeper notes live under `docs/dev/` in the source tree.

## Four cooperating layers

The walkthrough follows one cell — `plot(x, y)` — from keystroke to inline figure. Step through with the arrows, or let it play.

{% include eng/styles.html %}
{% include eng/arch.html %}

Each layer does the work it is best at.

| Layer | Handles | Examples |
|---|---|---|
| **Rust** (libnothelix) | Everything system-shaped | Notebook JSON parsing, kernel process management, image decode + Kitty encode, Typst compilation, error enrichment |
| **Steel** | Everything editor-shaped | Commands, keymaps, the picker UI, overlay + image placement, document lifecycle hooks |
| **Julia** (kernel) | Everything language-shaped | Cell execution, dependency tracking, runtime type introspection for hints |

The split is deliberate. JSON parsing and image encoding were slow in Steel on large notebooks. Heavy data work lives in Rust. Steel is the caller and UI layer, where you can iterate on UX without recompiling.

## Why a fork

Stock Helix compiles and loads nothelix. Three capabilities live only on the [koalazub/helix `feature/inline-image-rendering`](https://github.com/koalazub/helix/tree/feature/inline-image-rendering) branch.

| Fork addition | What it does |
|---|---|
| **RawContent API** | Attaches image payloads to a document. The terminal draws them through Kitty's Unicode-placeholder protocol, so images survive scrolling and edits. |
| **Math line annotations** | Adds virtual rows above and below source lines, used to stack big-operator limits without touching the text. |
| **Steel FFI surfaces** | Exposes both of the above to Scheme. |

The design is "dumb core, smart plugins". The core grew one new idea: a plugin can emit raw bytes at a document position, and those bytes consume some visual rows. It knows nothing about image formats, terminal protocols, or notebooks. The plugin provides all the policy. The rendering change is a small diff across the formatter, the buffer, and the terminal flush.

On stock Helix, image rendering and stacked-math limits fall back to placeholders. The plugin guards every fork FFI, so nothing crashes. You lose those two features.

## The kernel protocol

Kernels talk over file-based JSON. One long-lived process per open `.ipynb` or `.jl` document, running until `:kernel-shutdown`. One execution flows like this.

1. Steel asks libnothelix to start the cell. The library writes cell code and index to `input.json` in the kernel's scratch directory.
2. The kernel reads `input.json` and runs the code in a persistent module. Definitions accumulate across cells, like a REPL. It registers dependencies, captures output, and catches errors.
3. The kernel writes `output.json`: text output, base64 PNG images, structured errors, and a fresh cell-registry snapshot. Then a completion marker.
4. Steel polls without blocking the editor, reads `output.json`, renders output in place, and registers images through the RawContent API.

Plots travel as base64 PNGs through Julia's standard `MIME("image/png")` display system. The kernel writes pixels; libnothelix handles the wire format.

## Error enrichment

The kernel emits runtime errors as structured JSON. libnothelix folds source context, cross-cell context, and kernel-side type hints into the message you see. Toggle the views below.

{% include eng/enrich.html %}

| Error | Enrichment |
|---|---|
| `UndefVarError` | Adds a note on where the variable is defined, or would be. If the kernel has not indexed it, a static `.jl` scanner in Rust catches the common case of a variable defined in a later, unrun cell. |
| `MethodError` | Draws on two kernel snapshots: the recorded type of every binding, and the in-scope values a function could accept. Renders a "variables by type" block alongside viable candidates. |
| `DimensionMismatch`, `BoundsError`, `ParseError` | Source-line-aware enrichers re-express the error using your real variable names, not generic placeholders. |

Example: `no method matching (Matrix)(::Vector{ComplexF64})` points to the variable in scope holding that stray `Vector{ComplexF64}`, and to the in-scope value `Matrix()` would have accepted.

## Mathematics rendering

Inline math is a single-pass Unicode scanner. It walks the text inside each `$ … $` region and produces overlay pairs that swap LaTeX constructs for Unicode glyphs. One linear pass, no pathological cases.

Display math needs real typesetting.

1. Convert LaTeX to Typst via [MiTeX](credits.md).
2. Compile to SVG, rasterise, draw as a Kitty image.

MiTeX's output is not standalone Typst. It targets MiTeX's own package runtime, so nothelix vendors that scope and evaluates conversions against it. That lets bare Typst compile the result.

One detail shapes the approach: the fork paints images over real buffer cells and does not reserve phantom rows (an earlier scheme that did broke scrolling and cursor navigation). The plugin must supply real blank lines equal to an image's height. Display math is a dims-first cycle:

1. Render to measure natural height.
2. Perform one atomic whole-buffer rewrite reserving exactly that many blank lines.
3. Place the image.

Full reasoning: `docs/dev/math-rendering-strategy.md`.

## Where to read more

`docs/dev/` holds the working notes: the math-rendering strategy, the fork notes, and the upstreaming roadmap. They are point-in-time design documents, not reference material.
