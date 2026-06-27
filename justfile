# nothelix task runner
#
# `just install` installs the parts that change as you iterate:
#
#   ~/.steel/native/libnothelix.{dylib,so}  — FFI dylib, codesigned on macOS
#   ~/.steel/cogs/nothelix.scm              — plugin entry symlink
#   ~/.steel/cogs/nothelix/                 — plugin module directory symlink
#
# `just setup-lsp` is the one-time Julia LSP bootstrap (wrapper + env):
#
#   ~/.local/bin/julia-lsp                  — Julia LSP wrapper script
#
# Why ~/.steel/cogs? Steel's module resolver searches it automatically, so
# `(require "nothelix.scm")` in init.scm finds the plugin without any config
# wiring. This also sidesteps a class of breakage where config managers
# (home-manager, stow, chezmoi, dotfiles scripts) try to install per-file
# symlinks into ~/.config/helix/nothelix/ and walk through a whole-tree
# symlink into the project source, ultimately creating circular store ↔
# project links that loop back on themselves. STEEL_HOME is Steel's own
# territory and no sensible config manager touches it.
#
# macOS note: the kernel invalidates a dylib's code signature when the
# underlying file changes and SIGKILLs the process next time it pages in
# code — but only when Helix actually calls into the dylib, not during
# `hx --version`. Symlinks don't help because codesign stamps the real
# file. The install recipe does rm + cp + codesign so you don't have to
# remember the sequence.

set shell := ["sh", "-euc"]

steel_native := env("HOME") / ".steel" / "native"
steel_cogs := env("HOME") / ".steel" / "cogs"
local_bin := env("HOME") / ".local" / "bin"
nothelix_root := justfile_directory()

# build + install the dylib and plugin symlinks (run after Rust changes)
install profile="release":
    #!/usr/bin/env sh
    set -eu
    if [ "{{ profile }}" = "debug" ]; then
        echo "Building libnothelix (debug)..."
        cargo build -p libnothelix
    else
        echo "Building libnothelix (release)..."
        cargo build --release -p libnothelix
    fi

    # ── Dylib ─────────────────────────────────────────────────────────────
    mkdir -p "{{ steel_native }}"

    if [ -f "target/{{ profile }}/libnothelix.dylib" ]; then
        DYLIB="target/{{ profile }}/libnothelix.dylib"
        DEST="{{ steel_native }}/libnothelix.dylib"
    elif [ -f "target/{{ profile }}/libnothelix.so" ]; then
        DYLIB="target/{{ profile }}/libnothelix.so"
        DEST="{{ steel_native }}/libnothelix.so"
    else
        echo "error: no built library in target/{{ profile }}/"
        exit 1
    fi

    rm -f "$DEST"
    cp "$DYLIB" "$DEST"

    if [ "$(uname -s)" = "Darwin" ]; then
        codesign --force --sign - "$DEST"
    fi

    echo "Installed: $DEST"

    # ── Plugin files ──────────────────────────────────────────────────────
    # Install as direct out-of-store symlinks into ~/.steel/cogs/. Steel's
    # require resolver picks up both the entry file and the module dir
    # automatically via the cogs fallback. Editing plugin sources in-place
    # in the repo reflects immediately in Helix — no rebuild step.
    mkdir -p "{{ steel_cogs }}"
    rm -f "{{ steel_cogs }}/nothelix.scm"
    rm -f "{{ steel_cogs }}/nothelix"
    ln -s "{{ nothelix_root }}/plugin/nothelix.scm" "{{ steel_cogs }}/nothelix.scm"
    ln -s "{{ nothelix_root }}/plugin/nothelix" "{{ steel_cogs }}/nothelix"
    echo "Linked:    {{ steel_cogs }}/nothelix.scm"
    echo "Linked:    {{ steel_cogs }}/nothelix/"

    # ── Warn about stale conflicting paths ────────────────────────────────
    # Helix's Steel engine searches ~/.config/helix/ before $STEEL_HOME/cogs.
    # A pre-existing nothelix.scm or nothelix/ directory there will shadow
    # this install and keep loading stale code.
    config_helix="$HOME/.config/helix"
    if [ -e "$config_helix/nothelix.scm" ] || [ -e "$config_helix/nothelix" ]; then
        echo ""
        echo "warning: found legacy install under $config_helix/"
        echo "         Helix searches that directory before ~/.steel/cogs and will"
        echo "         load the stale copy. Remove these entries (and any home-manager"
        echo "         rules that manage them) before restarting Helix:"
        [ -e "$config_helix/nothelix.scm" ] && echo "           rm $config_helix/nothelix.scm"
        [ -e "$config_helix/nothelix" ]     && echo "           rm -rf $config_helix/nothelix"
    fi

