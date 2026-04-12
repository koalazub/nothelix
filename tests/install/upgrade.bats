#!/usr/bin/env bats

# install.sh caches the extracted tarball under NOTHELIX_SHARE/.cache/
# so `nothelix reset` can re-use it. This suite verifies both the
# cache behaviour and the wrapper's upgrade dispatch in test mode.

setup() {
    FAKE_HOME="$(mktemp -d)"
    export HOME="$FAKE_HOME"

    FIXTURE_DIR="$(mktemp -d)"
    mkdir -p "$FIXTURE_DIR/release"

    TARBALL_SRC="$(mktemp -d)/nothelix-vtest-darwin-arm64"
    mkdir -p "$TARBALL_SRC/bin" "$TARBALL_SRC/lib"
    mkdir -p "$TARBALL_SRC/share/nothelix/runtime/grammars"
    mkdir -p "$TARBALL_SRC/share/nothelix/examples"
    mkdir -p "$TARBALL_SRC/share/nothelix/plugin/nothelix"
    mkdir -p "$TARBALL_SRC/share/nothelix/lsp"

    echo "#!/bin/bash" > "$TARBALL_SRC/bin/hx-nothelix"
    chmod +x "$TARBALL_SRC/bin/hx-nothelix"
    cp "$BATS_TEST_DIRNAME/../../dist/nothelix" "$TARBALL_SRC/bin/nothelix"
    chmod +x "$TARBALL_SRC/bin/nothelix"
    echo "#!/bin/bash" > "$TARBALL_SRC/bin/julia-lsp"
    chmod +x "$TARBALL_SRC/bin/julia-lsp"

    echo "fake" > "$TARBALL_SRC/lib/libnothelix.dylib"
    echo "BUILD_ID=ci-test-00000000" > "$TARBALL_SRC/lib/libnothelix.meta"
    echo "# plugin" > "$TARBALL_SRC/share/nothelix/plugin/nothelix.scm"
    echo "# sub" > "$TARBALL_SRC/share/nothelix/plugin/nothelix/execution.scm"
    echo "# demo" > "$TARBALL_SRC/share/nothelix/examples/demo.jl"

    cat > "$TARBALL_SRC/VERSION" <<EOF
NOTHELIX_VERSION=vtest
BUILD_ID=ci-test-00000000
FORK_SHA=0000000000000000000000000000000000000000
FORK_BRANCH=feature/inline-image-rendering
LIBNOTHELIX_VERSION=vtest
INSTALL_DATE=2026-04-12T00:00:00Z
EOF

    cp "$BATS_TEST_DIRNAME/../../dist/install-local.sh" "$TARBALL_SRC/install-local.sh"
    chmod +x "$TARBALL_SRC/install-local.sh"

    tar -czf "$FIXTURE_DIR/release/nothelix-vtest-darwin-arm64.tar.gz" \
        -C "$(dirname "$TARBALL_SRC")" "$(basename "$TARBALL_SRC")"
    (cd "$FIXTURE_DIR/release" && shasum -a 256 nothelix-vtest-darwin-arm64.tar.gz > SHA256SUMS)

    export NOTHELIX_RELEASE_URL="file://$FIXTURE_DIR/release"
    export NOTHELIX_VERSION_OVERRIDE="vtest"
    export NOTHELIX_PLATFORM_OVERRIDE="darwin-arm64"
    export STEEL_HOME="$FAKE_HOME/.steel"
    export NOTHELIX_SHARE="$FAKE_HOME/.local/share/nothelix"

    INSTALL_SH="$BATS_TEST_DIRNAME/../../install.sh"
    WRAPPER="$BATS_TEST_DIRNAME/../../dist/nothelix"
}

teardown() {
    rm -rf "$FAKE_HOME" "$FIXTURE_DIR" "${TARBALL_SRC%/*}"
}

@test "install.sh caches extracted tarball under NOTHELIX_SHARE/.cache/extracted" {
    bash "$INSTALL_SH"
    [ -d "$NOTHELIX_SHARE/.cache/extracted" ]
    [ -x "$NOTHELIX_SHARE/.cache/extracted/install-local.sh" ]
    [ -f "$NOTHELIX_SHARE/.cache/extracted/VERSION" ]
}

@test "install.sh --upgrade refreshes cache idempotently" {
    bash "$INSTALL_SH"
    run bash "$INSTALL_SH" --upgrade
    [ "$status" -eq 0 ]
    [ -d "$NOTHELIX_SHARE/.cache/extracted" ]
    # init.scm should have exactly one require line
    run grep -c 'require "nothelix.scm"' "$HOME/.config/helix/init.scm"
    [ "$output" = "1" ]
}

@test "wrapper upgrade in test mode reports the install URL" {
    NOTHELIX_TEST_MODE=1 run "$WRAPPER" upgrade
    [ "$status" -eq 0 ]
    [[ "$output" == *"upgrade"* ]]
    [[ "$output" == *"install.sh"* ]]
}
