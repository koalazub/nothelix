#!/usr/bin/env bats

setup() {
    export WRAPPER="$BATS_TEST_DIRNAME/../../dist/nothelix"

    # Fake a complete install under a temp HOME
    FAKE_HOME="$(mktemp -d)"
    export HOME="$FAKE_HOME"
    export STEEL_HOME="$FAKE_HOME/.steel"

    mkdir -p "$HOME/.local/bin"
    mkdir -p "$STEEL_HOME/native"
    mkdir -p "$STEEL_HOME/cogs/nothelix"
    mkdir -p "$HOME/.local/share/nothelix/runtime/grammars"
    mkdir -p "$HOME/.local/share/nothelix/runtime/queries"
    mkdir -p "$HOME/.local/share/nothelix/runtime/themes"
    mkdir -p "$HOME/.local/share/nothelix/examples"
    mkdir -p "$HOME/.config/helix"
    mkdir -p "$HOME/.local/share/nothelix/lsp"

    echo "#!/bin/bash" > "$HOME/.local/bin/hx-nothelix"
    chmod +x "$HOME/.local/bin/hx-nothelix"
    cp "$WRAPPER" "$HOME/.local/bin/nothelix"
    echo "#!/bin/bash" > "$HOME/.local/bin/julia-lsp"
    chmod +x "$HOME/.local/bin/julia-lsp"

    echo "fake" > "$STEEL_HOME/native/libnothelix.dylib"
    echo "BUILD_ID=ci-20260412-abcdef12" > "$STEEL_HOME/native/libnothelix.meta"
    echo "# plugin" > "$STEEL_HOME/cogs/nothelix.scm"
    echo "# sub" > "$STEEL_HOME/cogs/nothelix/execution.scm"

    touch "$HOME/.local/share/nothelix/runtime/grammars/rust.so"
    touch "$HOME/.local/share/nothelix/examples/demo.jl"
    touch "$HOME/.local/share/nothelix/lsp/Manifest.toml"
    # Make Manifest.toml non-empty
    echo "# manifest stub" > "$HOME/.local/share/nothelix/lsp/Manifest.toml"

    echo '(require "nothelix.scm")' > "$HOME/.config/helix/init.scm"

    cat > "$HOME/.local/share/nothelix/VERSION" <<EOF
NOTHELIX_VERSION=v0.2.1
BUILD_ID=ci-20260412-abcdef12
FORK_SHA=89734c7291a9
FORK_BRANCH=feature/inline-image-rendering
LIBNOTHELIX_VERSION=v0.2.1
INSTALL_DATE=2026-04-12T03:14:15Z
EOF

    export PATH="$HOME/.local/bin:$PATH"
    export NOTHELIX_SHARE="$HOME/.local/share/nothelix"
    export NOTHELIX_SKIP_TTY_CHECK=1   # skip terminal graphics query in tests
}

teardown() {
    rm -rf "$FAKE_HOME"
}

@test "doctor exits 0 when every check passes" {
    run "$WRAPPER" doctor
    [ "$status" -eq 0 ]
    [[ "$output" == *"hx-nothelix"* ]]
    [[ "$output" == *"libnothelix"* ]]
    [[ "$output" == *"plugin cogs"* ]]
    [[ "$output" == *"build id"* ]]
    [[ "$output" == *"checks passed"* ]] || [[ "$output" == *"Ready to go"* ]]
}

@test "doctor fails if hx-nothelix is missing" {
    rm "$HOME/.local/bin/hx-nothelix"
    run "$WRAPPER" doctor
    [ "$status" -ne 0 ]
    [[ "$output" == *"hx-nothelix"* ]]
    [[ "$output" == *"missing"* ]] || [[ "$output" == *"not found"* ]] || [[ "$output" == *"fail"* ]]
}

@test "doctor fails if libnothelix is missing" {
    rm "$STEEL_HOME/native/libnothelix.dylib"
    run "$WRAPPER" doctor
    [ "$status" -ne 0 ]
    [[ "$output" == *"libnothelix"* ]]
}

@test "doctor fails if BUILD_ID in VERSION mismatches libnothelix.meta" {
    echo "BUILD_ID=ci-20260412-FFFFFFFF" > "$STEEL_HOME/native/libnothelix.meta"
    run "$WRAPPER" doctor
    [ "$status" -ne 0 ]
    [[ "$output" == *"build id"* ]]
    [[ "$output" == *"mismatch"* ]] || [[ "$output" == *"drift"* ]]
}

@test "doctor fails if init.scm is missing the require line" {
    echo "" > "$HOME/.config/helix/init.scm"
    run "$WRAPPER" doctor
    [ "$status" -ne 0 ]
    [[ "$output" == *"init.scm"* ]]
}

@test "doctor warns but succeeds when grammars dir is empty" {
    rm "$HOME/.local/share/nothelix/runtime/grammars"/*
    run "$WRAPPER" doctor
    # Empty grammars is a warn, not a fail
    [ "$status" -eq 0 ]
    [[ "$output" == *"grammar"* ]]
}
