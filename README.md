# Nothelix

Jupyter-style notebooks, inside the Helix editor.

Nothelix runs notebooks in Helix. You execute cells, watch their output land in
place, and see plots render right in the buffer — all from the modal editor you
already use, and without ever opening a browser. It is built for researchers and
scientists who would rather stay in their terminal and focus on the work than
switch to a browser tab to do the computational part of it.

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
if you do not have it). Full requirements and the build-from-source path are in the
[installation guide](https://koalazub.github.io/nothelix/install).

## Documentation

| Page | What it covers |
|---|---|
| [Overview](https://koalazub.github.io/nothelix/) | What nothelix is and who it is for |
| [Installation](https://koalazub.github.io/nothelix/install) | Quick install, requirements, building from source |
| [Notebooks](https://koalazub.github.io/nothelix/notebooks) | The cell format, execution, scaffolding |
| [Rendering](https://koalazub.github.io/nothelix/rendering) | Math, plots, tables, graphics protocols |
| [Export](https://koalazub.github.io/nothelix/export) | Markdown, Typst, and PDF export |
| [Language server](https://koalazub.github.io/nothelix/lsp) | Wiring up JETLS for Julia code intelligence |
| [Commands and keys](https://koalazub.github.io/nothelix/commands) | The full command and keybinding reference |
| [Architecture](https://koalazub.github.io/nothelix/architecture) | How the four layers fit together |
| [Credits](https://koalazub.github.io/nothelix/credits) | The open-source work nothelix is built on |
| [Troubleshooting](https://koalazub.github.io/nothelix/troubleshooting) | Diagnostics, debug logging, common breakages |

## Built on

Helix, [Steel](https://github.com/mattwparas/steel), [Julia](https://julialang.org/),
and [Typst](https://typst.app/), among many others. The
[credits](https://koalazub.github.io/nothelix/credits) page names each project and
explains why it is used.

Contributor and design notes live under [`docs/dev/`](docs/dev/).

## Licence

MIT
