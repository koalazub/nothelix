#!/usr/bin/env bats

setup() {
    FAKE_HOME="$(mktemp -d)"
    export HOME="$FAKE_HOME"
    export STEEL_HOME="$FAKE_HOME/.steel"
    export NOTHELIX_SHARE="$HOME/.local/share/nothelix"
    mkdir -p "$NOTHELIX_SHARE"
    cat > "$NOTHELIX_SHARE/VERSION" <<EOF
NOTHELIX_VERSION=v0.2.1
BUILD_ID=ci-20260412-abcdef12
FORK_SHA=89734c7291a9
FORK_BRANCH=feature/inline-image-rendering
LIBNOTHELIX_VERSION=v0.2.1
INSTALL_DATE=2026-04-12T03:14:15Z
EOF
    export WRAPPER="$BATS_TEST_DIRNAME/../../dist/nothelix"
    export NOTHELIX_TEST_MODE=1
}

teardown() {
    rm -rf "$FAKE_HOME"
}

@test "config show prints key=value lines" {
    run "$WRAPPER" config show
    [ "$status" -eq 0 ]
    [[ "$output" == *"nothelix.version"*"v0.2.1"* ]]
    [[ "$output" == *"nothelix.fork_sha"*"89734c7291a9"* ]]
    [[ "$output" == *"steel.home"* ]]
}

@test "config (no verb) is an alias for show" {
    run "$WRAPPER" config
    [ "$status" -eq 0 ]
    [[ "$output" == *"nothelix.version"* ]]
}

@test "config path prints the helix config.toml path" {
    run "$WRAPPER" config path
    [ "$status" -eq 0 ]
    [[ "$output" == *"$HOME/.config/helix/config.toml"* ]]
}

@test "config edit (in test mode) would exec hx-nothelix on config.toml" {
    run "$WRAPPER" config edit
    [ "$status" -eq 0 ]
    [[ "$output" == *"hx-nothelix"* ]]
    [[ "$output" == *"config.toml"* ]]
}