# Julia bootstrap: LSP wrapper + LSP env + kernel runtime dep (re-run after a Julia version change)
setup-lsp:
    #!/usr/bin/env sh
    set -eu
    mkdir -p "{{ local_bin }}"
    cp "{{ nothelix_root }}/lsp/julia-lsp" "{{ local_bin }}/julia-lsp"
    chmod +x "{{ local_bin }}/julia-lsp"
    echo "Installed: {{ local_bin }}/julia-lsp"

    if command -v julia >/dev/null 2>&1; then
        echo "Setting up Julia LSP environment (patched SymbolServer for 1.14)..."
        # Mirror lsp.rs::ensure_lsp_environment exactly: the runtime env lives
        # in the data dir, NOT the repo. Warming it here avoids a ~2 min wait
        # on the user's first .jl open (where lsp.rs would otherwise build it).
        runtime="${XDG_DATA_HOME:-$HOME/.local/share}/nothelix/lsp"
        mkdir -p "$runtime/depot"
        printf '[deps]\nLanguageServer = "2b0e0bc5-e4fd-59b4-8912-456d1b03d8d7"\n' > "$runtime/Project.toml"
        cp "{{ nothelix_root }}/lsp/symbolserver-1.14.patch" "$runtime/symbolserver-1.14.patch"
        cp "{{ nothelix_root }}/lsp/bootstrap.jl" "$runtime/bootstrap.jl"
        JULIA_DEPOT_PATH="$runtime/depot" \
            julia --startup-file=no --quiet --project="$runtime" \
            "$runtime/bootstrap.jl" "$runtime" "$runtime/symbolserver-1.14.patch" "$runtime/NothelixMacros"
        # The kernel and JETLS both resolve `using` against the user's default
        # env. JSON3 is the kernel's runtime dep; NothelixMacros provides the
        # @cell/@markdown markers JETLS must resolve to analyse a notebook. A
        # Julia version bump gives a fresh empty env, so ensure both here.
        echo "Ensuring default-env deps (JSON3, NothelixMacros)..."
        julia --startup-file=no --quiet -e 'using Pkg
            deps = Pkg.project().dependencies
            haskey(deps, "JSON3") || Pkg.add("JSON3")
            haskey(deps, "NothelixMacros") || Pkg.develop(path="{{ nothelix_root }}/lsp/NothelixMacros")'
    else
        echo "julia not on PATH — skipping Julia env warm-up (set up on first .jl open)"
    fi

# build without installing
build profile="release":
    {{ if profile == "debug" { "cargo build -p libnothelix" } else { "cargo build --release -p libnothelix" } }}

# run tests
test:
    cargo nextest run -p libnothelix

# static gate: run before committing. Rust lints + tests, then load the
# plugin in a REAL hx binary to catch Steel load errors (FreeIdentifier,
# BadSyntax, ArityMismatch). Standalone `steel` can't do the Steel half —
# it lacks helix's native builtins, so it can't resolve `require-builtin
# helix/core/*` or check `helix.static.*` arities.
check:
    #!/usr/bin/env sh
    set -eu
    echo "── clippy ──"
    cargo clippy -p libnothelix --all-targets -- -D warnings
    echo "── nextest ──"
    command -v cargo-nextest >/dev/null 2>&1 || cargo install --locked cargo-nextest
    cargo nextest run -p libnothelix
    echo "── plugin load ──"
    "{{ nothelix_root }}/scripts/check-plugin.sh"

# remove the installed dylib, plugin symlinks, and julia-lsp
uninstall:
    rm -f "{{ steel_native }}/libnothelix.dylib"
    rm -f "{{ steel_native }}/libnothelix.so"
    rm -f "{{ steel_cogs }}/nothelix.scm"
    rm -f "{{ steel_cogs }}/nothelix"
    rm -f "{{ local_bin }}/julia-lsp"
    @echo "Uninstalled nothelix"

# list available recipes
default:
    @just --list
