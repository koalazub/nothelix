---
title: Architecture
nav_order: 8
---

# Architecture

This page is for the curious and for contributors. None of it is needed to use
nothelix, but it explains why the system is shaped the way it is. Deeper internal
notes live under `docs/dev/` in the source tree.

## Four cooperating layers

Nothelix is four pieces, each doing the kind of work it is best at.

```
              ┌──────────────────────────────────────────┐
              │              Forked Helix                 │
              │  koalazub/helix feature/inline-image-     │
              │  rendering — adds the fork-only APIs       │
              └──────────────┬───────────────────────────┘
                             │  loads
              ┌──────────────▼───────────────────────────┐
              │        plugin/ (Steel / Scheme)           │
              │   editor commands, keymaps, rendering,     │
              │   document lifecycle hooks                 │
              └──────────────┬───────────────────────────┘
                             │  FFI (#%require-dylib)
              ┌──────────────▼───────────────────────────┐
              │     libnothelix (Rust cdylib)             │
              │   notebook parsing, kernel IPC, image      │
              │   encoding, math/typst, error enrichment   │
              └──────────────┬───────────────────────────┘
                             │  spawns + file IPC
              ┌──────────────▼───────────────────────────┐
              │        Julia kernel (one per doc)          │
              │   cell registry, AST analysis,             │
              │   output capture                           │
              └──────────────────────────────────────────┘
```

**Rust** handles everything system-shaped: notebook JSON parsing, kernel process
management, image decoding and Kitty-protocol encoding, Typst compilation, and
error enrichment. **Steel** handles everything editor-shaped: commands, keymaps,
the picker UI, overlay and image placement, and document lifecycle hooks.
**Julia** handles everything language-shaped: cell execution, dependency tracking,
and the runtime type introspection behind error hints.

The split is deliberate. Doing JSON parsing or image encoding in Steel was
noticeably slow on large notebooks, so the heavy data work lives in Rust and Steel
acts as the caller and the UI layer — which is where it shines, since you can
iterate on UX without recompiling.

## Why a fork

Stock Helix compiles and loads nothelix fine, but three capabilities live only on
the [koalazub/helix feature/inline-image-rendering](https://github.com/koalazub/helix/tree/feature/inline-image-rendering)
branch.

| Fork addition | What it does |
|---|---|
| **RawContent API** | Attaches image payloads to a document. The terminal draws them through Kitty's Unicode-placeholder protocol, so images survive scrolling and edits. |
| **Math line annotations** | Adds virtual rows above and below source lines, used to stack big-operator limits without touching the underlying text. |
| **Steel FFI surfaces** | Exposes both of the above to Scheme. |

The design follows a "dumb core, smart plugins" principle. The core grew a single
new idea — that a plugin can emit raw bytes at a document position, and those bytes
consume some number of visual rows. It knows nothing about image formats, terminal
protocols, or notebooks. The plugin provides all the policy. The whole rendering
change is a small, targeted diff across the formatter, the buffer, and the
terminal flush.

On stock Helix, image rendering and stacked-math limits quietly fall back to
placeholders. The plugin guards every fork FFI, so nothing crashes — you simply
lose those two features.

## The kernel protocol

Kernels talk over file-based JSON, one long-lived process per open `.ipynb` or
`.jl` document, running until you call `:kernel-shutdown`. A single execution flows
like this.

1. Steel asks libnothelix to start the cell. The library writes the cell code and
   index to `input.json` in the kernel's scratch directory.
2. The kernel watches that directory, reads `input.json`, and runs the code in a
   persistent module — so definitions accumulate across cells, exactly as in a
   REPL — registering dependencies, capturing output, and catching errors.
3. The kernel writes `output.json` with text output, base64 PNG images, structured
   error information, and a fresh snapshot of the cell registry, then a completion
   marker.
4. Steel polls for the result without blocking the editor, reads `output.json`,
   renders the output in place, and registers any images through the fork's
   RawContent API.

Plots travel as base64 PNGs through Julia's standard `MIME("image/png")` display
system. The kernel never thinks about Kitty; it writes pixels, and libnothelix
handles the wire format.

## Error enrichment

The kernel catches runtime errors and emits them as structured JSON. libnothelix
then runs each one through a pipeline of enrichers that fold source context,
cross-cell context, and kernel-side type hints into the message you actually see.

- **`UndefVarError`** gains a note saying where the variable is defined, or would
  be. If the kernel has not indexed it yet, a static `.jl` scanner in Rust catches
  the common case of a variable defined in a later cell you have not run.
- **`MethodError`** draws on two kernel snapshots: the recorded type of every
  binding, and the set of in-scope values a function could actually accept. The
  enricher maps the failing signature back to your variables and renders a
  "variables by type" block alongside the viable candidates.
- **`DimensionMismatch`, `BoundsError`, and `ParseError`** go through
  source-line-aware enrichers that re-express the error using your real variable
  names instead of generic placeholders.

The payoff is concrete. An error like `no method matching (Matrix)(::Vector{ComplexF64})`
does more than echo a type mismatch: it points to the variable in scope holding
that stray `Vector{ComplexF64}`, and to the in-scope value `Matrix()` would have
accepted.

## Mathematics rendering

Inline math is a single-pass Unicode scanner: it walks the text inside each
`$ … $` region and produces overlay pairs that replace LaTeX constructs with
Unicode glyphs, in one linear pass with no pathological cases.

Display math takes a different route, because it needs real typesetting. The LaTeX
is converted to Typst, compiled to SVG, rasterised, and drawn as a Kitty image.
The LaTeX-to-Typst conversion uses the [MiTeX](credits.md) project. MiTeX's output is
not standalone Typst — it targets MiTeX's own Typst package runtime — so nothelix
vendors that scope and evaluates conversions against it, which is what lets bare
Typst compile the result.

One load-bearing detail shapes the whole approach: the fork paints images over
real buffer cells and does not reserve phantom rows for them (an earlier scheme
that did broke scrolling and cursor navigation). So the plugin must supply real
blank lines equal to an image's height. For display math this means a dims-first
cycle — render to measure the natural height, then perform a single atomic
whole-buffer rewrite that reserves exactly the right number of blank lines, then
place the image. The full reasoning is in `docs/dev/math-rendering-strategy.md`.

## Where to read more

The `docs/dev/` directory in the source tree holds the working notes behind these
decisions: the math-rendering strategy, the fork notes, and the upstreaming
roadmap. They are point-in-time design documents rather than reference material,
but they record why things are the way they are.
