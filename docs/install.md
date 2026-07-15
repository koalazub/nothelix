---
title: Installation
nav_order: 2
---

# Installation

One command installs everything.

```bash
curl -sSL https://raw.githubusercontent.com/koalazub/nothelix/main/install.sh | sh
```

The installer supports macOS on Apple Silicon and Linux on x86_64. It downloads a
prebuilt tarball and lays it out under `~/.local/bin` and
`~/.local/share/nothelix`, then adds `(require "nothelix.scm")` to
`~/.config/helix/init.scm` so Helix loads the plugin on startup. The bundle
carries everything nothelix needs, namely the Helix fork, libnothelix, the
plugin sources, and a demo notebook.

Once it finishes, open the demo notebook.

```bash
nothelix
```

## Requirements

| Requirement | Detail |
|---|---|
| Platform | macOS arm64 or Linux x86_64. Other targets are not shipped yet. |
| Terminal | A Kitty-protocol terminal for inline plots and math. Run `:graphics-check` in Helix to confirm. |
| Julia | 1.9 or newer on your PATH. Install via [juliaup](https://julialang.org/install/). |

Nothelix runs whatever `julia` your PATH resolves. It does not vendor a copy and
it does not create per-notebook environments. The first cell run adds the
kernel's dependencies to your default environment.

Julia code intelligence such as hover, completion, and go-to-definition is
optional. Run `nothelix setup-lsp`, then follow [Language server](lsp.md) to wire
it up.

## Subcommands

| Subcommand | What it does |
|---|---|
| `nothelix <file>...` | Open notebooks in the bundled Helix fork |
| `nothelix new [path]` | Create a new `.jl` notebook from a template and open it |
| `nothelix doctor` | Run environment checks. Add `--smoke` to spawn a Julia kernel end to end |
| `nothelix setup-lsp` | Install the Julia packages the [language server](lsp.md) needs |
| `nothelix config [show\|edit\|path]` | Inspect or edit the effective config |
| `nothelix reset [--lsp\|--kernel\|--all]` | Clear runtime state such as caches, kernel env, and LSP |
| `nothelix upgrade` | Re-run the installer to upgrade in place |
| `nothelix uninstall` | Remove every file the installer placed |
| `nothelix version` | Print version and Julia information |

Uninstalling is surgical. `nothelix uninstall` leaves `~/.julia/`, your Helix
config, and your notebooks untouched, and it removes only the nothelix require
line from `init.scm`. Add `--purge` to also clear `~/.cache/helix/helix.log`.

## Install with Nix

The repository is a flake, so you can build the pieces directly instead of
fetching a tarball.

```bash
nix build github:koalazub/nothelix#hx-nothelix   # the forked Helix binary
nix build github:koalazub/nothelix#libnothelix   # the Rust FFI library
```

The default package builds the same release bundle the installer ships. Build it,
then run its local installer.

```bash
nix build github:koalazub/nothelix
sh result/nothelix-*/install-local.sh
```

## When something breaks

Start with the doctor. It runs environment checks and reports what is wrong.

```bash
nothelix doctor
```

Add `--smoke` to exercise the full execution pipeline, which spawns a Julia
kernel end to end. If the checks do not point you at the fix, see
[Troubleshooting](troubleshooting.md).

<!-- SCREENSHOT NEEDED: a `nothelix doctor` run showing the environment-check output -->

You now have nothelix running with the demo notebook open. Read
[Notebooks](notebooks.md) next to learn the core loop of writing cells, running
them, and moving around. To work on nothelix itself, see
[Contributing](contributing.md).
