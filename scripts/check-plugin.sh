#!/usr/bin/env bash
# Load the nothelix plugin in a real hx binary and fail unless it loads
# clean, then smoke the audio FFI boundary with production-shaped calls.
# Standalone `steel` can't do either — it lacks helix's native builtins,
# and the pure Steel suites never cross the live dylib boundary, which is
# how a steel-side argument-conversion bug shipped invisibly.
#
# Signal: sentinel files written by init.scm. nothelix.scm is a thin lazy
# shim, so the gate calls (nothelix-load) to compile the whole module
# graph, then requires the dylib directly and calls audio-info and
# audio-waveform on a generated PCM16 fixture, asserting non-ERROR
# replies. Steel aborts evaluation on any load or call error, so a
# missing sentinel is the verdict. No log scraping.
set -euo pipefail

command -v hx >/dev/null || { echo "hx not on PATH"; exit 2; }
command -v expect >/dev/null || { echo "expect not on PATH"; exit 2; }
command -v julia >/dev/null || { echo "julia not on PATH"; exit 2; }

repo=$(cd "$(dirname "$0")/.." && pwd)
cfg=$(mktemp -d)
sample="$cfg/sample.jl"
sentinel="$cfg/loaded.ok"
ffi_sentinel="$cfg/ffi.ok"
fixture="$cfg/tiny.wav"
trap 'rm -rf "$cfg"' EXIT

julia --startup-file=no -e "
include(\"$repo/kernel/cell_registry.jl\")
include(\"$repo/kernel/audio.jl\")
using .AudioArtifacts
AudioArtifacts.write_pcm16_wav(\"$fixture\", sin.(2pi .* (1:800) ./ 40), 8000)
"
[ -f "$fixture" ] || { echo "fixture wav was not written"; exit 2; }

mkdir -p "$cfg/helix"
cat > "$cfg/helix/init.scm" <<EOF
(require "nothelix.scm")
(nothelix-load)
(let ((p (open-output-file "$sentinel")))
  (write-string "ok" p)
  (close-output-port p))
(define (fetch name)
  (eval (list '%#maybe-module-get '(#%get-dylib "libnothelix") (list 'quote name))))
(define (assert-ok tag v)
  (when (or (not (string? v)) (= (string-length v) 0)
            (and (>= (string-length v) 6) (equal? (substring v 0 6) "ERROR:")))
    (error (string-append "ffi-smoke " tag " failed: " (if (string? v) v "non-string")))))
(define (assert-fn tag name)
  (define f (fetch name))
  (when (not (procedure? f))
    (error (string-append "ffi-smoke " tag " missing from dylib")))
  f)
(define audio-info-fn (assert-fn "audio-info" 'audio-info))
(define audio-waveform-fn (assert-fn "audio-waveform" 'audio-waveform))
(define djb2-fn (assert-fn "djb2-hash" 'djb2-hash))
(assert-ok "audio-info" (audio-info-fn "$fixture"))
(assert-ok "waveform-none" (audio-waveform-fn "$fixture" 10 2 0 0 0))
(assert-ok "waveform-playhead" (audio-waveform-fn "$fixture" 10 2 4 2 6))
(when (not (equal? (djb2-fn "abc") 193485963))
  (error "ffi-smoke djb2 parity failed"))
(let ((p (open-output-file "$ffi_sentinel")))
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

if [ ! -f "$sentinel" ]; then
  echo "PLUGIN LOAD FAILED — init.scm aborted before completing (run hx to see the Steel error)"
  exit 1
fi
if [ ! -f "$ffi_sentinel" ]; then
  echo "FFI SMOKE FAILED — plugin loads but a live dylib call errored (run hx to see the Steel error)"
  exit 1
fi
echo "plugin loads clean, ffi smoke clean"
