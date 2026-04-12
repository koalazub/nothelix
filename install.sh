#!/bin/sh
# install.sh — curl-sh entry point for nothelix.
#
# Usage:
#   curl -sSL https://raw.githubusercontent.com/koalazub/nothelix/main/install.sh | sh
#   curl -sSL .../install.sh | sh -s -- --upgrade
#   curl -sSL .../install.sh | sh -s -- --uninstall [--purge|--keep-data|--dry-run|--yes]
#
# Env overrides (for local testing):
#   NOTHELIX_RELEASE_URL     — base URL for releases (default: GitHub)
#   NOTHELIX_VERSION_OVERRIDE — pin to a specific version instead of "latest"
#   NOTHELIX_PLATFORM_OVERRIDE — force detected platform (e.g. "darwin-arm64")
#   NOTHELIX_PREFIX          — install prefix (default: $HOME/.local)

set -eu

MODE="install"
EXTRA_FLAGS=""

# ─── Arg parsing ──────────────────────────────────────────────────────
while [ $# -gt 0 ]; do
    case "$1" in
        --upgrade)   MODE="upgrade" ;;
        --uninstall) MODE="uninstall" ;;
        --purge|--keep-data|--dry-run|--yes)
            EXTRA_FLAGS="$EXTRA_FLAGS $1" ;;
        *) echo "install.sh: unknown arg: $1" >&2; exit 2 ;;
    esac
    shift
done

# ─── Platform detection ───────────────────────────────────────────────
detect_platform() {
    if [ -n "${NOTHELIX_PLATFORM_OVERRIDE:-}" ]; then
        printf '%s' "$NOTHELIX_PLATFORM_OVERRIDE"
        return
    fi
    os=$(uname -s)
    arch=$(uname -m)
    case "$os-$arch" in
        Darwin-arm64)        printf 'darwin-arm64' ;;
        Darwin-x86_64)       printf 'darwin-x86_64' ;;
        Linux-x86_64)        printf 'linux-x86_64' ;;
        Linux-aarch64)       printf 'linux-arm64' ;;
        *)                   printf 'unsupported' ;;
    esac
}

PLATFORM="$(detect_platform)"

SUPPORTED_PLATFORMS="darwin-arm64 linux-x86_64"
if ! echo "$SUPPORTED_PLATFORMS" | grep -qw "$PLATFORM"; then
    echo "install.sh: nothelix doesn't ship a binary for '$PLATFORM' yet." >&2
    echo "install.sh: supported: $SUPPORTED_PLATFORMS" >&2
    exit 1
fi

# ─── Mode: uninstall ──────────────────────────────────────────────────
if [ "$MODE" = "uninstall" ]; then
    echo "install.sh: --uninstall not yet implemented" >&2
    echo "install.sh: EXTRA_FLAGS=$EXTRA_FLAGS" >&2
    exit 1
fi

# ─── Resolve release URL ──────────────────────────────────────────────
# For local testing (NOTHELIX_RELEASE_URL = file://...) or a pinned
# version override (NOTHELIX_VERSION_OVERRIDE), use those values
# directly. Otherwise query the GitHub API for the latest release tag
# so the filename (nothelix-<tag>-<platform>.tar.gz) matches what CI
# actually uploaded.
VERSION="${NOTHELIX_VERSION_OVERRIDE:-}"
RELEASE_URL="${NOTHELIX_RELEASE_URL:-}"

if [ -z "$VERSION" ] && [ -z "$RELEASE_URL" ]; then
    api_url="https://api.github.com/repos/koalazub/nothelix/releases/latest"
    if command -v curl >/dev/null 2>&1; then
        latest_tag=$(curl -fsSL "$api_url" 2>/dev/null | grep '"tag_name"' | head -1 | sed 's/.*"tag_name": *"\([^"]*\)".*/\1/')
    elif command -v wget >/dev/null 2>&1; then
        latest_tag=$(wget -qO- "$api_url" 2>/dev/null | grep '"tag_name"' | head -1 | sed 's/.*"tag_name": *"\([^"]*\)".*/\1/')
    else
        echo "install.sh: need curl or wget to resolve latest release" >&2
        exit 1
    fi
    if [ -z "$latest_tag" ]; then
        echo "install.sh: could not resolve latest release tag from $api_url" >&2
        echo "install.sh: set NOTHELIX_VERSION_OVERRIDE=v0.1.0 (or similar) to pin" >&2
        exit 1
    fi
    VERSION="$latest_tag"
fi

if [ -z "$VERSION" ]; then
    VERSION="latest"
fi
if [ -z "$RELEASE_URL" ]; then
    RELEASE_URL="https://github.com/koalazub/nothelix/releases/download/$VERSION"
fi

