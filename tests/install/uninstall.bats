#!/usr/bin/env bats

setup() {
    FAKE_HOME="$(mktemp -d)"
    export HOME="$FAKE_HOME"
    export STEEL_HOME="$FAKE_HOME/.steel"
    export NOTHELIX_SHARE="$HOME/.local/share/nothelix"
    export NOTHELIX_BIN="$HOME/.local/bin"

    mkdir -p "$HOME/.local/bin"
    mkdir -p "$STEEL_HOME/native" "$STEEL_HOME/cogs/nothelix"
    mkdir -p "$NOTHELIX_SHARE/examples" "$NOTHELIX_SHARE/runtime/grammars"
    mkdir -p "$NOTHELIX_SHARE/lsp/depot" "$NOTHELIX_SHARE/kernel-scripts"
    mkdir -p "$HOME/.config/helix"
    mkdir -p "$HOME/.cache/helix"

    echo "binary" > "$HOME/.local/bin/hx-nothelix"
    echo "wrapper" > "$HOME/.local/bin/nothelix"
    echo "lsp wrapper" > "$HOME/.local/bin/julia-lsp"
    echo "dylib" > "$STEEL_HOME/native/libnothelix.dylib"
    echo "BUILD_ID=ci-test-00000000" > "$STEEL_HOME/native/libnothelix.meta"
    echo "plugin" > "$STEEL_HOME/cogs/nothelix.scm"
    echo "sub" > "$STEEL_HOME/cogs/nothelix/execution.scm"
    echo "demo" > "$NOTHELIX_SHARE/examples/demo.jl"
    echo "log contents" > "$HOME/.cache/helix/helix.log"

    cat > "$HOME/.config/helix/init.scm" <<EOF
(require "nothelix.scm")
(define my-custom 42)
EOF

    export WRAPPER="$BATS_TEST_DIRNAME/../../dist/nothelix"
    export NOTHELIX_TEST_MODE=0
}

teardown() {
    rm -rf "$FAKE_HOME"
}

@test "uninstall --dry-run removes nothing, lists plan" {
    run "$WRAPPER" uninstall --dry-run --yes
    [ "$status" -eq 0 ]
    [ -f "$HOME/.local/bin/hx-nothelix" ]
    [ -f "$STEEL_HOME/native/libnothelix.dylib" ]
    [[ "$output" == *"hx-nothelix"* ]]
    [[ "$output" == *"libnothelix"* ]]
}

@test "uninstall --yes removes all managed files" {
    run "$WRAPPER" uninstall --yes
    [ "$status" -eq 0 ]
    [ ! -f "$HOME/.local/bin/hx-nothelix" ]
    [ ! -f "$HOME/.local/bin/nothelix" ]
    [ ! -f "$HOME/.local/bin/julia-lsp" ]
    [ ! -f "$STEEL_HOME/native/libnothelix.dylib" ]
    [ ! -f "$STEEL_HOME/native/libnothelix.meta" ]
    [ ! -f "$STEEL_HOME/cogs/nothelix.scm" ]
    [ ! -d "$STEEL_HOME/cogs/nothelix" ]
    [ ! -d "$NOTHELIX_SHARE" ]
}

@test "uninstall --yes preserves user init.scm content except nothelix require line" {
    "$WRAPPER" uninstall --yes
    [ -f "$HOME/.config/helix/init.scm" ]
    run grep "my-custom" "$HOME/.config/helix/init.scm"
    [ "$status" -eq 0 ]
    run grep 'require "nothelix.scm"' "$HOME/.config/helix/init.scm"
    [ "$status" -ne 0 ]
}

@test "uninstall --yes deletes init.scm if it only contained our require" {
    echo '(require "nothelix.scm")' > "$HOME/.config/helix/init.scm"
    "$WRAPPER" uninstall --yes
    [ ! -f "$HOME/.config/helix/init.scm" ]
}

@test "uninstall --yes leaves ~/.julia alone" {
    mkdir -p "$HOME/.julia/packages/LinearAlgebra"
    touch "$HOME/.julia/packages/LinearAlgebra/fake"
    "$WRAPPER" uninstall --yes
    [ -f "$HOME/.julia/packages/LinearAlgebra/fake" ]
}

@test "uninstall --yes --keep-data preserves lsp/depot" {
    mkdir -p "$NOTHELIX_SHARE/lsp/depot/packages"
    echo "keep me" > "$NOTHELIX_SHARE/lsp/depot/packages/pkg"
    run "$WRAPPER" uninstall --yes --keep-data
    [ "$status" -eq 0 ]
    [ -f "$NOTHELIX_SHARE/lsp/depot/packages/pkg" ]
}

@test "uninstall --yes leaves ~/.cache/helix/helix.log alone by default" {
    "$WRAPPER" uninstall --yes
    [ -f "$HOME/.cache/helix/helix.log" ]
}

@test "uninstall --yes --purge also removes ~/.cache/helix/helix.log" {
    "$WRAPPER" uninstall --yes --purge
    [ ! -f "$HOME/.cache/helix/helix.log" ]
}
