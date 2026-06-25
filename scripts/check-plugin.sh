#!/usr/bin/env bash
# Load the nothelix plugin in a real hx binary and fail unless it loads
# clean. Standalone `steel` can't do this — it lacks helix's native
# builtins, so it can't resolve `require-builtin helix/core/*` or check
# `helix.static.*` arities; only the real binary can.
#
# Signal: a sentinel file written by init.scm *after* the require. Steel
# aborts evaluation on any load error (FreeIdentifier, BadSyntax,
# ArityMismatch), so the post-require write never runs and the file stays
# absent. No log scraping, no output parsing — the verdict is the file's
# existence. Helix is a TUI, so `expect` only supplies a PTY and quits it.
set -euo pipefail

command -v hx >/dev/null || { echo "hx not on PATH"; exit 2; }
command -v expect >/dev/null || { echo "expect not on PATH"; exit 2; }

cfg=$(mktemp -d)
sample="$cfg/sample.jl"
sentinel="$cfg/loaded.ok"
trap 'rm -rf "$cfg"' EXIT

mkdir -p "$cfg/helix"
cat > "$cfg/helix/init.scm" <<EOF
(require "nothelix.scm")
(let ((p (open-output-file "$sentinel")))
  (write-string "ok" p)
  (close-output-port p))
EOF
printf 'x = 1\n' > "$sample"

XDG_CONFIG_HOME="$cfg" expect -c "
set timeout 30
spawn hx $sample
sleep 6
send \"\033\"
send \":q!\r\"
expect eof
" >/dev/null 2>&1 || true

if [ -f "$sentinel" ]; then
  echo "plugin loads clean"
else
  echo "PLUGIN LOAD FAILED — init.scm aborted before completing (run hx to see the Steel error)"
  exit 1
fi
