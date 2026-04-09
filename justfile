# nothelix task runner
#
# `just install` is the single source of truth for installing nothelix:
#
#   ~/.steel/native/libnothelix.{dylib,so}  — FFI dylib, codesigned on macOS
#   ~/.steel/cogs/nothelix.scm              — plugin entry symlink
#   ~/.steel/cogs/nothelix/                 — plugin module directory symlink
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

# build + install dylib, plugin, julia-lsp, and set up the LSP environment
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

    # ── Julia LSP wrapper ─────────────────────────────────────────────────
    mkdir -p "{{ local_bin }}"
    cp "{{ nothelix_root }}/lsp/julia-lsp" "{{ local_bin }}/julia-lsp"
    chmod +x "{{ local_bin }}/julia-lsp"
    echo "Installed: {{ local_bin }}/julia-lsp"

    # ── LSP environment ───────────────────────────────────────────────────
    echo "Setting up Julia LSP environment..."
    julia --startup-file=no --quiet --project="{{ nothelix_root }}/lsp" -e 'using Pkg; Pkg.instantiate()'

# build without installing
build profile="release":
    {{ if profile == "debug" { "cargo build -p libnothelix" } else { "cargo build --release -p libnothelix" } }}

# run tests
test:
    cargo test -p libnothelix

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
