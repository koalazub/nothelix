#!/usr/bin/env bats

# Smoke test requires Julia. Skip if not available.

setup() {
    if ! command -v julia >/dev/null 2>&1; then
        skip "julia not installed"
    fi

    # Capture the real Julia depot path BEFORE overriding HOME, so the kernel
    # subprocess can find installed packages (e.g. JSON3) regardless of HOME.
    REAL_JULIA_DEPOT=$(julia --startup-file=no --quiet \
        -e 'print(join(Base.DEPOT_PATH, ":"))' 2>/dev/null || true)
    export JULIA_DEPOT_PATH="$REAL_JULIA_DEPOT"

    FAKE_HOME="$(mktemp -d)"
    export HOME="$FAKE_HOME"
    export STEEL_HOME="$FAKE_HOME/.steel"
    export NOTHELIX_SHARE="$HOME/.local/share/nothelix"

    mkdir -p "$NOTHELIX_SHARE/kernel-scripts"
    mkdir -p "$NOTHELIX_SHARE/dist/doctor"
    mkdir -p "$HOME/.local/bin"

    # Copy real kernel scripts from repo
    cp "$BATS_TEST_DIRNAME/../../kernel/"*.jl "$NOTHELIX_SHARE/kernel-scripts/"

    # Copy doctor helpers from dist
    cp "$BATS_TEST_DIRNAME/../../dist/doctor/"*.sh "$NOTHELIX_SHARE/dist/doctor/"
    cp "$BATS_TEST_DIRNAME/../../dist/nothelix" "$HOME/.local/bin/nothelix"
    chmod +x "$HOME/.local/bin/nothelix"

    export WRAPPER="$HOME/.local/bin/nothelix"

    # Satisfy static checks so the wrapper exits 0 when smoke passes
    mkdir -p "$STEEL_HOME/native"
    mkdir -p "$STEEL_HOME/cogs/nothelix"
    mkdir -p "$NOTHELIX_SHARE/runtime/grammars"
    mkdir -p "$NOTHELIX_SHARE/runtime/queries"
    mkdir -p "$NOTHELIX_SHARE/examples"
    mkdir -p "$NOTHELIX_SHARE/lsp"
    mkdir -p "$HOME/.config/helix"

    echo "#!/bin/bash" > "$HOME/.local/bin/hx-nothelix"
    chmod +x "$HOME/.local/bin/hx-nothelix"

    echo "fake" > "$STEEL_HOME/native/libnothelix.dylib"
    echo "BUILD_ID=ci-smoke-test" > "$STEEL_HOME/native/libnothelix.meta"
    echo "# plugin" > "$STEEL_HOME/cogs/nothelix.scm"
    echo "# sub" > "$STEEL_HOME/cogs/nothelix/execution.scm"
    touch "$NOTHELIX_SHARE/runtime/grammars/rust.so"
    touch "$NOTHELIX_SHARE/examples/demo.jl"
    echo "# manifest stub" > "$NOTHELIX_SHARE/lsp/Manifest.toml"
    echo '(require "nothelix.scm")' > "$HOME/.config/helix/init.scm"

    cat > "$NOTHELIX_SHARE/VERSION" <<EOF
NOTHELIX_VERSION=v0.0.0-smoke
BUILD_ID=ci-smoke-test
FORK_SHA=000000000000
FORK_BRANCH=smoke
LIBNOTHELIX_VERSION=v0.0.0-smoke
INSTALL_DATE=2026-04-12T00:00:00Z
EOF

    export PATH="$HOME/.local/bin:$PATH"
    export NOTHELIX_SKIP_TTY_CHECK=1
}

teardown() {
    rm -rf "$FAKE_HOME"
}

@test "doctor --smoke spawns a Julia kernel and gets 1+1=2" {
    run "$WRAPPER" doctor --smoke
    if [ "$status" -ne 0 ]; then
        echo "$output"
    fi
    [ "$status" -eq 0 ]
    [[ "$output" == *"kernel smoke"* ]]
    [[ "$output" == *"cold start"* ]] || [[ "$output" == *"execute"* ]]
}
