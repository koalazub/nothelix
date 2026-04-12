#!/bin/bash
# uninstall.sh — `nothelix uninstall` subcommand.
#
# Symmetric inverse of install: removes every file this install
# placed, surgically edits init.scm to remove only the nothelix
# require line, leaves ~/.julia/ and (by default) the shared
# ~/.cache/helix/helix.log untouched.

nothelix_uninstall() {
    local keep_data=0
    local dry_run=0
    local assume_yes=0
    local purge=0

    while [ $# -gt 0 ]; do
        case "$1" in
            --keep-data) keep_data=1 ;;
            --dry-run)   dry_run=1 ;;
            --yes|-y)    assume_yes=1 ;;
            --purge)     purge=1 ;;
            *) echo "nothelix uninstall: unknown flag: $1" >&2; exit 2 ;;
        esac
        shift
    done

    local targets=(
        "$NOTHELIX_BIN/hx-nothelix"
        "$NOTHELIX_BIN/nothelix"
        "$NOTHELIX_BIN/julia-lsp"
        "$STEEL_HOME/native/libnothelix.dylib"
        "$STEEL_HOME/native/libnothelix.so"
        "$STEEL_HOME/native/libnothelix.meta"
        "$STEEL_HOME/cogs/nothelix.scm"
        "$STEEL_HOME/cogs/nothelix"
        "$NOTHELIX_SHARE"
    )

    echo "nothelix uninstall plan:"
    for t in "${targets[@]}"; do
        if [ -e "$t" ] || [ -L "$t" ]; then
            echo "  remove  $t"
        fi
    done
    if grep -Fq '(require "nothelix.scm")' "$HOME/.config/helix/init.scm" 2>/dev/null; then
        echo "  modify  $HOME/.config/helix/init.scm (remove nothelix require line)"
    fi
    if [ $purge -eq 1 ] && [ -f "$HOME/.cache/helix/helix.log" ]; then
        echo "  remove  $HOME/.cache/helix/helix.log (purge)"
    fi
    echo ""
    echo "Leaving alone:"
    echo "  ~/.julia/"
    echo "  ~/.config/helix/* (except init.scm edits above)"
    if [ $purge -eq 0 ]; then
        echo "  ~/.cache/helix/helix.log"
    fi
    echo "  ~/.local/bin/hx (your plain Helix, if any)"
    echo ""

    if [ $dry_run -eq 1 ]; then
        echo "Dry run — no files changed."
        return 0
    fi

    if [ $assume_yes -eq 0 ] && [ -t 0 ]; then
        printf "Proceed? (y/N) "
        local reply
        read -r reply
        case "$reply" in
            y|Y|yes|YES) ;;
            *) echo "Aborted."; return 1 ;;
        esac
    fi

    for t in "${targets[@]}"; do
        if [ -d "$t" ] && ! [ -L "$t" ]; then
            if [ "$t" = "$NOTHELIX_SHARE" ] && [ $keep_data -eq 1 ]; then
                find "$NOTHELIX_SHARE" -mindepth 1 -maxdepth 1 \
                    ! -path "$NOTHELIX_SHARE/lsp" \
                    -exec rm -rf {} +
                find "$NOTHELIX_SHARE/lsp" -mindepth 1 -maxdepth 1 \
                    ! -path "$NOTHELIX_SHARE/lsp/depot" \
                    -exec rm -rf {} + 2>/dev/null || true
            else
                rm -rf "$t"
            fi
        elif [ -e "$t" ] || [ -L "$t" ]; then
            rm -f "$t"
        fi
    done

    local init="$HOME/.config/helix/init.scm"
    if [ -f "$init" ]; then
        local tmp="$init.tmp.$$"
        grep -Fv '(require "nothelix.scm")' "$init" > "$tmp" || true
        if ! grep -q '[^[:space:]]' "$tmp"; then
            rm -f "$init" "$tmp"
        else
            mv "$tmp" "$init"
        fi
    fi

    if [ $purge -eq 1 ] && [ -f "$HOME/.cache/helix/helix.log" ]; then
        rm -f "$HOME/.cache/helix/helix.log"
    fi

    echo ""
    echo "nothelix removed."
}
