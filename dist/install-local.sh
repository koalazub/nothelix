#!/bin/bash
# install-local.sh — in-tarball installer, invoked by install.sh after
# extraction or directly by developers who downloaded a tarball manually.
#
# Usage: install-local.sh <tarball-dir> [--upgrade|--uninstall]
#
# <tarball-dir> is the path to the extracted tarball root containing
# bin/, lib/, share/, VERSION, and this script.
#
# This script is idempotent: running it twice is equivalent to running
# it once. init.scm append is grep-then-append.

set -euo pipefail

TARBALL_DIR="${1:-}"
# shellcheck disable=SC2034  # MODE reserved for future --upgrade/--uninstall dispatch
MODE="${2:-install}"   # install | --upgrade | --uninstall

if [ -z "$TARBALL_DIR" ] || [ ! -d "$TARBALL_DIR" ]; then
    echo "install-local: usage: $0 <tarball-dir> [--upgrade|--uninstall]" >&2
    exit 2
fi

# ─── Paths ────────────────────────────────────────────────────────────
NOTHELIX_PREFIX="${NOTHELIX_PREFIX:-$HOME/.local}"
BIN_DIR="$NOTHELIX_PREFIX/bin"
SHARE_DIR="$NOTHELIX_PREFIX/share/nothelix"
STEEL_HOME="${STEEL_HOME:-$HOME/.steel}"
STEEL_NATIVE="$STEEL_HOME/native"
STEEL_COGS="$STEEL_HOME/cogs"
HELIX_CONFIG_DIR="$HOME/.config/helix"
INIT_SCM="$HELIX_CONFIG_DIR/init.scm"

# ─── Helpers ──────────────────────────────────────────────────────────
log() { printf "  %s\n" "$*"; }

place_file() {
    local src="$1"
    local dst="$2"
    mkdir -p "$(dirname "$dst")"
    cp "$src" "$dst"
    log "placing $(basename "$dst") -> $dst"
}

place_dir() {
    local src="$1"
    local dst="$2"
    mkdir -p "$(dirname "$dst")"
    rm -rf "$dst"
    cp -R "$src" "$dst"
    log "placing $(basename "$src")/ -> $dst"
}

append_init_scm_line() {
    local line='(require "nothelix.scm")'
    mkdir -p "$HELIX_CONFIG_DIR"
    if [ ! -f "$INIT_SCM" ]; then
        touch "$INIT_SCM"
    fi
    if grep -Fq "$line" "$INIT_SCM"; then
        log "init.scm already configured, skipping append"
    else
        # Ensure file ends with newline before appending
        if [ -s "$INIT_SCM" ] && [ "$(tail -c 1 "$INIT_SCM" | wc -l | tr -d ' ')" != "1" ]; then
            printf '\n' >> "$INIT_SCM"
        fi
        printf '%s\n' "$line" >> "$INIT_SCM"
        log "configuring init.scm ... added (require \"nothelix.scm\")"
    fi
}

# ─── Main ─────────────────────────────────────────────────────────────
echo "nothelix install-local"

# Binaries
place_file "$TARBALL_DIR/bin/hx-nothelix" "$BIN_DIR/hx-nothelix"
place_file "$TARBALL_DIR/bin/nothelix" "$BIN_DIR/nothelix"
place_file "$TARBALL_DIR/bin/julia-lsp" "$BIN_DIR/julia-lsp"
chmod +x "$BIN_DIR/hx-nothelix" "$BIN_DIR/nothelix" "$BIN_DIR/julia-lsp"

# Dylib (detect .dylib vs .so)
if [ -f "$TARBALL_DIR/lib/libnothelix.dylib" ]; then
    DYLIB_NAME="libnothelix.dylib"
elif [ -f "$TARBALL_DIR/lib/libnothelix.so" ]; then
    DYLIB_NAME="libnothelix.so"
else
    echo "install-local: no libnothelix.{dylib,so} in $TARBALL_DIR/lib/" >&2
    exit 1
fi
place_file "$TARBALL_DIR/lib/$DYLIB_NAME" "$STEEL_NATIVE/$DYLIB_NAME"
place_file "$TARBALL_DIR/lib/libnothelix.meta" "$STEEL_NATIVE/libnothelix.meta"

# Re-codesign on macOS (the tarball carries a CI signature; we re-sign
# after copy to survive the file being rewritten in a new inode).
if [ "$(uname -s)" = "Darwin" ]; then
    codesign --force --sign - "$BIN_DIR/hx-nothelix" 2>/dev/null || \
        log "warning: codesign failed for hx-nothelix (non-fatal)"
    codesign --force --sign - "$STEEL_NATIVE/$DYLIB_NAME" 2>/dev/null || \
        log "warning: codesign failed for $DYLIB_NAME (non-fatal)"
fi

# Plugin cogs
place_file "$TARBALL_DIR/share/nothelix/plugin/nothelix.scm" "$STEEL_COGS/nothelix.scm"
place_dir "$TARBALL_DIR/share/nothelix/plugin/nothelix" "$STEEL_COGS/nothelix"

# Runtime (Helix runtime with pre-built grammars)
place_dir "$TARBALL_DIR/share/nothelix/runtime" "$SHARE_DIR/runtime"

# Examples (demo notebook)
place_dir "$TARBALL_DIR/share/nothelix/examples" "$SHARE_DIR/examples"

# LSP env scaffold (Project.toml, Manifest.toml — NOT depot/)
place_dir "$TARBALL_DIR/share/nothelix/lsp" "$SHARE_DIR/lsp"

# Version metadata
place_file "$TARBALL_DIR/VERSION" "$SHARE_DIR/VERSION"

# init.scm configuration
append_init_scm_line

echo "nothelix install-local complete"
