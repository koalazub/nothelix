# On-device SLM cell summaries — design

## Problem

Picker rows now show each cell's first meaningful line, which helps but can't
say "question 6: null spaces" when the cell is a paragraph. The user wants
model-quality labels — without bundling a model runtime, without burning
battery, and only when explicitly opted in.

## Approach (spike-verified on macOS 27)

Use the OS-provided on-device model: Apple's FoundationModels framework
(macOS 26+, ANE-scheduled). A vendored ~30-line Swift helper
(`tools/nothelix-slm/main.swift`) compiles once with the system `swiftc` and
exposes two modes:

- `--probe` → exit 0 iff `SystemLanguageModel.default.availability == .available`
  (Apple Intelligence enabled). Verified: compiles first-try against the
  CommandLineTools MacOSX27 SDK; probe reports available; live batch produced
  labels like `null space basis, rank-nullity verification`.
- batch mode: cells on stdin separated by `\x1e`, one label line per cell on
  stdout (session instructions pin ≤6-word lowercase labels).

Measured ~7s/cell (includes model warmup) → summaries are NEVER computed on
the picker's critical path.

## Activation — detection AND opt-in (both required)

1. `.nothelix.conf`: `slm-summaries = true` (default false). Display-class
   config: it can never point at an arbitrary executable — the helper is
   compiled from the repo-vendored source only — so no trust gate.
2. Detection at first use, cached per session: helper binary present at
   `~/.local/share/nothelix/bin/nothelix-slm` (compiled lazily via `swiftc`,
   falling back to `DEVELOPER_DIR=/Library/Developer/CommandLineTools` when the
   env's xcrun shim is broken, e.g. under a nix devshell) AND `--probe` exits 0.
   Any failure → feature silently off, heuristic rows remain.

## Data flow

- Refresh (background): on notebook open and after cell execution, when opted
  in + detected, libnothelix spawns a plain `std::thread` (never
  `spawn-native-thread` — GC safepoint freeze) that batches every cell whose
  `djb2(cell text)` has no cache file through ONE helper process and writes
  `~/.local/share/nothelix/summaries/<workspace>/<hash>` files. Changed cells
  only — steady-state energy cost is zero for an untouched notebook.
- Read (picker open, synchronous, instant): row snippet precedence =
  explicit `# comment` marker label → cached SLM summary for the cell's
  current hash → first-meaningful-line heuristic. A stale hash simply misses
  the cache and shows the heuristic until the background pass catches up.

## FFI (libnothelix, v24 → v25)

- `slm-available(workspace) -> "yes"|"no"` (compile+probe, memoized)
- `slm-refresh-summaries(workspace, cells_blob) -> ""` (fire-and-forget;
  `cells_blob` = `\x1e`-separated cell texts)
- `slm-summary-for(workspace, hash) -> String` (cache read, `""` on miss)

## Non-goals

- No bundled runtimes (llama.cpp/SSM/ollama) — the OS model or nothing.
- No summaries for non-macOS / pre-26 / Apple-Intelligence-off machines —
  heuristic rows are the permanent fallback, not a degraded mode.
- No per-keystroke or render-path model calls.

## Testing

- Rust: cache read/write/miss + hash keying with a tempdir seam; detection
  memoization; blob split. Model/probe calls excluded from unit tests
  (environment-dependent) — guarded so absence = "no".
- Live (this machine): compile, probe=available, 5-cell batch produces sane
  labels (done in the spike; repeat post-integration).
