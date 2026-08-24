# Agent Instructions

This project uses **bd** (beads) for issue tracking. Run `bd prime` for full workflow context.

> **Architecture in one line:** Issues live in a local Dolt database
> (`.beads/dolt/`); cross-machine sync uses `bd dolt push/pull` (a
> git-compatible protocol), stored under `refs/dolt/data` on your git
> remote — separate from `refs/heads/*` where your code lives.
> `.beads/issues.jsonl` is a passive export, not the wire protocol.
>
> See [sync-concepts](https://github.com/gastownhall/beads/blob/main/docs/core-concepts/sync-concepts.md)
> for the one-screen overview and anti-patterns (don't treat JSONL as the
> source of truth; don't `bd import` during normal operation; don't
> reach for third-party Dolt hosting before trying the default).

## Quick Reference

```bash
bd ready              # Find available work
bd show <id>          # View issue details
bd update <id> --claim  # Claim work atomically
bd close <id>         # Complete work
bd dolt push          # Push beads data to remote
```

## Non-Interactive Shell Commands

**ALWAYS use non-interactive flags** with file operations to avoid hanging on confirmation prompts.

Shell commands like `cp`, `mv`, and `rm` may be aliased to include `-i` (interactive) mode on some systems, causing the agent to hang indefinitely waiting for y/n input.

**Use these forms instead:**
```bash
# Force overwrite without prompting
cp -f source dest           # NOT: cp source dest
mv -f source dest           # NOT: mv source dest
rm -f file                  # NOT: rm file

# For recursive operations
rm -rf directory            # NOT: rm -r directory
cp -rf source dest          # NOT: cp -r source dest
```

**Other commands that may prompt:**
- `scp` - use `-o BatchMode=yes` for non-interactive
- `ssh` - use `-o BatchMode=yes` to fail instead of prompting
- `apt-get` - use `-y` flag
- `brew` - use `HOMEBREW_NO_AUTO_UPDATE=1` env var


---

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
- Enter it with `just dev`, which goes through a profile at `.nix/devshell`.
  That profile is a GC root, so the toolchain survives garbage collection.
  A plain `nix develop` leaves the shell unrooted and the next collection
  deletes the compiler out from under it, surfacing later as `cargo not
  found` in a shell that was working minutes earlier.

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

<!-- BEGIN BEADS INTEGRATION v:1 profile:minimal hash:46cd31e7 -->
## Beads Issue Tracker

This project uses **bd (beads)** for issue tracking. Run `bd prime` to see full workflow context and commands.

### Quick Reference

```bash
bd ready              # Find available work
bd show <id>          # View issue details
bd update <id> --claim  # Claim work
bd close <id>         # Complete work
```

### Rules

- Use `bd` for ALL task tracking — do NOT use TodoWrite, TaskCreate, or markdown TODO lists
- Run `bd prime` for detailed command reference and session close protocol
- Use `bd remember` for persistent knowledge — do NOT use MEMORY.md files

**Architecture in one line:** issues live in a local Dolt DB; sync uses `refs/dolt/data` on your git remote; `.beads/issues.jsonl` is a passive export. See https://github.com/gastownhall/beads/blob/main/docs/core-concepts/sync-concepts.md for details and anti-patterns.

## Agent Context Profiles

The managed Beads block is task-tracking guidance, not permission to override repository, user, or orchestrator instructions.

- **Conservative (default)**: Use `bd` for task tracking. Do not run git commits, git pushes, or Dolt remote sync unless explicitly asked. At handoff, report changed files, validation, and suggested next commands.
- **Minimal**: Keep tool instruction files as pointers to `bd prime`; use the same conservative git policy unless active instructions say otherwise.
- **Team-maintainer**: Only when the repository explicitly opts in, agents may close beads, run quality gates, commit, and push as part of session close. A current "do not commit" or "do not push" instruction still wins.

## Session Completion

This protocol applies when ending a Beads implementation workflow. It is subordinate to explicit user, repository, and orchestrator instructions.

1. **File issues for remaining work** - Create beads for anything that needs follow-up
2. **Run quality gates** (if code changed) - Tests, linters, builds
3. **Update issue status** - Close finished work, update in-progress items
4. **Handle git/sync by active profile**:
   ```bash
   # Conservative/minimal/default: report status and proposed commands; wait for approval.
   git status

   # Team-maintainer opt-in only, unless current instructions forbid it:
   git pull --rebase
   bd dolt push
   git push
   git status
   ```
5. **Hand off** - Summarize changes, validation, issue status, and any blocked sync/commit/push step

**Critical rules:**
- Explicit user or orchestrator instructions override this Beads block.
- Do not commit or push without clear authority from the active profile or the current user request.
- If a required sync or push is blocked, stop and report the exact command and error.
<!-- END BEADS INTEGRATION -->

<!-- BEGIN BEADS CODEX SETUP: generated by bd setup codex -->
## Beads Issue Tracker

Use Beads (`bd`) for durable task tracking in repositories that include it. Use the `beads` skill at `.agents/skills/beads/SKILL.md` (project install) or `~/.agents/skills/beads/SKILL.md` (global install) for Beads workflow guidance, then use the `bd` CLI for issue operations.

### Quick Reference

```bash
bd ready                # Find available work
bd show <id>            # View issue details
bd update <id> --claim  # Claim work
bd close <id>           # Complete work
bd prime                # Refresh Beads context
```

### Rules

- Use `bd` for all task tracking; do not create markdown TODO lists.
- Run `bd prime` when Beads context is missing or stale. Codex 0.129.0+ can load Beads context automatically through native hooks; use `/hooks` to inspect or toggle them.
- Keep persistent project memory in Beads via `bd remember`; do not create ad hoc memory files.

**Architecture in one line:** issues live in a local Dolt DB; sync uses `refs/dolt/data` on your git remote; `.beads/issues.jsonl` is a passive export. See https://github.com/gastownhall/beads/blob/main/docs/core-concepts/sync-concepts.md for details and anti-patterns.
<!-- END BEADS CODEX SETUP -->
