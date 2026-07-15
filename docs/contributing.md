---
title: Contributing
nav_order: 10
---

# Contributing

To iterate on the fork, the Rust library, or the plugin, build by hand instead
of using the installer. Four things need to be in place.

| Dependency | Why |
|---|---|
| The [koalazub/helix fork](https://github.com/koalazub/helix/tree/feature/inline-image-rendering) | Adds the RawContent API inline rendering depends on. Stock Helix loads nothelix, but images and stacked math fall back to placeholders. See [Architecture](architecture.md#why-a-fork). |
| A Rust nightly toolchain | Builds Helix and libnothelix. |
| Julia | The only supported kernel. |
| A Kitty-protocol terminal | Inline plots, display math, and tables. |

Multiplexers like tmux and Zellij strip the escape sequences Kitty needs, so
image-based output stops appearing. Run Helix directly, or use a multiplexer
that forwards the Kitty protocol untouched.

## 1. Build the Helix fork

Build Helix and libnothelix against the same Steel commit so their FFI ABIs
match. The fork pins that commit, and libnothelix pins it to match.

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
`(require "nothelix.scm")` loads the plugin. When `swiftc` is present, the recipe
also compiles the optional on-device summary helper. It skips that step silently
when `swiftc` is missing, so summaries simply stay off.

No `just`? Install it with `cargo install just`, or read the justfile and run the
steps by hand.

The plugin installs under `$STEEL_HOME`, not the Helix config directory, so
config managers like home-manager, stow, and chezmoi never touch it.

On macOS, the dynamic library is re-codesigned after every rebuild. Do not
symlink it, because codesign stamps the real file and a rebuild leaves a symlink
stale. `just install` does the copy-then-sign in the right order.

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
| `just build` | Build the library without installing |
| `just build-wasm` | Build the Playground WebAssembly bundle into `docs/assets/eng/wasm` |
| `just test` | Run the libnothelix tests with nextest |
| `just check` | The pre-commit gate. Runs clippy, nextest, and a real plugin-load check in a live `hx` |
| `just setup-lsp` | Ensure the default Julia env has the kernel's JSON3 dependency (see [Language server](lsp.md)) |
| `just uninstall` | Remove the installed library and plugin links |

Run `just install` and restart Helix after any Rust change. Helix caches the
plugin modules and the loaded library, so a full restart picks up edits where a
config reload does not.

Contributors can drop into a pinned dev shell with the fork toolchain, Julia,
and tree-sitter through `nix develop`. The flake pins its toolchain inputs, so a
`nix flake update` will not silently move the build.
