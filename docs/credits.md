---
title: Credits
nav_order: 11
---

# Credits

Nothelix is built on open-source projects. Every dependency, its role, and why it's here.

## Editor and plugin system

| Project | Role | Why |
|---|---|---|
| [Helix](https://helix-editor.com/) | Host editor | Nothelix lives inside it; you never leave |
| [Steel](https://github.com/mattwparas/steel) + [plugin system](https://github.com/helix-editor/helix/pull/8675) | Embedded Scheme | Commands, keymaps, pickers, and hooks without recompiling the editor |
| [Inline-image fork](https://github.com/koalazub/helix/tree/feature/inline-image-rendering) | Buffer image rendering | Plots appear where you work. See [Architecture](architecture.md#why-a-fork) |

## Language runtime

| Project | Role | Why |
|---|---|---|
| [Julia](https://julialang.org/) | Kernel language | Cells run against a real process; state persists like a REPL |
| [juliaup](https://github.com/JuliaLang/juliaup) | Julia installer | Keeps kernel setup predictable |
| [JETLS](https://github.com/aviatesk/JETLS) | Language server | Optional code intelligence; see [Language server](lsp.md) |
| [Runic](https://github.com/fredrikekre/Runic.jl) | Formatter | Format-on-request, delegated by JETLS |

## Typesetting and graphics

| Project | Role | Why |
|---|---|---|
| [Typst](https://typst.app/) | Typesetting | Display math, tables, and PDF/Typst export with no LaTeX install |
| [MiTeX](https://github.com/mitex-rs/mitex) | LaTeX to Typst | Write LaTeX in a notebook, get correct Typst out. See [Architecture](architecture.md#mathematics-rendering) |
| [resvg](https://github.com/RazrFalcon/resvg) + [tiny-skia](https://github.com/RazrFalcon/tiny-skia) | SVG rasteriser | Turns Typst SVG into pixels |
| [Kitty graphics protocol](https://sw.kovidgoyal.net/kitty/graphics-protocol/) | Terminal image transport | Carries images into the buffer |

## Rust libraries

| Project | Role | Why |
|---|---|---|
| [image](https://github.com/image-rs/image) | Decode and convert | Image format handling |
| [serde](https://serde.rs/) | JSON | The wire format between every layer |
| [rayon](https://github.com/rayon-rs/rayon) | Parallelism | Batch math rendering |
| [base64](https://github.com/marshallpierce/rust-base64) | Encoding | Image wire format |
| [tree-sitter](https://tree-sitter.github.io/tree-sitter/) | Grammars | Build-time parsing |

Plus smaller crates for static maps, signals, and temp dirs. The build uses a Rust nightly toolchain.

## Prior art

| Project | What it taught |
|---|---|
| [Marimo](https://marimo.io/) | A notebook can be a decorated source file, not a JSON blob |
| [euporie](https://github.com/joouha/euporie), SymPy pretty-printer, termtex | What math looks like in a terminal; shaped the tiered rendering strategy |

## This site

Built with [just-the-docs](https://just-the-docs.com/), served by GitHub Pages.

---

Nothelix is released under the MIT licence. Each project above is governed by its own licence; follow the links.
