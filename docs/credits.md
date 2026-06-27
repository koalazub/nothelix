---
title: Credits
nav_order: 9
---

# Credits

Nothelix is a thin layer of glue over a great deal of excellent open-source work.
This page names what it depends on and, more importantly, why. The project would
not exist without any of these.

## The editor and its plugin system

- **[Helix](https://helix-editor.com/)** — the modal editor nothelix lives
  inside. The whole premise is that you should not have to leave it, which means
  Helix is the foundation everything else rests on.
- **The [Helix plugin system](https://github.com/helix-editor/helix/pull/8675)
  and [Steel](https://github.com/mattwparas/steel)** — Steel is the embedded
  Scheme that the plugin is written in. It is what makes commands, keymaps,
  pickers, and document hooks possible without recompiling the editor, and it is
  where the entire user-facing layer of nothelix lives.
- **The [inline-image fork of Helix](https://github.com/koalazub/helix/tree/feature/inline-image-rendering)**
  — a small set of additions to Helix itself for rendering non-text content in the
  buffer. The fork is the difference between plots that appear where you are
  working and plots you have to open elsewhere. Why it is needed, and what it adds,
  is covered in [Architecture](architecture.md#why-a-fork).

## The language runtime

- **[Julia](https://julialang.org/)** — the kernel language, and the only one
  supported today. Cells execute against a real Julia process, which is why state
  persists across them the way it does in a REPL.
- **[juliaup](https://github.com/JuliaLang/juliaup)** — the recommended way to
  install and manage Julia. Pointing people at it keeps the kernel setup
  predictable.
- **[JETLS](https://github.com/aviatesk/JETLS)** — the recommended Julia language
  server for code intelligence, built on the JET analysis framework. It is
  optional and bring-it-yourself; see [Language server](lsp.md).
- **[Runic](https://github.com/fredrikekre/Runic.jl)** — the Julia formatter JETLS
  delegates to, for anyone who wants format-on-request.

## Typesetting and graphics

- **[Typst](https://typst.app/)** — the typesetting system behind display-math
  rendering, table rendering, and Typst and PDF export. Choosing Typst over a full
  LaTeX toolchain means math typesetting works out of the box, with no external
  distribution to install. Nothelix uses its compilation, layout, SVG, PDF, and
  font-kit crates.
- **[MiTeX](https://github.com/mitex-rs/mitex)** — converts LaTeX math into Typst.
  It is what lets you write LaTeX in a notebook and get correct Typst in an export
  or a rendered equation. Its Typst scope is vendored so the conversions compile
  standalone, as explained in [Architecture](architecture.md#mathematics-rendering).
- **[resvg](https://github.com/RazrFalcon/resvg) and
  [tiny-skia](https://github.com/RazrFalcon/tiny-skia)** — rasterise the SVG that
  Typst produces into the pixels the terminal draws.
- **The [Kitty graphics protocol](https://sw.kovidgoyal.net/kitty/graphics-protocol/)**
  — the terminal protocol that carries images into the buffer. The fork's
  inline-image rendering is built around it.

## Rust libraries

The dynamic library leans on the wider Rust ecosystem for the system-shaped work:
**[image](https://github.com/image-rs/image)** for decoding and format conversion,
**[serde](https://serde.rs/)** for the JSON that flows between every layer,
**[rayon](https://github.com/rayon-rs/rayon)** for parallel batch math rendering,
**[base64](https://github.com/marshallpierce/rust-base64)** for the image wire
format, and a handful of smaller crates for static maps, signals, and temporary
directories. The build itself uses a Rust nightly toolchain and
**[tree-sitter](https://tree-sitter.github.io/tree-sitter/)** for grammars.

## Prior art

The approach to notebooks and to terminal math did not come from nowhere.

- **[Marimo](https://marimo.io/)** showed that a notebook can be a decorated
  source file rather than an opaque JSON blob — the idea the whole text-first cell
  format is built on.
- **[euporie](https://github.com/joouha/euporie)**, **SymPy's pretty-printer**,
  and projects like **termtex** mapped out what mathematics can look like in a
  terminal, and shaped the tiered rendering strategy.

## This site

These pages are built with **[just-the-docs](https://just-the-docs.com/)**, served
by GitHub Pages.

## First-party

- **NothelixMacros** — the small Julia package that defines the `@cell` and
  `@markdown` marker macros, so a converted notebook is valid Julia and so the
  language server can analyse it. It ships in the nothelix source tree.

---

Nothelix itself is released under the MIT licence. Each project above is governed
by its own licence; follow the links for details.