TARBALL="nothelix-${VERSION}-${PLATFORM}.tar.gz"
TARBALL_URL="$RELEASE_URL/$TARBALL"
SHA_URL="$RELEASE_URL/SHA256SUMS"

# ─── Julia check (non-fatal) ──────────────────────────────────────────
check_julia() {
    if command -v julia >/dev/null 2>&1; then
        julia_version=$(julia --version 2>&1 | head -1)
        printf '  checking julia         ... found (%s)\n' "$julia_version"
    else
        printf '  checking julia         ... NOT FOUND (install with:\n'
        printf '    curl -fsSL https://install.julialang.org | sh\n'
        printf '    then restart your shell)\n'
    fi
}

# ─── Download + verify ────────────────────────────────────────────────
fetch_file() {
    src="$1"
    dst="$2"
    if command -v curl >/dev/null 2>&1; then
        curl -fsSL -o "$dst" "$src"
    elif command -v wget >/dev/null 2>&1; then
        wget -q -O "$dst" "$src"
    else
        echo "install.sh: need curl or wget" >&2
        exit 1
    fi
}

verify_sha() {
    tarball="$1"
    sums_file="$2"
    expected=$(grep "$(basename "$tarball")" "$sums_file" | awk '{print $1}')
    if [ -z "$expected" ]; then
        echo "install.sh: no SHA256 entry for $(basename "$tarball") in SHA256SUMS" >&2
        return 1
    fi
    if command -v shasum >/dev/null 2>&1; then
        actual=$(shasum -a 256 "$tarball" | awk '{print $1}')
    elif command -v sha256sum >/dev/null 2>&1; then
        actual=$(sha256sum "$tarball" | awk '{print $1}')
    else
        echo "install.sh: need shasum or sha256sum" >&2
        return 1
    fi
    if [ "$expected" != "$actual" ]; then
        echo "install.sh: SHA256 mismatch for $(basename "$tarball")" >&2
        echo "  expected: $expected" >&2
        echo "  actual:   $actual" >&2
        return 1
    fi
    return 0
}

# ─── Main ─────────────────────────────────────────────────────────────
echo "nothelix install"
printf '  detected: %s\n' "$PLATFORM"

TMP_DIR=$(mktemp -d)
trap 'rm -rf "$TMP_DIR"' EXIT INT TERM

printf '  fetching: %s\n' "$TARBALL"
fetch_file "$TARBALL_URL" "$TMP_DIR/$TARBALL"
fetch_file "$SHA_URL" "$TMP_DIR/SHA256SUMS"

printf '  verifying SHA256 ... '
if verify_sha "$TMP_DIR/$TARBALL" "$TMP_DIR/SHA256SUMS"; then
    echo "ok"
else
    exit 1
fi

# Extract
tar -xzf "$TMP_DIR/$TARBALL" -C "$TMP_DIR"
# Find the single top-level extracted dir
EXTRACTED_DIR=$(find "$TMP_DIR" -maxdepth 1 -type d -name 'nothelix-*' | head -1)
if [ -z "$EXTRACTED_DIR" ] || [ ! -d "$EXTRACTED_DIR" ]; then
    echo "install.sh: tarball did not contain a nothelix-* directory" >&2
    exit 1
fi

# Run the in-tarball installer
EXTRA_ARGS=""
if [ "$MODE" = "upgrade" ]; then
    EXTRA_ARGS="--upgrade"
fi
"$EXTRACTED_DIR/install-local.sh" "$EXTRACTED_DIR" $EXTRA_ARGS

# Cache the extracted tarball so `nothelix reset` can re-copy files
# without hitting the network. Overwrites any previous cache.
NOTHELIX_SHARE_DIR="${NOTHELIX_SHARE:-${XDG_DATA_HOME:-$HOME/.local/share}/nothelix}"
CACHE_DIR="$NOTHELIX_SHARE_DIR/.cache"
mkdir -p "$CACHE_DIR"
rm -rf "$CACHE_DIR/extracted"
cp -R "$EXTRACTED_DIR" "$CACHE_DIR/extracted"

check_julia

# PATH check (non-fatal)
case ":$PATH:" in
    *":${NOTHELIX_PREFIX:-$HOME/.local}/bin:"*)
        printf '  checking PATH          ... %s/bin is on PATH\n' "${NOTHELIX_PREFIX:-$HOME/.local}" ;;
    *)
        printf '  checking PATH          ... %s/bin NOT on PATH\n' "${NOTHELIX_PREFIX:-$HOME/.local}"
        printf '    add this to your shell profile:\n'
        # shellcheck disable=SC2016  # $PATH is intentionally literal shell code for user to paste
        printf '      export PATH="%s/bin:$PATH"\n' "${NOTHELIX_PREFIX:-$HOME/.local}" ;;
esac

echo ""
echo "Done. Try: nothelix"
