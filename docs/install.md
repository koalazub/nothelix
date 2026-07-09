---
title: Installation
nav_order: 2
---

# Installation

```bash
curl -sSL https://raw.githubusercontent.com/koalazub/nothelix/main/install.sh | sh
```

macOS (Apple Silicon) or x86_64 Linux. The installer downloads a prebuilt
tarball, lays it out under `~/.local/bin` and `~/.local/share/nothelix`, and adds
`(require "nothelix.scm")` to `~/.config/helix/init.scm`. The bundle includes the
Helix fork, libnothelix, the plugin sources, and a demo notebook.

Then open the demo:

```bash
nothelix
```

## Requirements

| Requirement | Detail |
|---|---|
| Platform | macOS arm64 or Linux x86_64. Other targets are not shipped yet. |
| Terminal | A Kitty-protocol terminal for inline plots and math. Run `:graphics-check` in Helix to confirm. |
| Julia | 1.9 or newer on your PATH. Install via [juliaup](https://julialang.org/install/). |

Nothelix uses whatever `julia` your PATH resolves. It does not vendor a copy or
create per-notebook environments. The first cell run adds the kernel's
dependencies to your default environment.

Julia code intelligence (hover, completion, go-to-definition) is optional. Run
`nothelix setup-lsp`, then see [Language server](lsp.md).

## Subcommands

| Subcommand | What it does |
|---|---|
| `nothelix <file>...` | Open notebooks in the bundled Helix fork |
| `nothelix new [path]` | Create a new `.jl` notebook from a template and open it |
| `nothelix doctor` | Run environment checks; add `--smoke` to spawn a Julia kernel end to end |
| `nothelix setup-lsp` | Install the Julia packages the [language server](lsp.md) needs |
| `nothelix config [show\|edit\|path]` | Inspect or edit the Helix config |
| `nothelix reset [--lsp\|--kernel\|--all]` | Clear runtime state (caches, kernel env, LSP) |
| `nothelix upgrade` | Re-run the installer in place |
| `nothelix uninstall` | Remove every file the installer placed |
| `nothelix version` | Print version and Julia information |

`nothelix uninstall` leaves `~/.julia/`, your Helix config, and your notebooks
untouched. Add `--purge` to also clear `~/.cache/helix/helix.log`.

## Install with Nix

The repository is a flake. Build the pieces directly instead of fetching a
tarball.

```bash
nix build github:koalazub/nothelix#hx-nothelix   # the forked Helix binary
nix build github:koalazub/nothelix#libnothelix   # the Rust FFI library
```

The default package builds the same release bundle the installer ships:

```bash
nix build github:koalazub/nothelix
sh result/nothelix-*/install-local.sh
```

## When something breaks

```bash
nothelix doctor
```

Runs environment checks and reports what is wrong. Add `--smoke` to exercise the
full execution pipeline. See [Troubleshooting](troubleshooting.md).

To work on nothelix itself, see [Contributing](contributing.md).
