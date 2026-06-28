# Nothelix

Jupyter-style notebooks, inside the Helix editor.

Nothelix runs notebooks in Helix. You execute cells, watch their output land in
place, and see plots render right in the buffer — all from the modal editor you
already use, and without ever opening a browser. It is built for researchers and
scientists who would rather stay in their terminal and focus on the work than
detour into a browser tab to do the computational part of it.

A notebook here is a plain Julia file with explicit cell markers, not an opaque
`.ipynb` blob. It diffs cleanly, edits at the speed of modal motions, runs against
a live kernel that keeps its state between cells, and still converts back to
`.ipynb` when a collaborator needs it. Mathematics renders as you read it, figures
appear under the cells that produced them, and when you are finished a single
command turns the whole thing into Markdown, Typst, or a typeset PDF.

**Full documentation: [koalazub.github.io/nothelix](https://koalazub.github.io/nothelix)**

## Install

On macOS (Apple Silicon) or x86_64 Linux:

```bash
curl -sSL https://raw.githubusercontent.com/koalazub/nothelix/main/install.sh | sh
```

Then open the bundled demo:

```bash
nothelix
```

You will also need a Kitty-protocol terminal for inline plots and typeset math,
and Julia 1.9 or newer on your PATH (install [juliaup](https://julialang.org/install/)
if you do not have it). The [installation guide](https://koalazub.github.io/nothelix/install)
covers the full requirements, the `nothelix` CLI, and building from source.

## What the documentation covers

**[Notebooks.](https://koalazub.github.io/nothelix/notebooks)** The reason a
notebook is a decorated source file rather than JSON, and the whole workflow that
follows: converting and syncing `.ipynb` files, scaffolding cells by typing
`@cell` and pressing space, executing against a persistent Julia kernel, and the
quiet renumber pass that keeps cell indices tidy.

**[Rendering.](https://koalazub.github.io/nothelix/rendering)** How results get
into the buffer. Inline `$ … $` math becomes Unicode as you read it; display
`$$ … $$` blocks and Markdown tables render as properly typeset images; plots
arrive inline through the terminal's graphics protocol. Includes the graphics
matrix and the honest caveat about multiplexers stripping Kitty sequences.

**[Export.](https://koalazub.github.io/nothelix/export)** Turning a notebook into
something to hand over — Markdown, Typst, or a typeset PDF — and why the
mathematics survives the trip intact rather than being flattened, because the same
LaTeX-to-Typst machinery drives both the in-buffer rendering and the export.

**[Language server.](https://koalazub.github.io/nothelix/lsp)** Nothelix offers the
LSP interface, not a built-in server: Helix already speaks the protocol, so you
connect the Julia language server you prefer. The page covers both JETLS and
LanguageServer.jl, and the two packages either needs to make sense of a notebook.

**[Commands and keys.](https://koalazub.github.io/nothelix/commands)** The
authoritative reference — every `:command`, every keybinding, and the autofill
shortcuts — kept in sync with the plugin.

**[Architecture.](https://koalazub.github.io/nothelix/architecture)** For the
curious and for contributors: the four cooperating layers (the forked Helix, the
Steel plugin, the Rust library, and the Julia kernel), the file-based kernel
protocol, the error enrichment that rewrites a bare stack trace to name your
variables, and the math-rendering strategy. The page illustrates the system with
an interactive walkthrough.

**[Troubleshooting.](https://koalazub.github.io/nothelix/troubleshooting)** Start
with `nothelix doctor`, then debug logging and the image repro harness, plus the
breakages people actually hit and the current, plainly stated limitations.

## Status

Julia is the only supported kernel today; Python is planned. Inline rendering needs
the [Helix fork](https://github.com/koalazub/helix/tree/feature/inline-image-rendering)
and a Kitty-protocol terminal — on stock Helix or under a multiplexer it falls back
to placeholders. Prebuilt binaries ship for macOS arm64 and Linux x86_64. Where
something is unfinished, the docs say so rather than implying otherwise.

## Credits

Nothelix is a thin layer of glue over a great deal of excellent open-source work,
and would not exist without any of it.

- **[Helix](https://helix-editor.com/)** and its [Steel](https://github.com/mattwparas/steel)
  plugin system — the editor it lives in and the embedded Scheme the plugin is
  written in. A [small fork of Helix](https://github.com/koalazub/helix/tree/feature/inline-image-rendering)
  adds the inline-image rendering everything visual depends on.
- **[Julia](https://julialang.org/)** and **[juliaup](https://github.com/JuliaLang/juliaup)**
  — the kernel language and the supported way to install it.
- **[Typst](https://typst.app/)** with **[MiTeX](https://github.com/mitex-rs/mitex)**
  — the typesetting system behind math rendering and Markdown/Typst/PDF export, and
  the LaTeX-to-Typst conversion that feeds it. No LaTeX distribution required.
- **[resvg](https://github.com/RazrFalcon/resvg)** and **[tiny-skia](https://github.com/RazrFalcon/tiny-skia)**
  rasterise the typeset output; the **[image](https://github.com/image-rs/image)**
  crate, **[serde](https://serde.rs/)**, and **[rayon](https://github.com/rayon-rs/rayon)**
  do the rest of the system-shaped work in the Rust library.
- The **[Kitty graphics protocol](https://sw.kovidgoyal.net/kitty/graphics-protocol/)**
  carries images into the buffer.
- **[JETLS](https://github.com/aviatesk/JETLS)** and
  **[LanguageServer.jl](https://github.com/julia-vscode/LanguageServer.jl)** provide
  optional Julia code intelligence.
- These docs are built with **[just-the-docs](https://just-the-docs.com/)**.
- Prior art that shaped the approach: **[Marimo](https://marimo.io/)** (notebooks as
  source files), and **[euporie](https://github.com/joouha/euporie)** and SymPy's
  pretty-printer (mathematics in a terminal).

The [credits page](https://koalazub.github.io/nothelix/credits) names each project
and explains, in full, why it is used.

## Licence

MIT
