# Fork Update & Plugin Completion — Design

**Date:** 2026-05-24
**Status:** Approved, pending implementation plan

## Goal

Bring nothelix to a state where it can be used end-to-end without tribal
knowledge about which binary is installed, which FFI symbols resolved,
or why an animation isn't playing. Four pillars:

1. Unblock the stale `hx-nothelix` binary so animation + doc-focus +
   viewport events actually fire.
2. Make missing components self-diagnose and self-explain — no more
   silent degradation.
3. Backfill regression coverage for recent fixes (LaTeX scanner, error
   enrichment, cell picker, round-trip integrity).
4. Close three concrete notebook UX gaps (multi-plot, conceal jitter,
   animation fallback visibility) on top of an in-editor notification
   system.

## Why now

Recent sessions have repeatedly hit the same root cause: the installed
fork binary at `/nix/store/.../helix-master/bin/hx` predates the
animation FFI and the `document-focus-{gained,lost}` /
`viewport-changed` events. Every symptom we've debugged from "no
animation plays" to "free-identifier `register-hook!` failure" traces
back to that one fact. The plugin source already has the patches
(`feature/inline-image-rendering` HEAD `f8aad896` in
`~/projects/helix`), and the nixoala flake already pins that commit;
the binary is just stale. Once we get past darwin-rebuild, the
remaining fragility is around test coverage and the lack of any
mechanism to warn the user when their environment is broken in this
exact way.

## Sequencing

The pillars aren't independent — validation requires the rebuild, and
doctor's stale-binary check should land at the same time as the
bandaid removal so the user gets a hard signal instead of silent
breakage.

1. **Land pending nothelix changes** — commit + push the dirty
   scanner/animation/lib work so post-rebuild state is consistent.
2. **Verify nixoala flake pin** — currently `f8aad896` ==
   `origin/feature/inline-image-rendering` HEAD. No bump needed.
3. **Gate: `darwin-rebuild switch`** — user-driven, blocks 4 and 7.
4. **Validate animation + doc-focus + viewport end-to-end** against a
   real .ipynb with an animated GIF; remove the `with-handler`/`eval`
   bandaid in `plugin/nothelix/animation.scm`.
5. **Extend doctor** with `doctor_check_fork_symbols` so the next stale
   binary fails loud with a `darwin-rebuild switch` hint.
6. **Add notification system** — in-editor health check on first focus,
   `:nothelix-status` typable command, one-shot hint cache.
7. **Backfill regression tests** for LaTeX scanner, error enrichment,
   cell picker, round-trip integrity. Re-verify clippy + nextest clean.
8. **Close UX gaps** — multi-plot stress test, conceal jitter audit,
   animation fallback hard-logging.

## Pillar 1 — Animation + doc-focus actually firing

### Validation checklist (post-rebuild)

- `nothelix doctor` reports no missing fork symbols (after Pillar 2
  lands).
- Opening `.ipynb` with an animated GIF: `register-animation!` returns a
  positive engine id, ticks run at the GIF's declared delay.
- Switching to a different buffer: animation pauses within one tick
  (`document-focus-lost` fires).
- Switching back: animation resumes (`document-focus-gained` fires).
- Scrolling cell out of viewport: animation pauses
  (`viewport-changed` fires with `cell-anchor` outside
  `[anchor, visible-end)`).
- Scrolling back into viewport: animation resumes.

### Bandaid removal

In `plugin/nothelix/animation.scm`:

- Drop `try-add-or-replace-animating-raw-content!` wrapper. Call
  `helix.static.add-or-replace-animating-raw-content!` directly.
- Drop `try-register-hook!` wrapper. Use bare `(register-hook! event
  handler)` for the three fork events.
- Drop the lengthy explanatory comments about older binaries — they no
  longer apply.
- Remove `eval` and `with-handler` imports if nothing else needs them
  in this module.

This is a one-way door: a future stale binary will fail loud, which is
the right behavior because Pillar 2's doctor check will catch it
before plugin load.

## Pillar 2 — Doctor stale-binary detection

### Implementation

Add to `dist/doctor/static.sh`:

```bash
doctor_check_fork_symbols() {
    if [ ! -x "$HX_NOTHELIX" ]; then return; fi  # already failed above
    local missing=""
    for sym in \
        "add-or-replace-animating-raw-content" \
        "document-focus-gained" \
        "document-focus-lost" \
        "viewport-changed"; do
        if ! strings "$HX_NOTHELIX" 2>/dev/null | grep -Fq "$sym"; then
            missing="$missing $sym"
        fi
    done
    if [ -n "$missing" ]; then
        _doctor_fail "hx-nothelix predates fork patches — missing:$missing — run 'darwin-rebuild switch' (or rebuild via cargo and copy to ~/.local/bin/hx-nothelix)"
    else
        _doctor_pass "fork patches present in hx-nothelix (animation FFI + focus + viewport events)"
    fi
}
```

Register the check in `run_static_doctor_checks` after
`doctor_check_hx_nothelix`.

### Test

Add to `tests/install/doctor-static.bats`: synthesize a fake
`hx-nothelix` that lacks the symbols, assert doctor fails with the
expected hint.

## Pillar 3 — In-editor notification system

The doctor command is fine for explicit diagnosis, but it doesn't catch
users in the moment. Pillar 3 surfaces problems where the user sees
them: in Helix's status line.

### Components

1. **`nothelix-health-check` FFI** (libnothelix) — runs a subset of the
   doctor static checks that don't require shelling out: dylib
   presence, BUILD_ID match, plugin cogs present, fork symbols
   probable. Returns a structured result `(list (cons 'symbol "fix-hint") ...)`.
