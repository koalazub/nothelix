---
title: Credits
nav_order: 11
---

# Credits

Nothelix stands on open-source work. Every dependency below earns its place, and each table names the project, the job it does, and the reason it is here.

## Editor and plugin system

| Project | Role | Why |
|---|---|---|
| [Helix](https://helix-editor.com/) | Host editor | Nothelix lives inside it, so you never leave the editor |
| [Steel](https://github.com/mattwparas/steel) + [plugin system](https://github.com/helix-editor/helix/pull/8675) | Embedded Scheme | Commands, keymaps, pickers, and hooks without recompiling the editor |
| [Inline-image fork](https://github.com/koalazub/helix/tree/feature/inline-image-rendering) | Buffer image rendering | Plots appear where you work. See [Architecture](architecture.md#why-a-fork) |

## Language runtime

| Project | Role | Why |
|---|---|---|
| [Julia](https://julialang.org/) | Kernel language | Cells run against a real process, so state persists like a REPL |
| [juliaup](https://github.com/JuliaLang/juliaup) | Julia installer | Keeps kernel setup predictable |
| [JETLS](https://github.com/aviatesk/JETLS) | Language server | Optional code intelligence. See [Language server](lsp.md) |
| [Runic](https://github.com/fredrikekre/Runic.jl) | Formatter | Format-on-request, delegated by JETLS |

## Typesetting and graphics

| Project | Role | Why |
|---|---|---|
| [Typst](https://typst.app/) | Typesetting | Display math, tables, and PDF or Typst export with no LaTeX install |
| [MiTeX](https://github.com/mitex-rs/mitex) | LaTeX to Typst | Write LaTeX in a notebook and get correct Typst out. See [Architecture](architecture.md#mathematics-rendering) |
| [resvg](https://github.com/RazrFalcon/resvg) + [tiny-skia](https://github.com/RazrFalcon/tiny-skia) | SVG rasteriser | Turns Typst SVG into pixels |
| [Kitty graphics protocol](https://sw.kovidgoyal.net/kitty/graphics-protocol/) | Terminal image transport | Carries images into the buffer |

## Rust libraries

| Project | Role | Why |
|---|---|---|
| [image](https://github.com/image-rs/image) | Decode and convert | Image format handling |
| [serde](https://serde.rs/) | Serialization | The wire format between every layer |
| [comrak](https://github.com/kivikakk/comrak) | Markdown parser | Reads markdown cells and prose blocks |
| [rayon](https://github.com/rayon-rs/rayon) | Parallelism | Batch math rendering |
| [base64](https://github.com/marshallpierce/rust-base64) | Encoding | Image wire format |

Smaller crates round out the build. `phf` provides the static lookup maps, `nix` handles process signals, and `tempfile` manages scratch directories. The build uses a Rust nightly toolchain.

## Prior art

| Project | What it taught |
|---|---|
| [Marimo](https://marimo.io/) | A notebook can be a decorated source file, not a JSON blob |
| [euporie](https://github.com/joouha/euporie), SymPy pretty-printer, termtex | What math looks like in a terminal, which shaped the tiered rendering strategy |

## This site

Built with [just-the-docs](https://just-the-docs.com/), served by GitHub Pages.

---

Nothelix is released under the MIT licence. Each project above is governed by its own licence, so follow the links.
