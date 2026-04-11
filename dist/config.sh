#!/bin/bash
# config.sh — `nothelix config` subcommand handlers.
# Sourced by dist/nothelix.

nothelix_config_show() {
    if [ ! -f "$VERSION_FILE" ]; then
        echo "nothelix: VERSION file not found at $VERSION_FILE" >&2
        exit 1
    fi
    # shellcheck disable=SC1090
    . "$VERSION_FILE"
    local julia_path="(not found)"
    local julia_version="unknown"
    if command -v julia >/dev/null 2>&1; then
        julia_path=$(command -v julia)
        julia_version=$(julia --version 2>&1 | head -1 | sed 's/^julia //')
    fi
    cat <<EOF
nothelix.version     = ${NOTHELIX_VERSION:-unknown}
nothelix.fork_sha    = ${FORK_SHA:-unknown}
nothelix.fork_branch = ${FORK_BRANCH:-unknown}
nothelix.build_id    = ${BUILD_ID:-unknown}
nothelix.install_dir = $NOTHELIX_SHARE
steel.home           = $STEEL_HOME
steel.native         = $STEEL_HOME/native/libnothelix.dylib
steel.cogs           = $STEEL_HOME/cogs/nothelix
helix.runtime        = $NOTHELIX_SHARE/runtime
helix.init_scm       = $HOME/.config/helix/init.scm
helix.config_toml    = $HOME/.config/helix/config.toml
julia.path           = $julia_path
julia.version        = $julia_version
lsp.env              = $NOTHELIX_SHARE/lsp
lsp.depot            = $NOTHELIX_SHARE/lsp/depot
demo.notebook        = $NOTHELIX_SHARE/examples/demo.jl
EOF
}

nothelix_config_path() {
    printf '%s\n' "$HOME/.config/helix/config.toml"
}

nothelix_config_edit() {
    local config_toml="$HOME/.config/helix/config.toml"
    mkdir -p "$(dirname "$config_toml")"
    if [ ! -f "$config_toml" ]; then
        printf 'theme = "default"\n' > "$config_toml"
    fi
    _run_or_print "$HX_NOTHELIX" "$config_toml"
}
