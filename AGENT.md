# Working on nothelix

Instructions for any coding agent or contributor. Tool-agnostic.

## Version control

This is a jj (Jujutsu) colocated repo. Use `jj` for history operations,
never raw `git rebase`/`git stash`. Small mechanical git plumbing (worktree
pruning, `git ls-files`) is fine.

## Layout

- `plugin/` — the Steel plugin. `nothelix.scm` is a thin lazy-load shim;
  the graph lives under `plugin/nothelix/`.
- `libnothelix/` — Rust dylib behind the plugin's FFI.
- `kernel/` — the Julia kernel runner and cell registry.
- `lsp/` — JETLS integration.
- `docs/` — the Jekyll site (just-the-docs). `docs/dev/` is excluded from
  the site and holds internal design notes.
- The editor itself is a Helix fork: `koalazub/helix`, branch
  `feature/inline-image-rendering`.

## Build and verify

- `just check` is the gate: clippy, `cargo nextest run -p libnothelix`,
  then a headless load of the real plugin via `scripts/check-plugin.sh`.
- Use `cargo nextest`, not `cargo test`.
- The nextest suite includes a kernel boot gate that installs the embedded
  kernel scripts into a scratch HOME and boots them with real `julia` to
  the ready marker, so julia must be on PATH to run the tests.
- The justfile is nushell. Keep recipes nushell.
- Rust ↔ Steel FFI versions move in lockstep: `NOTHELIX_FFI_VERSION` in
  `libnothelix/src/lib.rs` must equal `EXPECTED-FFI-VERSION` in
  `plugin/nothelix/ffi-version.scm`. Bump both or neither.
- After changing Rust FFI, the dylib must be rebuilt and installed
  (`just install`) before a running editor sees it. `just install fast`
  builds with release optimization but no LTO for iteration; plain
  `just install` (full LTO) is for real use.
- A running `hx` caches Steel modules and the dylib. Fully restart the
  editor to test changes; do not diagnose against a stale session.
- The dev shell comes from `flake.nix`. The rust-overlay dev shell and the
  fenix release build are split on purpose; do not unify or pin them
  together.

## Code standards

- Clippy passes clean across the codebase. No blanket `#[allow(...)]`
  shortcuts.
- No code comments. Code is self-documenting; name things properly instead.
  Steel `;;@doc` docstrings are the one exception because tooling consumes
  them.
- Errors propagate `Result` with input context. Never `unwrap_or` a
  fallback as error handling; never swallow failures silently.
- Prefer tokenizers and scanners over regex.
- Tests exercise public API behaviour, not private helpers.
- No dead code, no remnants. Work reports, plans, and scratch files never
  belong in the repo; keep them in your session's own scratch space.

## Docs

When a command, keybinding, or behaviour changes, update `docs/commands.md`
and the relevant page in the same change, and mention user-facing additions
in `README.md` where they fit.
