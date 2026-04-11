# nothelix portability: one-line install for research peers

**Status:** design (brainstorm complete, implementation plan pending)
**Date:** 2026-04-11
**Author:** koalazub + Claude

## Context

nothelix is a Jupyter-notebook-style workflow for Helix built on top of a
custom Helix fork (`koalazub/helix@feature/inline-image-rendering`), a Rust
FFI dylib (`libnothelix`), a Steel plugin tree, and a Julia kernel + LSP.
Right now the only install path is "clone the repo, compile the fork,
compile the dylib, run `just install`, set up nix-darwin / home-manager."
That's fine for the author. It's a wall for the target audience.

The target audience is **research peers**: people who already use Helix or
Vim, run Kitty-protocol terminals (Kitty / Ghostty / WezTerm), work mostly
on Apple Silicon MacBooks or x86_64 Linux workstations, and know Julia —
but don't want to install a Rust toolchain, set up codesigning, bump a
nix flake, or understand how `~/.steel` differs from `~/.config/helix`.
Their tolerance for "dev-environment busywork" is low.

The goal of this design is to give that audience a **single curl-pipe-sh
install command** that puts a working nothelix on their machine, opens a
demo notebook on first run, provides a `doctor` subcommand for debugging,
and uninstalls cleanly with no lingering artefacts.

## Goals

1. **One command to install.** The peer types one line, everything is
   placed and ready. No Rust, no cargo, no manual grammar builds, no
   codesign dance.
2. **Cross-platform, initial scope.** macOS arm64 and Linux x86_64 —
   the two configurations that cover the target research audience
   (Apple Silicon MacBooks and x86_64 Linux workstations). Intel Macs
   and Linux arm64 are deliberately deferred: the CI matrix is
   designed so additional targets plug in without changing any other
   part of this design, but we don't pay the CI cost until there's a
   peer asking for them. No Windows (deliberate scope exclusion —
   terminal graphics protocols on Windows are a separate rabbit hole).
3. **Delightful first run.** `nothelix` with no arguments opens a demo
   notebook that renders an inline plot in under 90 seconds on a cold
   machine and confirms the whole stack works.
4. **Graceful uninstall.** `nothelix uninstall` removes every byte this
   project wrote and leaves nothing else touched.
5. **Self-diagnosing.** `nothelix doctor` catches 95% of "doesn't work"
   issues without a round-trip to the author.
6. **Auto-maintained.** The pin to the helix fork updates automatically
   when the fork pushes — no manual SHA bumps.

## Non-goals

