# nothelix task runner
#
# macOS invalidates a dylib's code signature when the underlying file changes.
# The kernel SIGKILL's the process with "Code Signature Invalid" the next time
# it pages in code from the stale binary — but only when Helix actually calls
# into the dylib, not during `hx --version`, which makes the failure look
# non-deterministic. Symlinks don't help because codesign stamps the real file.
# The install recipe handles the full rm + cp + codesign sequence so you don't
# have to remember it.

set shell := ["sh", "-euc"]

steel_native := env("HOME") / ".steel" / "native"

# build and install the dylib (default: release)
install profile="release":
    #!/usr/bin/env sh
    set -eu
    if [ "{{ profile }}" = "debug" ]; then
        echo "Building libnothelix (debug)..."
        cargo build -p libnothelix
    else
        echo "Building libnothelix (release)..."
        cargo build --release -p libnothelix
    fi

    mkdir -p "{{ steel_native }}"

    if [ -f "target/{{ profile }}/libnothelix.dylib" ]; then
        DYLIB="target/{{ profile }}/libnothelix.dylib"
        DEST="{{ steel_native }}/libnothelix.dylib"
    elif [ -f "target/{{ profile }}/libnothelix.so" ]; then
        DYLIB="target/{{ profile }}/libnothelix.so"
        DEST="{{ steel_native }}/libnothelix.so"
    else
        echo "error: no built library in target/{{ profile }}/"
        exit 1
    fi

    rm -f "$DEST"
    cp "$DYLIB" "$DEST"

    if [ "$(uname -s)" = "Darwin" ]; then
        codesign --force --sign - "$DEST"
    fi

    echo "Installed: $DEST"

# build without installing
build profile="release":
    {{ if profile == "debug" { "cargo build -p libnothelix" } else { "cargo build --release -p libnothelix" } }}

# run tests
test:
    cargo test -p libnothelix

# remove installed dylib
uninstall:
    rm -f "{{ steel_native }}/libnothelix.dylib"
    rm -f "{{ steel_native }}/libnothelix.so"
    @echo "Uninstalled nothelix"

# list available recipes
default:
    @just --list