2. **Startup health check** (`plugin/nothelix/init` path, wherever
   nothelix.scm wires) — call FFI on plugin load, store result in a
   `*health-issues*` box.
3. **First-focus hint** — register a one-shot hook on the first
   document-focus event (any event will do as a trigger). If
   `*health-issues*` is non-empty, render the first issue via
   `set-status!`: `"⚠ <issue>: <fix>. Run :nothelix-status for more."`
   Set `*health-hint-shown?*` so it doesn't repeat.
4. **`:nothelix-status` typable command** — registers via the typable
   command machinery (mirror how `:nothelix` is wired). When invoked,
   dumps all `*health-issues*` to status or a popup.

### Design notes

- The FFI must be cheap (microseconds). It does not invoke julia, does
  not read the kernel, does not run the network. Just file existence +
  `strings` parsing.
- The first-focus hook is preferred over a startup announcement because
  Helix's startup status line gets clobbered by the first buffer
  message. First-focus gives the user a moment to read.
- We use `set-status!` not a popup because status is non-modal and the
  user can dismiss by typing. Popups would feel heavy for a warning.
- The notification path *cannot* depend on the fork symbols it's
  warning about — `set-status!` is in core helix and predates all our
  patches.

## Pillar 4 — Regression tests

### libnothelix nextest coverage

Add tests in the relevant module files (no new files needed):

**`libnothelix/src/unicode/mod.rs`** — already has
`sum_paired_limits_keep_both_at_normal_size` and
`complex_braced_superscript_keeps_braces_visible`. Add siblings:
- `integral_with_bounds_keeps_limits_normal_size` (`\int_a^b f(x)dx`)
- `prod_with_subscript_super` (`\prod_{i=1}^n a_i`)
- `inline_super_after_concealed_command` (`\alpha^2` should produce `α²`,
  confirming pending_limits doesn't fire when the command isn't a
  big-operator).

**`libnothelix/src/error_format.rs`** — table-driven test array of
`(input_error_json, expected_enrichment_substring)` pairs. One row per
enrichment path: UndefVar, MethodError-named, MethodError-parenthesized,
NotCallable, ParseError-bracket-balance, ParseError-expected-end demoted.

**`libnothelix/src/notebook.rs`** — `round_trip_preserves_outputs_and_attachments`:
- Build a synthetic `.ipynb` with: one code cell containing
  display_data PNG, one markdown cell with an attachment PNG, one code
  cell with execution_count=null, the `using NothelixMacros` preamble
  pattern.
- `convert_to_ipynb` (sidecar dir → .jl). Edit nothing.
- `convert_jl_to_ipynb` back.
- Diff cell-for-cell: source identical, outputs/execution_count
  preserved on unedited cells, attachments preserved.

### Plugin tests

Plugin-side (Steel) regression coverage is deferred. Steel doesn't ship
a unit-test runner I want to bolt on right now; bats tests + manual
smoke remain the loop.

### Pass criteria

- `cargo nextest run --all` clean.
- `cargo clippy --all-targets -- -D warnings` clean.
- `bats tests/install/` clean.

## Pillar 4b — UX gap closure

### Multi-plot stress test

Build a synthetic notebook in `examples/` with:
- One cell with 3 inline plots in sequence.
- One cell with 2 plots inside a nested `let` block.
- One cell with 1 plot inside the `then` branch of `if true ... end`.
- One cell with a plot in implicit-return position.

Run end-to-end. For any cell that renders fewer plots than declared,
add the failing case to `kernel/output_capture.jl`'s test fixtures and
extend `capture_toplevel`'s per-expression plot capture to cover it.

### Conceal jitter audit

Identify all buffer-mutating callsites in `plugin/nothelix/*.scm` that
*don't* already call `schedule-reconceal`. Likely candidates:
- `output-insert.scm` (cell output insertion)
- `selection.scm` paste-like operations
- Any find-replace path

For each, add `(schedule-reconceal 50)` after the mutation. The 50ms
delay matches what `backslash.scm` uses and gives Helix's render path
time to settle before we recompute overlays.

### Animation static-fallback hard-logging

Once Pillar 1's bandaid is removed, `(helix.static.add-or-replace-
animating-raw-content! ...)` returns a defined value. When that value
is falsy (engine error, decoder refused, etc.), surface it via
`set-status!` so the failure is visible. Today's silent swallow hides
real bugs.

## Out of scope

- **No new fork patches.** Existing patches are sufficient.
- **No fork rebase onto newer upstream.** Defer until upstream lands
  something we need.
- **No Steel unit-test runner.** Plugin code stays validated by manual
  smoke + bats integration.
- **No CI.** Local `cargo nextest` + `just install` + `bats` remains
  the loop.
- **No README rewrite.** Architecture doc is current.

## Acceptance criteria

1. `darwin-rebuild switch` completes; `nothelix doctor` reports zero
   failures including the new fork-symbols check.
2. Opening an animated `.ipynb` plays the animation, pauses on
   buffer-switch and viewport-scroll-out, resumes on return.
3. `animation.scm` contains no `with-handler` / `eval` wrappers around
   fork FFIs.
4. Forcing a stale binary (renaming `hx-nothelix` to a stub) makes
   `nothelix doctor` fail with a concrete fix hint.
5. Forcing a missing dylib (`mv ~/.steel/native/libnothelix.dylib`)
   triggers the in-editor first-focus warning + `:nothelix-status`
   command shows the issue.
6. `cargo nextest run --all` + `cargo clippy --all-targets -- -D
   warnings` + `bats tests/install/` all clean.
7. Multi-plot test notebook renders all declared plots.
