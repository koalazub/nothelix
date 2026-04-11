#!/bin/bash
# doctor/static.sh — static checks for `nothelix doctor`.
#
# Sourced by dist/nothelix. Expects these vars set by the caller:
#   NOTHELIX_SHARE, STEEL_HOME, HX_NOTHELIX, VERSION_FILE,
#   NOTHELIX_BIN (~/.local/bin), HELIX_RUNTIME
#
# Each check appends to three globals:
#   DOCTOR_CHECKS_OUTPUT   — formatted lines for display
#   DOCTOR_FAIL_COUNT      — number of hard failures
#   DOCTOR_WARN_COUNT      — number of warnings

DOCTOR_CHECKS_OUTPUT=""
DOCTOR_FAIL_COUNT=0
DOCTOR_WARN_COUNT=0

_doctor_pass() {
    DOCTOR_CHECKS_OUTPUT="${DOCTOR_CHECKS_OUTPUT}  [✓] $1
"
}
_doctor_warn() {
    DOCTOR_CHECKS_OUTPUT="${DOCTOR_CHECKS_OUTPUT}  [▲] $1
"
    DOCTOR_WARN_COUNT=$((DOCTOR_WARN_COUNT + 1))
}
_doctor_fail() {
    DOCTOR_CHECKS_OUTPUT="${DOCTOR_CHECKS_OUTPUT}  [✗] $1
"
    DOCTOR_FAIL_COUNT=$((DOCTOR_FAIL_COUNT + 1))
}

doctor_check_hx_nothelix() {
    if [ -x "$HX_NOTHELIX" ]; then
        _doctor_pass "hx-nothelix binary at $HX_NOTHELIX"
    else
        _doctor_fail "hx-nothelix missing at $HX_NOTHELIX — run 'nothelix upgrade'"
    fi
}

doctor_check_libnothelix() {
    local dylib=""
    if [ -f "$STEEL_HOME/native/libnothelix.dylib" ]; then
        dylib="$STEEL_HOME/native/libnothelix.dylib"
    elif [ -f "$STEEL_HOME/native/libnothelix.so" ]; then
        dylib="$STEEL_HOME/native/libnothelix.so"
    fi

    if [ -z "$dylib" ]; then
        _doctor_fail "libnothelix missing from $STEEL_HOME/native — run 'nothelix upgrade'"
        return
    fi

    if [ "$(uname -s)" = "Darwin" ]; then
        if codesign --verify "$dylib" 2>/dev/null; then
            _doctor_pass "libnothelix at $dylib (codesigned)"
        else
            _doctor_warn "libnothelix at $dylib (codesign invalid — run 'nothelix upgrade' to re-sign)"
        fi
    else
        _doctor_pass "libnothelix at $dylib"
    fi
}

doctor_check_build_id() {
    local meta="$STEEL_HOME/native/libnothelix.meta"
    if [ ! -f "$meta" ]; then
        _doctor_fail "libnothelix.meta missing — dylib install is incomplete"
        return
    fi
    if [ ! -f "$VERSION_FILE" ]; then
        _doctor_fail "VERSION file missing — run 'nothelix upgrade'"
        return
    fi
    local meta_id version_id
    meta_id=$(grep '^BUILD_ID=' "$meta" | head -1 | cut -d= -f2)
    version_id=$(grep '^BUILD_ID=' "$VERSION_FILE" | head -1 | cut -d= -f2)
    if [ -z "$meta_id" ] || [ -z "$version_id" ]; then
        _doctor_fail "build id missing from meta or VERSION file"
        return
    fi
    if [ "$meta_id" = "$version_id" ]; then
        _doctor_pass "build id matches (${meta_id})"
    else
        _doctor_fail "build id mismatch: libnothelix=${meta_id} nothelix=${version_id} — run 'nothelix upgrade'"
    fi
}

doctor_check_plugin_cogs() {
    if [ ! -f "$STEEL_HOME/cogs/nothelix.scm" ]; then
        _doctor_fail "plugin cogs missing: $STEEL_HOME/cogs/nothelix.scm not found"
        return
    fi
    if [ ! -d "$STEEL_HOME/cogs/nothelix" ]; then
        _doctor_fail "plugin cogs submodules missing: $STEEL_HOME/cogs/nothelix/"
        return
    fi
    local count
    count=$(find "$STEEL_HOME/cogs/nothelix" -maxdepth 1 -name '*.scm' | wc -l | tr -d ' ')
    _doctor_pass "plugin cogs at $STEEL_HOME/cogs/nothelix/ ($count files)"
}

doctor_check_helix_runtime() {
    if [ ! -d "$HELIX_RUNTIME" ]; then
        _doctor_fail "HELIX_RUNTIME $HELIX_RUNTIME does not exist"
        return
    fi
    if [ ! -d "$HELIX_RUNTIME/queries" ]; then
        _doctor_warn "HELIX_RUNTIME missing queries/ — syntax highlighting will be limited"
        return
    fi
    _doctor_pass "HELIX_RUNTIME resolves to $HELIX_RUNTIME"
}

