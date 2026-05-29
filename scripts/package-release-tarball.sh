#!/bin/bash
# package-release-tarball.sh — turns a `nix build`-produced nothelix-tarball
# result into the release artifacts install.sh downloads:
#
#   <out-dir>/nothelix-<version>-<target>.tar.gz
#   <out-dir>/nothelix-<version>-<target>.tar.gz.sha256
#
# The flake derivation bakes its own pname version into the staging dir name
# and the VERSION file, but the release tag is only decided at workflow time —
# so this script renames the staging dir and re-stamps NOTHELIX_VERSION before
# tarring. The .sha256 entry carries the bare filename because install.sh
# verifies with `grep "$(basename tarball)" SHA256SUMS`.
#
# Usage: package-release-tarball.sh <result-path> <version> <target> <out-dir>

set -euo pipefail

RESULT="${1:?usage: package-release-tarball.sh <result-path> <version> <target> <out-dir>}"
VERSION="${2:?missing version (e.g. v0.1.2)}"
TARGET="${3:?missing target (e.g. darwin-arm64)}"
OUT="${4:?missing out-dir}"

src=$(find -H "$RESULT" -maxdepth 1 -type d -name 'nothelix-*' | head -n 1)
if [ -z "$src" ]; then
    echo "package-release-tarball: no nothelix-* staging dir under $RESULT" >&2
    exit 1
fi

staging="nothelix-${VERSION}-${TARGET}"
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT INT TERM

cp -R "$src" "$work/$staging"
# nix store trees are read-only; the VERSION re-stamp and the EXIT trap's
# cleanup both need write bits.
chmod -R u+w "$work/$staging"

ver_file="$work/$staging/VERSION"
# `|| [ -n "$line" ]` keeps a final line that lacks a trailing newline: read
# returns nonzero at EOF even when it has filled $line.
while IFS= read -r line || [ -n "$line" ]; do
    case "$line" in
        NOTHELIX_VERSION=*) printf 'NOTHELIX_VERSION=%s\n' "$VERSION" ;;
        *)                  printf '%s\n' "$line" ;;
    esac
done < "$ver_file" > "$ver_file.new"
mv "$ver_file.new" "$ver_file"

mkdir -p "$OUT"
tarball="${staging}.tar.gz"
tar -czf "$OUT/$tarball" -C "$work" "$staging"

(
    cd "$OUT"
    if command -v shasum >/dev/null 2>&1; then
        shasum -a 256 "$tarball" > "$tarball.sha256"
    else
        sha256sum "$tarball" > "$tarball.sha256"
    fi
)

echo "packaged: $OUT/$tarball"
