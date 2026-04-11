#!/usr/bin/env bats

# Build a minimal fake tarball directory, run install-local.sh against
# a fake $HOME, assert files land in the right places.

setup() {
    # Fake tarball source dir
    TARBALL_DIR="$(mktemp -d)"
    mkdir -p "$TARBALL_DIR/bin"
    mkdir -p "$TARBALL_DIR/lib"
    mkdir -p "$TARBALL_DIR/share/nothelix/runtime/grammars"
    mkdir -p "$TARBALL_DIR/share/nothelix/examples"
    mkdir -p "$TARBALL_DIR/share/nothelix/plugin/nothelix"
    mkdir -p "$TARBALL_DIR/share/nothelix/lsp"
    mkdir -p "$TARBALL_DIR/share/nothelix/dist/doctor"
    echo "# fake doctor helper" > "$TARBALL_DIR/share/nothelix/dist/doctor/static.sh"

    # Stub files the installer will copy
    echo "#!/bin/bash" > "$TARBALL_DIR/bin/hx-nothelix"
    echo "echo stub hx-nothelix" >> "$TARBALL_DIR/bin/hx-nothelix"
    chmod +x "$TARBALL_DIR/bin/hx-nothelix"

    echo "#!/bin/bash" > "$TARBALL_DIR/bin/nothelix"
    echo "echo stub nothelix" >> "$TARBALL_DIR/bin/nothelix"
    chmod +x "$TARBALL_DIR/bin/nothelix"

    echo "#!/bin/bash" > "$TARBALL_DIR/bin/julia-lsp"
    chmod +x "$TARBALL_DIR/bin/julia-lsp"

    echo "fake dylib" > "$TARBALL_DIR/lib/libnothelix.dylib"
    echo "BUILD_ID=ci-20260412-abcdef12" > "$TARBALL_DIR/lib/libnothelix.meta"

    echo "# fake plugin" > "$TARBALL_DIR/share/nothelix/plugin/nothelix.scm"
    echo "# fake submodule" > "$TARBALL_DIR/share/nothelix/plugin/nothelix/execution.scm"
    echo "# demo" > "$TARBALL_DIR/share/nothelix/examples/demo.jl"

    cat > "$TARBALL_DIR/VERSION" <<EOF
NOTHELIX_VERSION=v0.2.1
BUILD_ID=ci-20260412-abcdef12
FORK_SHA=89734c7291a9
FORK_BRANCH=feature/inline-image-rendering
LIBNOTHELIX_VERSION=v0.2.1
INSTALL_DATE=2026-04-12T03:14:15Z
EOF

    cp "$BATS_TEST_DIRNAME/../../dist/install-local.sh" "$TARBALL_DIR/install-local.sh"
    chmod +x "$TARBALL_DIR/install-local.sh"

    # Fake HOME
    FAKE_HOME="$(mktemp -d)"
    export HOME="$FAKE_HOME"
    export STEEL_HOME="$FAKE_HOME/.steel"
}

teardown() {
    rm -rf "$TARBALL_DIR" "$FAKE_HOME"
}

@test "install-local places hx-nothelix in ~/.local/bin" {
    run "$TARBALL_DIR/install-local.sh" "$TARBALL_DIR"
    [ "$status" -eq 0 ]
    [ -x "$HOME/.local/bin/hx-nothelix" ]
}

@test "install-local places the wrapper" {
    run "$TARBALL_DIR/install-local.sh" "$TARBALL_DIR"
    [ "$status" -eq 0 ]
    [ -x "$HOME/.local/bin/nothelix" ]
}

@test "install-local places julia-lsp" {
    run "$TARBALL_DIR/install-local.sh" "$TARBALL_DIR"
    [ "$status" -eq 0 ]
    [ -x "$HOME/.local/bin/julia-lsp" ]
}

@test "install-local places libnothelix.dylib and .meta" {
    run "$TARBALL_DIR/install-local.sh" "$TARBALL_DIR"
    [ "$status" -eq 0 ]
    [ -f "$HOME/.steel/native/libnothelix.dylib" ]
    [ -f "$HOME/.steel/native/libnothelix.meta" ]
}

@test "install-local places plugin cogs" {
    run "$TARBALL_DIR/install-local.sh" "$TARBALL_DIR"
    [ "$status" -eq 0 ]
    [ -f "$HOME/.steel/cogs/nothelix.scm" ]
    [ -f "$HOME/.steel/cogs/nothelix/execution.scm" ]
}

@test "install-local places runtime + demo + VERSION" {
    run "$TARBALL_DIR/install-local.sh" "$TARBALL_DIR"
    [ "$status" -eq 0 ]
    [ -d "$HOME/.local/share/nothelix/runtime/grammars" ]
    [ -f "$HOME/.local/share/nothelix/examples/demo.jl" ]
    [ -f "$HOME/.local/share/nothelix/VERSION" ]
}

@test "install-local appends require line to init.scm when absent" {
    run "$TARBALL_DIR/install-local.sh" "$TARBALL_DIR"
    [ "$status" -eq 0 ]
    [ -f "$HOME/.config/helix/init.scm" ]
    run grep 'require "nothelix.scm"' "$HOME/.config/helix/init.scm"
    [ "$status" -eq 0 ]
}

@test "install-local is idempotent on init.scm" {
    "$TARBALL_DIR/install-local.sh" "$TARBALL_DIR"
    "$TARBALL_DIR/install-local.sh" "$TARBALL_DIR"
    # Should have exactly one require line, not two
    run grep -c 'require "nothelix.scm"' "$HOME/.config/helix/init.scm"
    [ "$output" = "1" ]
}

@test "install-local preserves other init.scm content" {
    mkdir -p "$HOME/.config/helix"
    cat > "$HOME/.config/helix/init.scm" <<EOF
(require "my-custom-plugin.scm")
(define my-var 42)
EOF
    run "$TARBALL_DIR/install-local.sh" "$TARBALL_DIR"
    [ "$status" -eq 0 ]
    run grep "my-custom-plugin" "$HOME/.config/helix/init.scm"
    [ "$status" -eq 0 ]
    run grep "my-var" "$HOME/.config/helix/init.scm"
    [ "$status" -eq 0 ]
    run grep 'require "nothelix.scm"' "$HOME/.config/helix/init.scm"
    [ "$status" -eq 0 ]
}