doctor_check_grammars() {
    local grammars_dir="$HELIX_RUNTIME/grammars"
    if [ ! -d "$grammars_dir" ]; then
        _doctor_warn "grammars dir not found at $grammars_dir"
        return
    fi
    local count
    count=$(find "$grammars_dir" -maxdepth 1 \( -name '*.so' -o -name '*.dylib' \) | wc -l | tr -d ' ')
    if [ "$count" -eq 0 ]; then
        _doctor_warn "grammars: 0 built — syntax highlighting will be limited"
    else
        _doctor_pass "grammars: $count built ($grammars_dir)"
    fi
}

doctor_check_init_scm() {
    local init="$HOME/.config/helix/init.scm"
    if [ ! -f "$init" ]; then
        _doctor_fail "$HOME/.config/helix/init.scm missing — run 'nothelix upgrade'"
        return
    fi
    if grep -Fq '(require "nothelix.scm")' "$init"; then
        _doctor_pass "$HOME/.config/helix/init.scm contains (require \"nothelix.scm\")"
    else
        _doctor_fail "$HOME/.config/helix/init.scm missing the nothelix require line — add: (require \"nothelix.scm\")"
    fi
}

doctor_check_path() {
    case ":$PATH:" in
        *":$NOTHELIX_BIN:"*)
            _doctor_pass "$NOTHELIX_BIN on PATH" ;;
        *)
            _doctor_warn "$NOTHELIX_BIN not on PATH — add: export PATH=\"$NOTHELIX_BIN:\$PATH\"" ;;
    esac
}

doctor_check_julia() {
    if command -v julia >/dev/null 2>&1; then
        local julia_version
        julia_version=$(julia --version 2>&1 | head -1)
        _doctor_pass "julia: $julia_version at $(command -v julia)"
    else
        _doctor_fail "julia not found on PATH — install: curl -fsSL https://install.julialang.org | sh"
    fi
}

doctor_check_lsp_env() {
    local manifest="$NOTHELIX_SHARE/lsp/Manifest.toml"
    if [ -f "$manifest" ] && [ -s "$manifest" ]; then
        _doctor_pass "LSP env instantiated ($manifest, $(wc -c < "$manifest") bytes)"
    else
        _doctor_warn "LSP env not yet instantiated — auto-populates on first .jl open"
    fi
}

doctor_check_demo() {
    local demo="$NOTHELIX_SHARE/examples/demo.jl"
    if [ -f "$demo" ]; then
        _doctor_pass "demo notebook at $demo"
    else
        _doctor_warn "demo notebook missing — 'nothelix' with no args will open an empty buffer"
    fi
}

doctor_check_terminal_graphics() {
    if [ "${NOTHELIX_SKIP_TTY_CHECK:-0}" = "1" ]; then
        _doctor_pass "terminal graphics query skipped (NOTHELIX_SKIP_TTY_CHECK=1)"
        return
    fi

    if [ ! -c /dev/tty ]; then
        _doctor_warn "terminal graphics: not running on a TTY, skipping query"
        return
    fi

    # Emit a Kitty graphics capability query and read the response with
    # a 100ms timeout. The sequence:
    #   \x1b_Ga=q,i=1,s=1,v=1,f=24,t=d,m=0;AAAA\x1b\\
    # asks the terminal to acknowledge it supports the Kitty graphics
    # protocol. A capable terminal responds with `\x1b_Gi=1;OK\x1b\\`.
    local response
    response=$({
        # shellcheck disable=SC1003  # \\ inside single quotes is intentional APC string terminator (ESC \)
        printf '\033_Ga=q,i=1,s=1,v=1,f=24,t=d,m=0;AAAA\033\\' > /dev/tty
        # Read up to 256 bytes or 100ms, whichever comes first.
        # bash's `read -t 0.1` handles the timeout; portable enough on
        # bash 4+.
        # shellcheck disable=SC2162
        IFS= read -r -t 0.1 -n 256 resp < /dev/tty || true
        printf '%s' "$resp"
    } 2>/dev/null)

    case "$response" in
        *";OK"*)
            _doctor_pass "terminal speaks Kitty graphics protocol" ;;
        *"_Gi=1"*|*"AAAA"*)
            _doctor_warn "terminal echoed APC literally — no Kitty graphics support (plots will fall back to text)" ;;
        "")
            _doctor_warn "terminal did not respond to Kitty graphics query within 100ms (plots will fall back to text)" ;;
        *)
            _doctor_warn "terminal response to Kitty graphics query is unexpected: $response" ;;
    esac
}

run_static_doctor_checks() {
    doctor_check_hx_nothelix
    doctor_check_libnothelix
    doctor_check_build_id
    doctor_check_plugin_cogs
    doctor_check_helix_runtime
    doctor_check_grammars
    doctor_check_init_scm
    doctor_check_path
    doctor_check_julia
    doctor_check_lsp_env
    doctor_check_demo
    doctor_check_terminal_graphics
}
