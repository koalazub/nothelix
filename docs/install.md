---
title: Installation
nav_order: 2
---

# Installation

There are two ways in. Most people want the prebuilt installer. If you intend to
work on nothelix itself — the Helix fork, the Rust library, or the plugin — build
from source.

## The quick path

On macOS (Apple Silicon) or x86_64 Linux, one line gets you running.

```bash
curl -sSL https://raw.githubusercontent.com/koalazub/nothelix/main/install.sh | sh
```

The installer downloads a prebuilt tarball for your platform and lays it out
under `~/.local/bin` and `~/.local/share/nothelix`. That bundle includes the
Helix fork binary, the libnothelix dynamic library, the plugin sources, and a
runtime with a demo notebook. It also adds `(require "nothelix.scm")` to
`~/.config/helix/init.scm` if it is not already there.

When it finishes, open the bundled demo.

```bash
nothelix
```

`nothelix` with no arguments opens the demo; with file arguments it opens those
files. Run `nothelix --help` for the other subcommands.

| Subcommand | What it does |
|---|---|
| `nothelix <file>...` | Open notebooks in the bundled Helix fork |
| `nothelix doctor` | Run environment checks; add `--smoke` to also spawn a Julia kernel end to end |
| `nothelix config [show\|edit\|path]` | Inspect or edit the Helix config |
| `nothelix reset [--lsp\|--kernel\|--all]` | Clear runtime state (caches, kernel env, LSP) |
| `nothelix upgrade` | Re-run the installer in place |
| `nothelix uninstall` | Remove every file the installer placed |
| `nothelix version` | Print version and Julia information |

## Requirements

- **macOS arm64 or Linux x86_64.** Other targets are not shipped yet.
- **A Kitty-protocol terminal** for inline plots and typeset math. Kitty itself
  is the reference implementation; other terminals that implement the protocol
  also work. Run `:graphics-check` inside Helix to confirm what was detected.
- **Julia 1.9 or newer on your PATH.** If you do not have it, install
  [juliaup](https://julialang.org/install/) first — it manages Julia versions
  cleanly and is the supported way to get a kernel.

Julia code intelligence (hover, completion, go-to-definition) is a separate,
optional step. See [Language server](lsp.md).

## When something breaks

```bash
nothelix doctor
```

This runs a battery of environment checks and tells you what is wrong. Add
`--smoke` to also spawn a Julia kernel and exercise the full execution pipeline
end to end. The [troubleshooting](troubleshooting.md) page covers what to do with
what it reports.

## Uninstalling

```bash
nothelix uninstall
```

This removes every file the installer placed and leaves `~/.julia/`, your Helix
config, and your own notebooks untouched. Add `--purge` to also clear
`~/.cache/helix/helix.log`.

## Building from source

To iterate on the fork, the Rust library, or the plugin, skip the installer and
build by hand. You need four things:

- The [koalazub/helix fork](https://github.com/koalazub/helix/tree/feature/inline-image-rendering),
  which adds the RawContent API that inline rendering depends on. Stock Helix
  compiles and loads nothelix fine, but inline images and stacked-math limits
  fall back to placeholders without it. See [Architecture](architecture.md#why-a-fork).
- A Rust nightly toolchain, to build Helix and libnothelix.
- Julia, currently the only supported kernel.
- A terminal that speaks the Kitty graphics protocol.

A word on multiplexers. Zellij and tmux intercept escape sequences and strip the
ones Kitty needs, so anything image-based stops appearing — plots, typeset
display math, and rendered tables. Inline Unicode math and concealed symbols
still show, since those are plain text. Run Helix directly in a Kitty-protocol
terminal, or use a multiplexer that forwards the Kitty protocol untouched.

### 1. Build the Helix fork

Build Helix and libnothelix against the same Steel commit so their FFI ABIs
match. The fork pins that commit; libnothelix pins it to match.

```bash
git clone https://github.com/koalazub/helix.git
cd helix

# Grammar builds are slow, so disable the automatic one and run it after.
HELIX_DISABLE_AUTO_GRAMMAR_BUILD=1 cargo build --release --features steel
./target/release/hx --grammar fetch
./target/release/hx --grammar build
```

Put the `hx` binary on your PATH, and point `HELIX_RUNTIME` at the fork's runtime
directory when running from a non-system install.

```bash
ln -sf "$(pwd)/target/release/hx" ~/.local/bin/hx
export HELIX_RUNTIME="/path/to/helix/runtime"
```

### 2. Set STEEL_HOME

Steel keeps its native libraries in a home directory. Add this to your shell
profile so it is set every time Helix launches.

```bash
export STEEL_HOME="$HOME/.steel"
```

### 3. Install nothelix

```bash
git clone https://github.com/koalazub/nothelix.git
cd nothelix
just install
```

That is the whole install. Under the hood, `just install` builds libnothelix in
release mode, copies and codesigns the dynamic library into `~/.steel/native/`,
and symlinks the plugin sources into `~/.steel/cogs/`. Everything but the LSP
setup lives under `$STEEL_HOME`, which defaults to `~/.steel`. Steel's resolver
already searches `$STEEL_HOME/cogs`, so a plain `(require "nothelix.scm")` in your
`init.scm` loads the plugin with no extra wiring.

No `just`? Install it with `cargo install just` or your package manager, or read
the justfile and run the steps by hand.

> **Why `~/.steel/cogs` and not `~/.config/helix/`?** Earlier versions installed
> under the Helix config directory, which broke whenever a config manager like
> home-manager, stow, or chezmoi owned that tree. `$STEEL_HOME/cogs` is Steel's
> own territory; no config manager touches it, and Steel finds modules there
> without the Helix config directory being in its resolver path at all.

> **macOS note.** The dynamic library has to be re-codesigned after every
> rebuild — macOS invalidates the signature when the file changes and kills
> whatever loads it. Do not symlink the library, because codesign stamps the real
> file and a rebuild leaves a symlink's target stale. `just install` does the
> copy-then-sign for you.

### 4. Load the plugin

Add this line to `~/.config/helix/init.scm`, creating the file if it does not
exist.

```scheme
(require "nothelix.scm")
```

If you previously installed nothelix under `~/.config/helix/nothelix*`, delete
those files first. Helix searches the config directory before `$STEEL_HOME/cogs`,
so a stale copy there silently shadows the fresh install. `just install` warns
you when it spots one.

### Development recipes

| Recipe | Description |
|---|---|
| `just install` | Build, install, and codesign the library, and link the plugin |
| `just install debug` | The same, with the debug profile |
| `just build` | Build without installing |
| `just test` | Run the libnothelix tests with nextest |
| `just check` | Lints, tests, and a real plugin-load check — the pre-commit gate |
| `just setup-lsp` | Install the Julia packages JETLS needs (see [Language server](lsp.md)) |
| `just uninstall` | Remove the installed library and plugin links |

Run `just install` and restart Helix after any Rust change. Helix caches the
plugin modules and the loaded library, so a full restart — not a config reload —
is what picks up edits.
