---
title: Contributing
nav_order: 10
---

# Contributing

To iterate on the fork, the Rust library, or the plugin, build by hand instead
of using the installer. You need four things:

| Dependency | Why |
|---|---|
| The [koalazub/helix fork](https://github.com/koalazub/helix/tree/feature/inline-image-rendering) | Adds the RawContent API inline rendering depends on. Stock Helix loads nothelix, but images and stacked math fall back to placeholders. See [Architecture](architecture.md#why-a-fork). |
| A Rust nightly toolchain | Builds Helix and libnothelix. |
| Julia | The only supported kernel. |
| A Kitty-protocol terminal | Inline plots, display math, and tables. |

Multiplexers (tmux, Zellij) strip the escape sequences Kitty needs, so
image-based output stops appearing. Run Helix directly, or use a multiplexer
that forwards the Kitty protocol untouched.

## 1. Build the Helix fork

Build Helix and libnothelix against the same Steel commit so their FFI ABIs
match. The fork pins that commit; libnothelix pins it to match.

```bash
git clone https://github.com/koalazub/helix.git
cd helix

# Grammar builds are slow; disable the automatic one and run it after.
HELIX_DISABLE_AUTO_GRAMMAR_BUILD=1 cargo build --release --features steel
./target/release/hx --grammar fetch
./target/release/hx --grammar build
```

Put `hx` on your PATH, and point `HELIX_RUNTIME` at the fork's runtime directory.

```bash
ln -sf "$(pwd)/target/release/hx" ~/.local/bin/hx
export HELIX_RUNTIME="/path/to/helix/runtime"
```

## 2. Set STEEL_HOME

Add this to your shell profile so it is set every time Helix launches.

```bash
export STEEL_HOME="$HOME/.steel"
```

## 3. Install nothelix

```bash
git clone https://github.com/koalazub/nothelix.git
cd nothelix
just install
```

`just install` builds libnothelix in release mode, copies and codesigns the
dynamic library into `~/.steel/native/`, and symlinks the plugin sources into
`~/.steel/cogs/`. Steel's resolver searches `$STEEL_HOME/cogs`, so a plain
`(require "nothelix.scm")` loads the plugin.

No `just`? Install it with `cargo install just`, or read the justfile and run the
steps by hand.

The plugin installs under `$STEEL_HOME`, not the Helix config directory, so
config managers (home-manager, stow, chezmoi) never touch it.

macOS note: the dynamic library is re-codesigned after every rebuild. Do not
symlink it; codesign stamps the real file and a rebuild leaves a symlink stale.
`just install` does the copy-then-sign.

## 4. Load the plugin

Add this to `~/.config/helix/init.scm`, creating the file if needed.

```scheme
(require "nothelix.scm")
```

If you previously installed under `~/.config/helix/nothelix*`, delete those
files. Helix searches the config directory first, so a stale copy shadows the
fresh install. `just install` warns you when it spots one.

## Development recipes

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
picks up edits.

Contributors can drop into a pinned dev shell (fork toolchain, Julia,
tree-sitter) with `nix develop`. The flake pins its toolchain inputs, so a
`nix flake update` will not silently move the build.
