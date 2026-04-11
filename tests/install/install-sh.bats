#!/usr/bin/env bats

# install.sh is harder to unit-test because it hits the network. We
# use a MOCK_GH_RELEASES env var to point at a local file:// URL
# during tests so the installer downloads from a local fixture dir
# instead of GitHub.

setup() {
    FAKE_HOME="$(mktemp -d)"
    export HOME="$FAKE_HOME"

    # Build a local fake release: a tarball in a temp dir
    FIXTURE_DIR="$(mktemp -d)"
    mkdir -p "$FIXTURE_DIR/release"

    # Assemble the tarball contents matching Task 6's layout
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
    echo "# submod" > "$TARBALL_SRC/share/nothelix/plugin/nothelix/execution.scm"
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

    # Pack into a .tar.gz in FIXTURE_DIR/release/
    tar -czf "$FIXTURE_DIR/release/nothelix-vtest-darwin-arm64.tar.gz" -C "$(dirname "$TARBALL_SRC")" "$(basename "$TARBALL_SRC")"
    (cd "$FIXTURE_DIR/release" && shasum -a 256 nothelix-vtest-darwin-arm64.tar.gz > SHA256SUMS)

    # Point installer at the fixture release dir
    export NOTHELIX_RELEASE_URL="file://$FIXTURE_DIR/release"
    export NOTHELIX_VERSION_OVERRIDE="vtest"
    export NOTHELIX_PLATFORM_OVERRIDE="darwin-arm64"
    export STEEL_HOME="$FAKE_HOME/.steel"

    INSTALL_SH="$BATS_TEST_DIRNAME/../../install.sh"
}

teardown() {
    rm -rf "$FAKE_HOME" "$FIXTURE_DIR" "${TARBALL_SRC%/*}"
}

@test "install.sh runs end to end with a local fixture release" {
    run bash "$INSTALL_SH"
    [ "$status" -eq 0 ]
    [ -x "$HOME/.local/bin/hx-nothelix" ]
    [ -x "$HOME/.local/bin/nothelix" ]
    [ -f "$HOME/.steel/native/libnothelix.dylib" ]
    [ -f "$HOME/.steel/native/libnothelix.meta" ]
    [ -f "$HOME/.local/share/nothelix/examples/demo.jl" ]
    [ -f "$HOME/.local/share/nothelix/VERSION" ]
}

@test "install.sh aborts if SHA256SUMS mismatches the tarball" {
    # Corrupt the SHA
    echo "0000000000000000000000000000000000000000000000000000000000000000  nothelix-vtest-darwin-arm64.tar.gz" > "$FIXTURE_DIR/release/SHA256SUMS"
    run bash "$INSTALL_SH"
    [ "$status" -ne 0 ]
    [[ "$output" == *"SHA256"* ]]
}

@test "install.sh aborts on unsupported platform" {
    export NOTHELIX_PLATFORM_OVERRIDE="freebsd-sparc"
    run bash "$INSTALL_SH"
    [ "$status" -ne 0 ]
    [[ "$output" == *"freebsd-sparc"* ]] || [[ "$output" == *"not supported"* ]]
}

@test "install.sh --upgrade is idempotent (two runs leave same state)" {
    bash "$INSTALL_SH"
    run bash "$INSTALL_SH" --upgrade
    [ "$status" -eq 0 ]
    [ -f "$HOME/.local/share/nothelix/VERSION" ]
    # init.scm should have exactly one require line
    run grep -c 'require "nothelix.scm"' "$HOME/.config/helix/init.scm"
    [ "$output" = "1" ]
}
