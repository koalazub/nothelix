#!/bin/bash
# build-test-tarball.sh — assembles a local release tarball from the
# current working tree for end-to-end testing. Does not hit the
# network, does not tag a release. Output at /tmp/nothelix-test-release/.
#
# Assumes libnothelix and hx-nothelix are already built via `just install`
# and available under ~/.steel/native/ and ~/.local/bin/ respectively.

set -euo pipefail

cd "$(dirname "$0")/.."

OUT="/tmp/nothelix-test-release"
STAGING="$OUT/nothelix-vtest-local"

rm -rf "$OUT"
mkdir -p "$STAGING/bin" "$STAGING/lib"
mkdir -p "$STAGING/share/nothelix/runtime"
mkdir -p "$STAGING/share/nothelix/examples"
mkdir -p "$STAGING/share/nothelix/plugin"
mkdir -p "$STAGING/share/nothelix/lsp"
mkdir -p "$STAGING/share/nothelix/kernel-scripts"
mkdir -p "$STAGING/share/nothelix/dist"

if [ -x "$HOME/.local/bin/hx-nothelix" ]; then
    cp "$HOME/.local/bin/hx-nothelix" "$STAGING/bin/"
else
    cp "$HOME/projects/helix/target/release/hx" "$STAGING/bin/hx-nothelix"
fi
cp dist/nothelix "$STAGING/bin/nothelix"
cp lsp/julia-lsp "$STAGING/bin/julia-lsp"
chmod +x "$STAGING"/bin/*

if [ -f "$HOME/.steel/native/libnothelix.dylib" ]; then
    cp "$HOME/.steel/native/libnothelix.dylib" "$STAGING/lib/"
elif [ -f "target/release/libnothelix.dylib" ]; then
    cp target/release/libnothelix.dylib "$STAGING/lib/"
else
    echo "build-test-tarball: no libnothelix.dylib found; run 'just install' first" >&2
    exit 1
fi
cargo run -p libnothelix --bin nothelix-meta --release > "$STAGING/lib/libnothelix.meta"

if [ -d "$HOME/projects/helix/runtime" ]; then
    cp -R "$HOME/projects/helix/runtime"/* "$STAGING/share/nothelix/runtime/"
fi

cp plugin/nothelix.scm "$STAGING/share/nothelix/plugin/"
cp -R plugin/nothelix "$STAGING/share/nothelix/plugin/"

cp examples/demo.jl "$STAGING/share/nothelix/examples/"

cp lsp/Project.toml lsp/Manifest.toml "$STAGING/share/nothelix/lsp/"

cp kernel/*.jl "$STAGING/share/nothelix/kernel-scripts/"

cp -R dist/doctor "$STAGING/share/nothelix/dist/"
cp dist/config.sh dist/reset.sh dist/uninstall.sh "$STAGING/share/nothelix/dist/"

cp dist/install-local.sh "$STAGING/install-local.sh"

FORK_SHA=$(cat .helix-fork-rev)
BUILD_ID="dev-$(date -u +%Y%m%d)-$(git rev-parse --short=12 HEAD 2>/dev/null || echo local)"
cat > "$STAGING/VERSION" <<EOF
NOTHELIX_VERSION=vtest-local
BUILD_ID=${BUILD_ID}
FORK_SHA=${FORK_SHA}
FORK_BRANCH=feature/inline-image-rendering
LIBNOTHELIX_VERSION=$(grep '^version' libnothelix/Cargo.toml | head -1 | cut -d'"' -f2)
INSTALL_DATE=$(date -u +%Y-%m-%dT%H:%M:%SZ)
EOF

tar -czf "$OUT/nothelix-vtest-local-$(uname -s | tr '[:upper:]' '[:lower:]')-$(uname -m).tar.gz" \
    -C "$OUT" "$(basename "$STAGING")"

(cd "$OUT" && shasum -a 256 nothelix-vtest-local-*.tar.gz > SHA256SUMS)

echo "Tarball built at: $OUT"
ls -lh "$OUT"