- Windows support.
- Notarized macOS binaries (ad-hoc codesign is enough for curl-downloaded
  tarballs that skip Gatekeeper's quarantine check).
- Installing Julia on the peer's behalf — we detect and prompt them to
  run juliaup themselves.
- A browser-based or WASM port. WASM cannot do direct TTY I/O, process
  spawning, or dynamic library loading — all of which nothelix requires.
  The native tarball is the most portable shape for a TUI + kernel +
  dylib tool.
- An interactive installer with prompts. curl-pipe-sh means stdin is the
  pipe, not a TTY. All decisions come from env vars or flags.

## Architecture overview

Three moving parts:

1. **A release pipeline** (GitHub Actions) that produces pre-built
   tarballs for two target triples on every git tag — darwin-arm64
   and linux-x86_64 — and auto-bumps the helix-fork SHA pin on a
   schedule. Additional targets can be added by appending rows to
   the matrix.
2. **An install script** (`install.sh` hosted on `main` in the nothelix
   repo, fetched via curl) that detects OS/arch, downloads the right
   tarball, verifies its checksum, runs the in-tarball installer, and
   wires up `init.scm`.
3. **A wrapper command** (`nothelix`) installed alongside the fork
   binary, which is the interface peers actually learn. It forwards
   unknown args to `hx-nothelix` and exposes subcommands for `upgrade`,
   `uninstall`, `doctor`, `config`, `reset`, and `version`.

## Section 1 — User-visible install flow

```bash
curl -sSL https://raw.githubusercontent.com/koalazub/nothelix/main/install.sh | sh
```

That one command:

1. Detects OS (`darwin`/`linux`) and arch (`arm64`/`x86_64`). Aborts
   with a clear message on unsupported targets.
2. Checks if `julia` is on PATH. If missing, prints the juliaup
   one-liner (`curl -fsSL https://install.julialang.org | sh`) and
   exits with a non-fatal warning — the install still proceeds because
   the LSP bootstrap is lazy. Julia is needed to *use* nothelix, not
   to install it.
3. Fetches the latest release tarball for the detected triple from
   GitHub Releases: `nothelix-vX.Y.Z-<os>-<arch>.tar.gz`.
4. Fetches `SHA256SUMS` from the same release and verifies the
   tarball. Mismatch aborts.
5. Extracts to a temp dir, runs the in-tarball `install-local.sh` to
   place files in their final homes (see Section 2).
6. Grep-then-appends `(require "nothelix.scm")` to
   `~/.config/helix/init.scm`, creating the file if absent.
7. Checks whether `~/.local/bin` is on `$PATH`. If not, prints a
   shell-appropriate export line **without** editing any shell
   profile. Same pattern as rustup when run non-interactively.
8. Prints "Done. Try: `nothelix`" and exits 0.

### Upgrade

```bash
nothelix upgrade               # wrapper subcommand
curl -sSL .../install.sh | sh -s -- --upgrade   # direct
```

Both re-invoke the install script with `--upgrade`. In upgrade mode
the installer skips the Julia check (ran once), skips the init.scm
append (already done, grep-then-append would be a no-op anyway), and
overwrites existing binaries, dylib, cogs, runtime, and demo.
Peer-edited files under the demo path are not preserved by design —
re-installing restores the stock demo.

### Uninstall

```bash
nothelix uninstall             # wrapper subcommand
curl -sSL .../install.sh | sh -s -- --uninstall   # escape hatch
```

Also re-invokes the install script, this time in `--uninstall` mode.
Uninstall is the symmetric inverse of install; see Section 7.

## Section 2 — File layout on the peer's machine

```
~/.local/bin/
├── hx-nothelix              ← the fork binary (copy, not a symlink)
├── nothelix                 ← wrapper script
└── julia-lsp                ← LanguageServer wrapper from lsp/julia-lsp

~/.steel/
├── native/
│   └── libnothelix.{dylib,so}
└── cogs/
    ├── nothelix.scm         ← plugin entry
    └── nothelix/            ← plugin submodules (execution.scm, scaffold.scm, …)

~/.local/share/nothelix/
├── runtime/                 ← Helix runtime (themes, queries, grammars pre-built)
├── examples/
│   └── demo.jl              ← bundled demo notebook
├── lsp/                     ← LSP bootstrap env (Project.toml, Manifest.toml)
│                              ← depot/ populated lazily on first notebook open
├── kernel/                  ← Julia kernel scripts (also extracted lazily,
│                              ← but present here after first cell run)
└── VERSION                  ← plain text: nothelix version + fork SHA

~/.config/helix/
└── init.scm                 ← only this one line appended:
                             ← (require "nothelix.scm")
```

### Decisions baked into this layout

- **Binaries are copies, not symlinks.** macOS codesigns the real file.
  Symlinks into a temp-extract dir would break when the temp dir is
  cleaned up. Matches what `just install` does today.
- **Plugin source lives under `~/.steel/cogs/`, not `~/.config/helix/`.**
  The latter is where config managers (home-manager, stow, chezmoi)
  tend to inject their own per-file symlinks, which creates circular
  symlink loops if we also install there. `~/.steel/cogs` is Steel's
  own territory and config managers leave it alone.
- **`HELIX_RUNTIME` points at `~/.local/share/nothelix/runtime/`** and
  is set by the wrapper script so the peer never needs to know about
  the variable. Plain `hx` (if they have one installed separately) is
  unaffected.
- **Nothing is written to `~/.julia/`.** The LSP depot under
  `~/.local/share/nothelix/lsp/depot/` is isolated, but the LSP
  wrapper stacks `~/.julia` onto `JULIA_DEPOT_PATH` as a read-only
  second entry so the analyser can resolve the packages the user
  actually has installed via `Pkg.add`. See the `fix(lsp)` commit
  `c8d59573` on nothelix `main`.

## Section 3 — CI / release pipeline

Every git tag on nothelix (e.g. `v0.1.0`) triggers a GitHub Actions
workflow with a **three-job** build matrix. Initial scope is
intentionally trimmed to the two platforms that cover the target
research audience — Apple Silicon MacBooks and x86_64 Linux
workstations. Additional targets (Intel Mac, Linux arm64) can be
added by appending rows to this matrix without changing any other
part of the design.

| Runner             | Target triple                    | Artifact                                    |
|--------------------|----------------------------------|---------------------------------------------|
| `macos-14`         | `aarch64-apple-darwin`           | `nothelix-vX.Y.Z-darwin-arm64.tar.gz`       |
| `ubuntu-24.04`     | `x86_64-unknown-linux-gnu`       | `nothelix-vX.Y.Z-linux-x86_64.tar.gz`       |
| any                | — (checksums + metadata)         | `SHA256SUMS`, `install.sh` archived copy    |

Each platform job runs:

1. Checkout `koalazub/nothelix` at the tagged commit.
2. Checkout `koalazub/helix` at the SHA pinned in nothelix's
   top-level `.helix-fork-rev` file. Any mismatch between
   `.helix-fork-rev` and the actual checkout fails the build
   immediately — this is the guard that keeps fork and plugin in
   lockstep.
3. Install Rust (version pinned via `rust-toolchain.toml` in the fork).
4. Build `hx-nothelix` from the fork: `cargo build --release --features
   steel`. `HELIX_DISABLE_AUTO_GRAMMAR_BUILD=0` is the default here
   (CI *wants* grammars built), unlike the dev build where we skip
   grammars for iteration speed.
5. Build `libnothelix` from the nothelix repo: `cargo build --release
   -p libnothelix`.
6. Fetch + compile tree-sitter grammars: `hx-nothelix --grammar fetch
   && hx-nothelix --grammar build`. Output lands under
   `runtime/grammars/*.{so,dylib}`.
7. Ad-hoc codesign on macOS jobs: `codesign --force --sign -
   hx-nothelix` and the dylib. No developer cert required — peers
   download via curl, skipping Gatekeeper's quarantine.
8. Assemble the tarball with this layout inside:
   ```
   nothelix-vX.Y.Z-<os>-<arch>/
   ├── bin/
   │   ├── hx-nothelix
   │   ├── nothelix
   │   └── julia-lsp
   ├── lib/
   │   └── libnothelix.{dylib,so}
   ├── share/nothelix/
   │   ├── runtime/
   │   ├── examples/demo.jl
   │   ├── plugin/            ← Steel cogs
   │   └── lsp/               ← Project.toml, Manifest.toml (NOT
   │                            pre-instantiated — Julia does that
   │                            lazily on the peer's machine)
   ├── install-local.sh       ← in-tarball installer invoked by the
   │                            top-level install.sh
   └── VERSION                ← plain text: nothelix version + helix
                                fork SHA, written by CI
   ```
9. Upload the tarball + SHA256SUMS as release assets.

A parallel `shellcheck` workflow runs on every PR to keep `install.sh`
and `install-local.sh` from rotting.

### Auto-bumping the helix-fork SHA pin

A scheduled workflow (nightly 03:00 UTC, also triggered by pushes to
`koalazub/helix@feature/inline-image-rendering` via repository_dispatch):

1. Resolve the current fork tip with `git ls-remote`.
2. Compare against `.helix-fork-rev` on nothelix `main`. No-op if equal.
3. Open a `bump/helix-fork-<short-sha>` branch. Single commit:
   `chore(deps): bump helix fork to <sha>` with the fork's `<old>..<new>`
   git log in the body — the commit message itself documents what's
   changing upstream.
4. Run the full build matrix on that branch.
5. On green: auto-merge (squash) to `main`. On red: open an issue
   tagged `helix-fork-bump-broken` with the failing job's log snippet
   and the range of fork commits that introduced the break, so a
   `git bisect` is one click away.
6. Releases are NOT cut automatically. Bumps accumulate on `main`
   until the maintainer tags a release. "My CI merged a thing to main
   at 3am" is one level of automation; "and tagged a release" is
   another level that's easier to regret.

## Section 4 — The `nothelix` wrapper script

A bash script installed at `~/.local/bin/nothelix`. Kept simple so
it's easy to debug and small enough to audit in one screenful.

### Subcommands

| Command                              | Behaviour                                                     |
|--------------------------------------|---------------------------------------------------------------|
| `nothelix`                           | Opens `~/.local/share/nothelix/examples/demo.jl`              |
| `nothelix <file> [<file>...]`        | Forwards all args to `hx-nothelix`                            |
| `nothelix upgrade`                   | Re-invokes `install.sh --upgrade`                             |
| `nothelix uninstall [--purge]`       | Re-invokes `install.sh --uninstall` (see Section 7)           |
| `nothelix doctor [--fix] [--smoke]`  | Pre-flight checks (Section 5)                                 |
| `nothelix config [show\|edit\|path]` | Inspect or edit effective configuration (see below)           |
| `nothelix reset [--lsp\|--kernel\|--all]` | Reset runtime state without uninstalling (see below)     |
| `nothelix version`                   | Prints version metadata                                       |
| `nothelix --help` / `-h`             | Usage                                                         |

Unknown flags pass through verbatim so `nothelix -v foo.jl` and
`nothelix +42 notes.md` behave exactly like `hx +42 notes.md`.

### `nothelix config`

Three sub-verbs, all read-only except `edit`:

- **`nothelix config`** or **`nothelix config show`** prints the
  effective configuration to stdout as a flat key=value list. Read
  from the VERSION file + resolved env vars + actual paths on disk,
  not from any separate config file of nothelix's own:

  ```
  $ nothelix config show
  nothelix.version     = v0.2.1
  nothelix.fork_sha    = 89734c72
  nothelix.install_dir = /Users/researcher/.local/share/nothelix
  steel.home           = /Users/researcher/.steel
  steel.native         = /Users/researcher/.steel/native/libnothelix.dylib
  steel.cogs           = /Users/researcher/.steel/cogs/nothelix
  helix.runtime        = /Users/researcher/.local/share/nothelix/runtime
  helix.init_scm       = /Users/researcher/.config/helix/init.scm
  julia.path           = /opt/homebrew/bin/julia
  julia.version        = 1.12.5
  lsp.env              = /Users/researcher/.local/share/nothelix/lsp
  lsp.depot            = /Users/researcher/.local/share/nothelix/lsp/depot
  demo.notebook        = /Users/researcher/.local/share/nothelix/examples/demo.jl
  ```

  This is the copy-paste target for bug reports. Same information as
  `nothelix doctor` but formatted for machine parsing (`|` pipes into
  `grep`, `awk`, etc.) and without the check status.

- **`nothelix config edit`** opens `~/.config/helix/config.toml` in
  `hx-nothelix` for the user to tweak Helix settings. Creates the file
  if absent with a minimal template (a single `theme = "default"`
  line so the file parses). Nothelix itself has no config surface
  outside Helix's config.toml and the init.scm require line, so
  "edit" means "edit Helix's config" — which is the 99% case anyway.

- **`nothelix config path`** prints the absolute path to
  `~/.config/helix/config.toml` and nothing else. Useful for piping
  into other tools: `$(nothelix config path)` returns a path the
  caller can `cat`, `grep`, etc.

### `nothelix reset`

Three flags, additive. Zero-arg default is equivalent to
`--managed-files` (the safest thing): re-copy the binaries, dylib,
plugin cogs, runtime, and demo from the last-installed tarball. The
tarball is cached at `~/.local/share/nothelix/.cache/current.tar.gz`
on every install/upgrade so `reset` doesn't need network access. If
the cache is missing (manual cleanup, older install), `reset` falls
back to re-invoking `install.sh --upgrade`.

| Flag                        | What gets reset                                                           |
|-----------------------------|---------------------------------------------------------------------------|
| `nothelix reset` (default)  | Binaries + dylib + plugin cogs + runtime + demo notebook. Leaves LSP      |
|                             | depot, kernel scripts, init.scm, and all user data untouched. Useful when |
|                             | a plugin file has been accidentally edited and you want the stock copy.   |
| `nothelix reset --lsp`      | Additionally wipes `~/.local/share/nothelix/lsp/depot/` so the LSP        |
|                             | bootstrap re-precompiles from scratch next time. Useful when the Julia    |
|                             | LanguageServer cache gets stuck on a stale package version after a        |
|                             | `Pkg.update()`.                                                           |
| `nothelix reset --kernel`   | Additionally kills any running kernels and removes                        |
|                             | `~/.local/share/nothelix/kernel/` so `libnothelix` re-extracts the        |
|                             | embedded kernel scripts on next cell run. Useful when a kernel process    |
|                             | is wedged and `cancel-cell` can't reach it.                               |
| `nothelix reset --all`      | All three: managed files + LSP depot + kernel. The nuclear option that    |
|                             | stops short of a full uninstall.                                          |

Reset is strictly less destructive than uninstall:

- init.scm is never touched.
- `~/.julia/` is never touched.
- User notebooks are never touched.
- `$PATH` is never modified.
- No confirmation prompt — reset is idempotent and recoverable by
  re-running it.

When in doubt, run `nothelix reset` (no flags) first, then escalate
to `--lsp`, then `--kernel`, then `--all`, then `nothelix uninstall`
only if none of that helps.

### Launch-time environment

On every launch the wrapper sets:

```bash
export HELIX_RUNTIME="$NOTHELIX_SHARE/runtime"
export STEEL_HOME="${STEEL_HOME:-$HOME/.steel}"
```

Then `exec hx-nothelix "$@"` (or the demo path if no args). Using
`exec` hands the terminal off cleanly so job-control signals
(`Ctrl-Z`, `Ctrl-C`) and foreground-PID terminal integrations behave
normally.

### `nothelix version` output

```
nothelix v0.2.1
  helix fork:   koalazub/helix@89734c72 (feature/inline-image-rendering)
  libnothelix:  v0.2.1
  install dir:  ~/.local/share/nothelix
  steel home:   ~/.steel
  julia:        julia 1.12.5 (/opt/homebrew/bin/julia)
```

The version string is read from the `VERSION` file written by CI at
tarball-assembly time, not shelled out to `hx-nothelix --version`. A
`cat` is ~1ms; an `--version` shell-out is ~100ms and has to boot the
Helix runtime.

## Section 5 — `nothelix doctor`

Runs a set of pre-flight checks and prints pass / warn / fail for each.
This is the command peers learn to run first when something is off.

```
$ nothelix doctor
nothelix v0.2.1 environment check
  [✓] hx-nothelix binary at ~/.local/bin/hx-nothelix (89734c72)
  [✓] libnothelix at ~/.steel/native/libnothelix.dylib (codesigned, 2.6 MB)
  [✓] steel engine matches fork (libnothelix=0.6.0 hx-nothelix=0.6.0)
  [✓] plugin cogs at ~/.steel/cogs/nothelix/ (14 files)
  [✓] HELIX_RUNTIME resolves to ~/.local/share/nothelix/runtime
  [✓] grammars: 284 built (~/.local/share/nothelix/runtime/grammars/)
  [✓] ~/.config/helix/init.scm contains (require "nothelix.scm")
  [✓] ~/.local/bin on PATH
  [✓] julia 1.12.5 at /opt/homebrew/bin/julia
  [✓] LSP env instantiated (~/.local/share/nothelix/lsp/Manifest.toml, 9.5 KB)
  [✓] terminal speaks Kitty graphics protocol (APC response OK, width=8×rows)
  [✓] demo notebook at ~/.local/share/nothelix/examples/demo.jl

12 checks passed. Ready to go.

  ↪ run `nothelix doctor --smoke` to additionally spin up a Julia kernel,
    execute a trivial cell, and verify the full pipeline end to end.
```

### Individual checks

| Check             | What it verifies                                                                   |
|-------------------|-------------------------------------------------------------------------------------|
| `hx-nothelix`     | File exists, is executable, `hx-nothelix --version` succeeds.                      |
| `libnothelix`     | File exists at `~/.steel/native/libnothelix.{dylib,so}`. On macOS also runs        |
|                   | `codesign --verify` — a broken signature manifests as a kernel `Killed: 9`         |
|                   | on Steel's first `#%require-dylib`, which is otherwise opaque.                     |
| `build id`        | Compares the `BUILD_ID` written to the tarball's `VERSION` file at CI              |
|                   | assembly time against the same `BUILD_ID` written to a `libnothelix.meta`          |
|                   | sidecar file installed next to the dylib. A match means the dylib, wrapper,       |
|                   | runtime, cogs, and demo all came from the same CI run (and therefore the same     |
|                   | pinned Steel revision, fork SHA, and plugin version). A mismatch means someone     |
|                   | manually swapped a file or mixed installs across releases — fails hard with       |
|                   | `nothelix build mismatch (libnothelix=<id1>, nothelix=<id2>), run nothelix         |
|                   | upgrade`. This is a cleaner implementation of the same intent as a naive "Steel    |
|                   | engine version" check: we don't need an FFI getter because the files the doctor   |
|                   | reads are already on disk, and BUILD_ID catches the real failure mode (mixed      |
|                   | installs) rather than the theoretical one (Steel version drift within a single    |
|                   | CI build, which can't happen because CI builds both binaries in one pipeline).    |
| `plugin cogs`     | `~/.steel/cogs/nothelix.scm` exists, `~/.steel/cogs/nothelix/` has submodules.     |
| `HELIX_RUNTIME`   | Resolves the assumed path; confirms `queries/`, `themes/`, `grammars/` exist.      |
| `grammars`        | Counts `.so`/`.dylib` files under `runtime/grammars/`. Warns if zero.              |
| `init.scm`        | Greps for `(require "nothelix.scm")`. Fails with exact append instructions.        |
| `PATH`            | `~/.local/bin` in `$PATH`. Warns with shell-aware export snippet if missing.       |
| `julia`           | `julia --version` succeeds. Warns on < 1.9. Fails with juliaup hint if missing.    |
| `LSP env`         | `~/.local/share/nothelix/lsp/Manifest.toml` exists and is non-empty. Warns         |
|                   | (not fails) if missing — this is lazy and auto-populates on first notebook open.   |
| `terminal`        | Emits the Kitty graphics capability query directly to the controlling TTY and     |
|                   | reads the response with a 100ms timeout (see below). Authoritative — not a        |
|                   | guess from `$TERM_PROGRAM`, which lies under tmux / screen / ssh passthrough.     |
|                   | Warns (not fails) on negative response — nothelix falls back to the iTerm2        |
|                   | protocol or a text placeholder, so the tool still runs.                            |
| `demo notebook`   | `~/.local/share/nothelix/examples/demo.jl` exists. Warns (not fails) if missing.   |

### The terminal graphics capability query (details)

`nothelix doctor` writes the following APC sequence to `/dev/tty`:

```
\x1b_Gi=1,a=q,s=1,v=1,f=24,t=d,m=0;AAAA\x1b\\
```

- `a=q` — query mode (no image persists after the query)
- `i=1` — client-assigned image id for tracking the response
- `s=1,v=1,f=24` — a single 24-bit pixel, smallest valid payload
- `t=d` — direct transmission (data in-band)
- `m=0` — no continuation
- Payload `AAAA` is the base64 of a single-pixel RGB image

A Kitty-protocol terminal responds with another APC sequence starting
`\x1b_Gi=1;OK`. A terminal that doesn't understand the protocol either
responds with nothing (timeout, hit at 100ms) or echoes the APC back
as literal text (iTerm2 prior to Kitty-mode support). The doctor check
reads with a tiny `select` loop on the TTY fd and interprets the
result as pass / warn / fail:

- Response contains `;OK` → pass (green ✓). Report the terminal name
  from `$TERM_PROGRAM` as a hint but don't rely on it for correctness.
- No response within 100ms → warn (yellow ▲). Suggest installing
  Ghostty / Kitty / WezTerm but don't block. Nothelix has a text
  fallback path in `graphics.scm`.
- Response contains literal `_Gi=1` text (i.e. the terminal echoed
  the APC instead of parsing it) → warn. Same fallback applies.
- Not running on a TTY (e.g. `nothelix doctor` piped into a log or run
  under CI) → inconclusive, skip the check with a `[?]` mark, not a
  failure.

This is the specific query you asked for — no `$TERM_PROGRAM`
heuristics, no hard-coded list of known-good terminal names.
Whatever the peer is running, if it speaks the protocol, doctor
sees it.

### The kernel-start smoke test (`--smoke`)

Opt-in because it takes ~5 seconds and spawns a real Julia process.
The default `nothelix doctor` is static-only (file existence, version
checks, TTY query) and finishes in under a second so peers can run it
liberally. `nothelix doctor --smoke` additionally:

1. Creates a temp kernel dir under `$TMPDIR/nothelix-doctor-<pid>/`.
2. Extracts the embedded kernel scripts into it (same code path
   `libnothelix::kernel::ensure_kernel_scripts` uses at runtime —
   exposed via a new `nothelix-doctor-smoke` subcommand on the dylib
   or a short Rust helper binary shipped in the tarball).
3. Spawns `julia runner.jl <kernel-dir>`.
4. Writes an `input.json` with a trivial cell: `{"command":
   "execute_cell","cell_index":0,"code":"1 + 1"}`.
5. Polls `output.json.done` for up to 10 seconds.
6. Reads the response, verifies `status == "ok"` and the cell result
   contains `output_repr == "2"`.
7. Shuts down the kernel process and removes the temp dir.
8. Reports pass / fail with timing: `[✓] kernel smoke test (cold
   start 4.8s, execute 22ms)`.

Failure modes the smoke test catches:

- Julia not on PATH (already caught by the static `julia` check,
  but smoke confirms end-to-end).
- Julia version too old to run the kernel modules.
- Missing packages in the user's env that the kernel needs.
- Kernel script extraction failure (disk full, permissions, etc.).
- `libnothelix`'s `include_str!` not matching what the running code
  expects (catches the "dylib was built from different kernel source"
  case across the CI / manual install boundary).
- IPC protocol drift between the plugin's Steel code and the kernel's
  Julia code — a trivial cell catches the JSON-shape mismatch class
  of bug before the peer sees it on their first `<space>nr`.

Smoke is separate from static doctor because the static checks
should be safe to run inside an editor (some future
`:nothelix-doctor` helix command could shell out to it), while
spawning Julia should be opt-in and explicit.

Each check produces one of three outcomes: **pass** (green ✓), **warn**
(yellow ▲ with a remediation hint), or **fail** (red ✗ with a clear "run
this to fix" instruction). Warnings do not cause a non-zero exit;
failures do.

### `nothelix doctor --fix`

Optional flag that attempts automatic remediation for the fixable
subset: re-codesign on macOS, re-append the init.scm require line,
re-extract the demo notebook. Does **not** install Julia or modify
`$PATH` — those touch global state and need the peer's informed
consent.

## Section 6 — The demo notebook

`examples/demo.jl` is the file that opens when peers type `nothelix`
with no arguments. Design goal: under 90 seconds from cold install to
"oh, it works, I believe in this" for peers who already know Julia.
Not a tutorial.

```julia
# ═══════════════════════════════════════════════════════════════════════════
# nothelix demo — Jupyter-style notebooks inside Helix
# ═══════════════════════════════════════════════════════════════════════════
#
# Each `@cell N :julia` block below is a code cell. Place your cursor
# inside one and hit <space>nr to execute it. Output lands under
# `# ─── Output ───` as commented lines, so the file stays valid Julia
# at rest.
#
# Keys worth knowing:
#   <space>nr           execute the cell under the cursor
#   <space>nj           picker: jump to any cell by index
#   <space>nn           insert a new cell
#   ]l / [l             next / previous cell
#   :execute-all-cells  run the whole notebook top to bottom
#   :sync-to-ipynb      round-trip this file to a real .ipynb for sharing
#   :w                  save (stays as .jl)
#
# If anything here surprises you: run `nothelix doctor` in a shell.

@cell 0 :julia
# Stdlib only — runs instantly. Confirms execution works and shows how
# `display` output is captured as commented lines below the cell.
using LinearAlgebra
using Statistics

A = [1.0 2.0 3.0;
     4.0 5.0 6.0;
     7.0 8.0 10.0]

display(A)
println("det(A) = ", det(A))
println("rank(A) = ", rank(A))
println("‖A‖ = ", norm(A))

@cell 1 :julia
# This cell triggers Plots precompilation on first run (~60s on a cold
# machine, instant after that). When it finishes you should see a
# rendered chart inline, not a `# [Plot: …]` text placeholder. If you
# see the placeholder, your terminal doesn't speak the Kitty graphics
# protocol — run `nothelix doctor` and check the terminal line.
using Plots

x = range(0, 4π; length = 200)
plot(x,  sin.(x), label = "sin", lw = 2, title = "hello from nothelix")
plot!(x, cos.(x), label = "cos", lw = 2)
plot!(x, sin.(x) .* cos.(x), label = "sin·cos", lw = 2, ls = :dash)

@markdown 2
### What's next?

- Open any `.ipynb` with `nothelix path/to/notebook.ipynb` — it
  auto-converts to `.jl` on open and back to `.ipynb` on
  `:sync-to-ipynb`.
- Create new cells anywhere with `<space>nn`. Pick code or markdown
  from the popup.
- Type `@cell` followed by space on an empty line and the autofill
  picker comes up; type `@md` followed by space and it expands straight
  to a markdown cell.
- Run `:new-notebook` to scaffold a fresh `.jl` notebook.

That's the tour. Delete this file when you're done — it's just a demo,
and `nothelix upgrade` restores it.
```

Design choices:

- **Two code cells + one markdown cell.** More feels like a tutorial;
  less doesn't cover both interesting output classes (commentified
  text + inline plot).
- **First cell is stdlib only.** Zero precompilation, instant feedback.
  A broken dylib fails here in ~2 seconds instead of after a minute
  of Plots cold-start.
- **Second cell tells the peer about the 60-second Plots warm-up** and
  what the graphics-fallback case looks like. Turns silence into a
  clearly-labelled expected delay.
- **Markdown cell covers the next five minutes** (autofill,
  :sync-to-ipynb, :new-notebook) without making them open a README.
- **No `Pkg.add` anywhere.** Researchers who already use Julia have
  Plots in their shared v1.x env. If they don't, the first run errors
  with a readable `ArgumentError: Package Plots not found` that
  `nothelix doctor` can grow a check for later.
- **"Delete this file when you're done"** — avoids the "is this
  important?" hoarding problem. The demo lives in
  `~/.local/share/nothelix/examples/demo.jl` and is restored on every
  upgrade.

## Section 7 — Error handling, edge cases, and uninstall

### Install script failure modes

| Scenario                                         | Behaviour                                                                                        |
|--------------------------------------------------|--------------------------------------------------------------------------------------------------|
| Unsupported OS / arch                            | Abort step 1 with `nothelix doesn't ship a binary for <os>/<arch>. Supported: ...`               |
| Network failure during download                  | Abort with the curl error verbatim + `Check network, proxy, or GitHub rate limit.`               |
| GitHub rate limit                                 | Distinct message: `GitHub rate limit. Export GH_TOKEN=<PAT> and re-run.`                         |
| Tarball SHA256 mismatch                           | Abort with `Download corrupt. Retry; if it keeps failing open an issue.`                         |
| `julia` missing                                   | Non-fatal warning, install continues. Print juliaup install one-liner.                           |
| `~/.local/bin` not writable                       | Abort with `Fix ownership or set NOTHELIX_PREFIX=/path.` (env var supported.)                    |
| `~/.steel/` shadowed by read-only symlink         | Detect via temp-file probe. Abort with specific message about the config manager symlink.       |
| Previous failed install left partial state       | Fresh mode: prompt `Already installed. Re-run with --upgrade or uninstall first.`                |
| Existing init.scm with other Steel content       | Append-only. Non-event — other plugins keep working.                                             |
| macOS `codesign` fails                            | Log stderr, warn, continue. CI signature is still valid; local re-sign is belt-and-braces.      |
| stdin is a pipe                                   | Installer never reads stdin. All decisions from env vars / flags. Never hangs waiting for input. |

### Wrapper runtime failure modes

| Scenario                              | Behaviour                                                                        |
|----------------------------------------|----------------------------------------------------------------------------------|
| `hx-nothelix` missing / corrupted     | `exec` fails; wrapper suggests `nothelix doctor` or `nothelix upgrade`.          |
| `libnothelix` missing                 | Caught by Steel `#%require-dylib` at Helix startup. `doctor` catches it earlier. |
| Demo notebook missing                 | Fall back to empty buffer. Warn on stderr with `nothelix upgrade` suggestion.    |
| `hx-nothelix` panics                  | Panic goes to stderr; terminal returns to shell. Diagnostic message is        |
|                                        | sufficient for an issue paste. (See the `Tree::get` diagnostic work on the    |
|                                        | helix fork, commit `89734c72`.)                                                  |

### First-launch quirks

| Scenario                                            | Behaviour                                                              |
|------------------------------------------------------|------------------------------------------------------------------------|
| LSP env not instantiated on first notebook open      | `ensure_lsp_environment` spawns Julia fire-and-forget. Status line   |
|                                                      | shows `nothelix: LSP bootstrapping...` for 20–60s.                   |
| First kernel start                                   | `ensure_kernel_scripts` extracts embedded `.jl` files, spawns Julia. |
|                                                      | Status shows `Starting Julia kernel...`. ~5s cold.                    |
| Terminal doesn't speak Kitty graphics protocol       | Images fall back to `# [Plot: ...]` text placeholder via graphics.scm.|
|                                                      | Doctor warns. Behaviour is graceful.                                  |

### Happy-path install transcript

```
$ curl -sSL https://raw.githubusercontent.com/koalazub/nothelix/main/install.sh | sh
nothelix install
  detected: darwin arm64
  fetching: nothelix-v0.2.1-darwin-arm64.tar.gz (47 MB)
  verifying SHA256 ... ok
  placing hx-nothelix      -> ~/.local/bin/hx-nothelix
  placing nothelix         -> ~/.local/bin/nothelix
  placing julia-lsp        -> ~/.local/bin/julia-lsp
  placing libnothelix.dylib-> ~/.steel/native/libnothelix.dylib
  placing plugin cogs      -> ~/.steel/cogs/nothelix/
  placing runtime          -> ~/.local/share/nothelix/runtime/
  placing demo notebook    -> ~/.local/share/nothelix/examples/demo.jl
  codesigning hx-nothelix  ... ok
  codesigning libnothelix  ... ok
  checking julia           ... found (julia 1.12.5)
  configuring init.scm     ... added (require "nothelix.scm")
  checking PATH            ... ~/.local/bin is on PATH

Done. Try: nothelix
```

### Graceful uninstall

Symmetric with install, exposed two ways:

```bash
nothelix uninstall
curl -sSL .../install.sh | sh -s -- --uninstall
```

The curl form is the escape hatch for when the `nothelix` wrapper
itself has been removed or corrupted. Both routes re-invoke the
install script in `--uninstall` mode.

#### What gets removed

```
~/.local/bin/hx-nothelix            ← removed
~/.local/bin/nothelix                ← removed
~/.local/bin/julia-lsp               ← removed
~/.steel/native/libnothelix.{dylib,so} ← removed
~/.steel/cogs/nothelix.scm           ← removed
~/.steel/cogs/nothelix/              ← removed recursively
~/.local/share/nothelix/             ← removed recursively
~/.config/helix/init.scm             ← the (require "nothelix.scm") line is
                                       removed; rest of file preserved verbatim.
                                       If the file is left empty after removal,
                                       delete it; otherwise rewrite with the
                                       line gone.
```

#### What stays untouched, always

- `~/.julia/` — the peer's Julia packages and depot. We never wrote
  there on install and we never delete there on uninstall.
- `~/.cache/helix/helix.log`.
- Everything else in `~/.config/helix/` (themes, languages.toml,
  config.toml, etc.).
- Any `.jl` / `.ipynb` files on the peer's filesystem outside
  `~/.local/share/nothelix/examples/`. Their research notebooks are
  never at risk.
- `~/.local/bin/hx` if they have upstream Helix installed separately.
  We only remove `hx-nothelix`, never plain `hx`.

#### Flags

| Flag                              | Effect                                                                 |
|-----------------------------------|------------------------------------------------------------------------|
| `nothelix uninstall`              | Default. Remove everything listed above.                               |
| `nothelix uninstall --keep-data`  | Preserve `~/.local/share/nothelix/lsp/depot/` (the precompiled         |
|                                   | LanguageServer cache) across a reinstall.                              |
| `nothelix uninstall --dry-run`    | List files that would be removed; remove nothing.                      |
| `nothelix uninstall --yes`        | Skip the confirmation prompt. Implicit when stdin is not a TTY.       |
| `nothelix uninstall --purge`      | Everything in the default uninstall PLUS scrub                         |
|                                   | `~/.cache/helix/helix.log` (the shared Helix log file where nothelix's |
|                                   | debug messages end up when `nothelix-debug-on` is set). Targets that   |
|                                   | one file specifically rather than the whole `~/.cache/helix/` dir so   |
|                                   | plain Helix's own cache state (if upstream Helix is also installed)    |
|                                   | is untouched. `--purge` implies `--yes` when run non-interactively     |
|                                   | and still prompts on a TTY unless `--yes` is also passed.              |

#### The confirmation prompt

Only appears when stdin is a real TTY. The plan adapts to any flags
passed — `--purge` adds the `helix.log` scrub line, `--keep-data`
removes the lsp depot line and annotates the `~/.local/share/nothelix/`
entry with `(except lsp/depot/)`, etc.

```
$ nothelix uninstall
nothelix v0.2.1 uninstall plan:
  remove  ~/.local/bin/hx-nothelix
  remove  ~/.local/bin/nothelix
  remove  ~/.local/bin/julia-lsp
  remove  ~/.steel/native/libnothelix.dylib
  remove  ~/.steel/cogs/nothelix.scm
  remove  ~/.steel/cogs/nothelix/        (14 files, 187 KB)
  remove  ~/.local/share/nothelix/        (423 files, 312 MB)
  modify  ~/.config/helix/init.scm       (remove the nothelix require line)

Leaving alone:
  ~/.julia/                               (your Julia packages)
  ~/.config/helix/*                       (except the init.scm line above)
  ~/.cache/helix/helix.log                (shared with plain Helix)
  ~/.local/bin/hx                         (your plain Helix, if any)

Proceed? (y/N)
```

With `--purge` the plan gains one extra line and the log-file entry
moves from "leaving alone" to "remove":

```
  remove  ~/.cache/helix/helix.log        (purge: shared Helix log)
```

Any `--yes` / non-TTY invocation skips the prompt.

#### Post-uninstall verification

After removing, the installer re-runs a subset of the `doctor` checks
in reverse (expecting everything to be absent) and prints:

```
nothelix removed.
Verified clean:
  ~/.local/bin/hx-nothelix      ✓ gone
  ~/.local/bin/nothelix         ✓ gone
  ~/.steel/native/libnothelix.* ✓ gone
  ~/.steel/cogs/nothelix*       ✓ gone
  ~/.local/share/nothelix       ✓ gone
  init.scm nothelix line        ✓ removed
```

Any residual gets flagged with `! still present: <path> — remove
manually` and exits non-zero. Peers learn if something went wrong
instead of finding crumbs a year later.

## Open questions

1. **Does `nothelix version` need to detect a drift between the fork SHA
   it expects and the one currently linked into `libnothelix`?** Could
   be a `doctor` check.
2. **Should the installer drop a tiny man page** at
   `~/.local/share/man/man1/nothelix.1` so `man nothelix` works? Nice
   polish but extra scope; defer until a peer asks.
3. **Release cadence.** No schedule proposed. Tag when there's
   something worth shipping. Revisit if auto-bump churn makes
   "what's the current release" hard to answer.
4. **Future Windows support.** Explicitly out of scope now. If it
   becomes a want, the terminal-graphics-protocol story has to be
   solved first (Windows Terminal is gaining Kitty-protocol support
   but it's not universal, and Helix on Windows has its own rough
   edges).

## Dependencies

- A reliable GitHub Actions runner matrix for macOS + Linux.
- A `.helix-fork-rev` file added to the nothelix repo root.
- The `fix(lsp)` commit `c8d59573` on nothelix `main` (already shipped).
- The `fix(view): report which ViewId blew up in Tree::get` commit
  `89734c72` on `koalazub/helix@feature/inline-image-rendering`
  (already shipped).
- A writable GitHub release process with tag-triggered workflows.
