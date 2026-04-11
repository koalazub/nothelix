#!/usr/bin/env bats

setup() {
    FAKE_HOME="$(mktemp -d)"
    export HOME="$FAKE_HOME"
    export STEEL_HOME="$FAKE_HOME/.steel"
    export NOTHELIX_SHARE="$HOME/.local/share/nothelix"

    # Simulate a complete install that nothelix reset can operate on.
    mkdir -p "$NOTHELIX_SHARE/examples" "$NOTHELIX_SHARE/runtime/grammars"
    mkdir -p "$NOTHELIX_SHARE/lsp/depot/packages" "$NOTHELIX_SHARE/kernel-scripts"
    mkdir -p "$STEEL_HOME/cogs/nothelix" "$STEEL_HOME/native"
    mkdir -p "$HOME/.local/bin"

    # Fake a cached tarball for reset to copy from
    CACHE_DIR="$NOTHELIX_SHARE/.cache"
    mkdir -p "$CACHE_DIR/extracted/bin" "$CACHE_DIR/extracted/lib"
    mkdir -p "$CACHE_DIR/extracted/share/nothelix/examples"
    mkdir -p "$CACHE_DIR/extracted/share/nothelix/plugin/nothelix"
    mkdir -p "$CACHE_DIR/extracted/share/nothelix/runtime/grammars"
    mkdir -p "$CACHE_DIR/extracted/share/nothelix/lsp"

    echo "fresh hx" > "$CACHE_DIR/extracted/bin/hx-nothelix"
    echo "fresh nothelix" > "$CACHE_DIR/extracted/bin/nothelix"
    echo "fresh julia-lsp" > "$CACHE_DIR/extracted/bin/julia-lsp"
    echo "fresh dylib" > "$CACHE_DIR/extracted/lib/libnothelix.dylib"
    echo "BUILD_ID=ci-fresh-00000000" > "$CACHE_DIR/extracted/lib/libnothelix.meta"
    echo "fresh plugin" > "$CACHE_DIR/extracted/share/nothelix/plugin/nothelix.scm"
    echo "fresh sub" > "$CACHE_DIR/extracted/share/nothelix/plugin/nothelix/execution.scm"
    echo "fresh demo" > "$CACHE_DIR/extracted/share/nothelix/examples/demo.jl"
    cp "$BATS_TEST_DIRNAME/../../dist/install-local.sh" "$CACHE_DIR/extracted/install-local.sh"
    chmod +x "$CACHE_DIR/extracted/install-local.sh"
    cat > "$CACHE_DIR/extracted/VERSION" <<EOF
NOTHELIX_VERSION=v0.2.1
BUILD_ID=ci-fresh-00000000
FORK_SHA=0000000000000000000000000000000000000000
FORK_BRANCH=feature/inline-image-rendering
LIBNOTHELIX_VERSION=v0.2.1
INSTALL_DATE=2026-04-12T00:00:00Z
EOF

    export WRAPPER="$BATS_TEST_DIRNAME/../../dist/nothelix"
}

teardown() {
    rm -rf "$FAKE_HOME"
}

@test "reset (no flags) re-copies managed files from cache" {
    echo "stale hx" > "$HOME/.local/bin/hx-nothelix"
    run "$WRAPPER" reset
    [ "$status" -eq 0 ]
    run cat "$HOME/.local/bin/hx-nothelix"
    [[ "$output" == *"fresh hx"* ]]
}

@test "reset leaves LSP depot alone by default" {
    echo "precompile artefact" > "$NOTHELIX_SHARE/lsp/depot/packages/foo"
    run "$WRAPPER" reset
    [ "$status" -eq 0 ]
    [ -f "$NOTHELIX_SHARE/lsp/depot/packages/foo" ]
}

@test "reset --lsp wipes the LSP depot" {
    echo "precompile artefact" > "$NOTHELIX_SHARE/lsp/depot/packages/foo"
    run "$WRAPPER" reset --lsp
    [ "$status" -eq 0 ]
    [ ! -f "$NOTHELIX_SHARE/lsp/depot/packages/foo" ]
}

@test "reset --kernel wipes kernel-scripts" {
    echo "extracted kernel" > "$NOTHELIX_SHARE/kernel-scripts/runner.jl"
    run "$WRAPPER" reset --kernel
    [ "$status" -eq 0 ]
    [ ! -f "$NOTHELIX_SHARE/kernel-scripts/runner.jl" ]
}

@test "reset --all wipes both LSP depot and kernel, and re-copies files" {
    echo "precompile" > "$NOTHELIX_SHARE/lsp/depot/packages/foo"
    echo "kernel" > "$NOTHELIX_SHARE/kernel-scripts/runner.jl"
    echo "stale hx" > "$HOME/.local/bin/hx-nothelix"
    run "$WRAPPER" reset --all
    [ "$status" -eq 0 ]
    [ ! -f "$NOTHELIX_SHARE/lsp/depot/packages/foo" ]
    [ ! -f "$NOTHELIX_SHARE/kernel-scripts/runner.jl" ]
    run cat "$HOME/.local/bin/hx-nothelix"
    [[ "$output" == *"fresh hx"* ]]
}

@test "reset never touches init.scm" {
    mkdir -p "$HOME/.config/helix"
    echo '(require "nothelix.scm")' > "$HOME/.config/helix/init.scm"
    echo '(my-user-code)' >> "$HOME/.config/helix/init.scm"
    run "$WRAPPER" reset
    [ "$status" -eq 0 ]
    run grep "my-user-code" "$HOME/.config/helix/init.scm"
    [ "$status" -eq 0 ]
}
