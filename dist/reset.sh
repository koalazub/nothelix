#!/bin/bash
# reset.sh — `nothelix reset` subcommand.
#
# Re-copies the nothelix-managed files from a cached tarball without
# touching user data or init.scm.

nothelix_reset() {
    local reset_lsp=0
    local reset_kernel=0

    while [ $# -gt 0 ]; do
        case "$1" in
            --lsp)    reset_lsp=1 ;;
            --kernel) reset_kernel=1 ;;
            --all)    reset_lsp=1; reset_kernel=1 ;;
            *) echo "nothelix reset: unknown flag: $1" >&2; exit 2 ;;
        esac
        shift
    done

    local cache_dir="$NOTHELIX_SHARE/.cache/extracted"
    if [ ! -d "$cache_dir" ]; then
        echo "nothelix reset: no cached tarball at $cache_dir; run 'nothelix upgrade' instead" >&2
        exit 1
    fi
    if [ ! -x "$cache_dir/install-local.sh" ]; then
        echo "nothelix reset: cache is incomplete ($cache_dir/install-local.sh missing); run 'nothelix upgrade'" >&2
        exit 1
    fi

    echo "nothelix reset"

    # Stash lsp/depot outside the install path before invoking
    # install-local.sh — place_dir does `rm -rf` on the lsp/ target
    # before copying, which would otherwise wipe the precompile cache.
    # We restore it after unless --lsp was passed.
    local stash_dir=""
    if [ -d "$NOTHELIX_SHARE/lsp/depot" ] && [ $reset_lsp -eq 0 ]; then
        stash_dir=$(mktemp -d -t "nothelix-reset-lsp-stash.XXXXXX")
        mv "$NOTHELIX_SHARE/lsp/depot" "$stash_dir/depot"
    elif [ $reset_lsp -eq 1 ] && [ -d "$NOTHELIX_SHARE/lsp/depot" ]; then
        rm -rf "$NOTHELIX_SHARE/lsp/depot"
        echo "  wiped LSP depot at $NOTHELIX_SHARE/lsp/depot"
    fi

    if [ $reset_kernel -eq 1 ]; then
        if [ -d "$NOTHELIX_SHARE/kernel-scripts" ]; then
            rm -rf "$NOTHELIX_SHARE/kernel-scripts"
            echo "  wiped kernel-scripts at $NOTHELIX_SHARE/kernel-scripts"
        fi
    fi

    # Re-run install-local.sh from the cache. This re-copies binaries,
    # dylib, cogs, runtime, demo — everything except init.scm (which
    # the append step skips because grep-then-append is idempotent).
    "$cache_dir/install-local.sh" "$cache_dir"

    # Restore the stashed lsp/depot now that install-local.sh has
    # rebuilt lsp/ from the cache.
    if [ -n "$stash_dir" ] && [ -d "$stash_dir/depot" ]; then
        mkdir -p "$NOTHELIX_SHARE/lsp"
        mv "$stash_dir/depot" "$NOTHELIX_SHARE/lsp/depot"
        rm -rf "$stash_dir"
    fi

    echo "Reset complete."
}
