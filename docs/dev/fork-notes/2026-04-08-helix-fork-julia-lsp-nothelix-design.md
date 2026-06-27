# Helix Fork + Julia LSP + Nothelix Integration

**Date:** 2026-04-08
**Status:** Approved

## Goal

Replace the system-installed Helix (vanilla upstream) with the koalazub/helix fork (Steel + dylibs enabled), wire up the Julia LSP for `.jl` files, and ensure the nothelix plugin is active — all managed through Nix.

## Context

- **System Helix** is built from upstream `helix-editor/helix` via `~/nixoala/packages/helix/flake.nix`, installed system-wide in `darwin/default.nix`. No Steel support.
- **Fork** at `github:koalazub/helix` (`feature/inline-image-rendering` branch) tracks `mattwparas/helix` upstream which has Steel plugin support. Rebased periodically.
- **Nothelix** plugin at `~/projects/nothelix` provides Jupyter notebook support for Helix via Steel + a Rust cdylib FFI.
- **Julia 1.12.5** is available in Nix but not on the system PATH. `LanguageServer.jl` is not installed.
- The Julia LSP provides native backslash-to-unicode completions (dropdown before Tab), which is the missing UX piece.

## Changes

### 1. `~/nixoala/packages/helix/flake.nix` — Build fork with Steel

**What changes:**
- `helix-src` input: `github:helix-editor/helix/master` -> `github:koalazub/helix/feature/inline-image-rendering`
- Add `buildFeatures = [ "steel" ];` to the `buildRustPackage` call
- Add `cargoLock.outputHashes` for the `steel` git dependency (`github:mattwparas/steel` at rev `605d490c`). Nix requires explicit hashes for git deps in `Cargo.lock`.

**Rebase workflow:** Merge upstream into the fork branch, push, then `nix flake update helix-src` in `~/nixoala/packages/helix/` followed by a system rebuild.

### 2. `~/nixoala/config/helix/languages.toml` — Julia LSP

**What changes:** Add Julia language-server entry:

```toml
[language-server.julia]
command = "julia"
timeout = 60
args = [
  "--startup-file=no",
  "--history-file=no",
  "--quiet",
  "-e",
  "using LanguageServer; runserver()",
]

[[language]]
name = "julia"
scope = "source.julia"
file-types = ["jl"]
language-servers = ["julia"]
roots = ["Project.toml", "Manifest.toml"]
indent = { tab-width = 4, unit = "    " }
```

This matches the upstream Helix default config but makes it explicit in the user's `languages.toml` so it's managed alongside the other LSP configs.

**Why:** The Julia LSP provides backslash-tab completions natively (dropdown with symbol preview), plus go-to-definition, hover docs, diagnostics, etc. for `.jl` files.

### 3. `~/projects/nothelix/flake.nix` — LanguageServer.jl in dev shell

**What changes:** Add a `shellHook` check that installs `LanguageServer.jl` if it's not already present in the default Julia depot (`~/.julia`). This is a one-time operation that persists across shell sessions.

```bash
julia -e 'try; using LanguageServer; catch; import Pkg; Pkg.add("LanguageServer"); end'
```

**Why:** `julia-bin` is already in `buildInputs` so Julia is on PATH inside `nix develop`. The LSP package just needs to be installed once.

### What stays the same

- `~/.config/helix/config.toml` — untouched (theme, keys, editor settings)
- `~/.config/helix/init.scm` — still loads nothelix via `(require "nothelix.scm")`
- `~/.config/helix/nothelix.scm` — symlink to `~/projects/nothelix/plugin/nothelix.scm`
- `~/.steel/native/libnothelix.dylib` — still installed via `just install` in nothelix project
- Nothelix's `julia-tab-complete` — remains as fallback when LSP isn't connected

## Tab-complete UX after integration

1. Open a `.jl` file inside `nix develop` (Julia on PATH, LSP starts)
2. Type `\alp` — Julia LSP shows completion dropdown: `\alpha -> a`, `\aleph -> N`, etc.
3. Select from dropdown or finish typing + Tab
4. If LSP not connected (outside dev shell), nothelix's `julia-tab-complete` still works as before

## Risk / Notes

- **Steel git dep hash:** Will need to be computed on first build. Nix will error with the expected hash if a placeholder is used — copy it in and rebuild.
- **Julia LSP startup time:** First launch is slow (~30-60s) as Julia compiles. Subsequent launches use cached sysimages if available. The `timeout = 60` in the LSP config accounts for this.
- **Fork divergence:** The fork tracks `mattwparas/helix` (not upstream `helix-editor/helix`). Steel features may lag behind upstream Helix. Rebase carefully.
