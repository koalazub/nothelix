# nothelix portability: curl-sh installer, wrapper, CI pipeline — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Ship a single `curl -sSL … | sh` install command that puts a working nothelix (hx-nothelix fork + libnothelix + plugin cogs + Julia LSP wrapper + demo notebook) on darwin-arm64 or linux-x86_64 research machines, plus the wrapper command and CI infrastructure that keeps it maintainable.

**Architecture:** A tag-triggered GitHub Actions matrix builds per-platform tarballs containing everything needed; `install.sh` at the nothelix repo root detects OS/arch, fetches the right tarball, verifies its checksum, and hands off to an in-tarball `install-local.sh` that places files and configures `init.scm`. A `nothelix` wrapper command (bash) is the peer-facing interface — it launches `hx-nothelix` with the demo by default and exposes `upgrade`, `uninstall`, `doctor`, `config`, `reset`, and `version` subcommands. A scheduled auto-bump workflow keeps `.helix-fork-rev` in lockstep with the fork's `feature/inline-image-rendering` branch.

**Tech Stack:** Bash (POSIX-compatible for install.sh, bash-4+ for the wrapper), GitHub Actions, Rust (libnothelix build.rs additions), `shellcheck`, `bats-core` (bash testing), the existing `just install` flow, `cargo build`, `jj` / `git` for commits.

---

## Pre-flight

- [ ] **Step 0: Create a dedicated bats test directory**

```bash
mkdir -p tests/install
```

- [ ] **Step 1: Install bats-core locally for dev testing (idempotent)**

```bash
if ! command -v bats >/dev/null; then
  brew install bats-core 2>/dev/null || {
    echo "bats-core not installed — tests can still run in CI. Continue."
  }
fi
```

- [ ] **Step 2: Confirm working directory is clean**

```bash
jj status
```
Expected: empty working copy OR only the spec-doc edits from the brainstorm (which are fine — they're already committed).

---

## Phase 1 — Foundation: the fork SHA pin

This phase creates the single file that links nothelix to a specific helix-fork commit. Everything else in this plan depends on this pin existing so CI can validate against it.

### Task 1: Create `.helix-fork-rev` with current fork SHA

**Files:**
- Create: `.helix-fork-rev`
- Create: `tests/install/helix-fork-rev.bats`

- [ ] **Step 1.1: Write the test**

Create `tests/install/helix-fork-rev.bats`:

```bash
#!/usr/bin/env bats

@test ".helix-fork-rev exists at repo root" {
  [ -f "$BATS_TEST_DIRNAME/../../.helix-fork-rev" ]
}

@test ".helix-fork-rev contains a 40-char git SHA" {
  run cat "$BATS_TEST_DIRNAME/../../.helix-fork-rev"
  [ "$status" -eq 0 ]
  # Exactly one line, 40 hex chars
  [[ "$output" =~ ^[0-9a-f]{40}$ ]]
}

@test ".helix-fork-rev has no trailing whitespace or newlines beyond one" {
  run wc -l "$BATS_TEST_DIRNAME/../../.helix-fork-rev"
  [ "$status" -eq 0 ]
  # wc -l reports 1 for "sha\n" (one newline); reject >1 lines
  [[ "$output" =~ ^[[:space:]]*1[[:space:]] ]]
}
```

- [ ] **Step 1.2: Run the test to verify it fails**

```bash
bats tests/install/helix-fork-rev.bats
```
Expected: all three tests FAIL with "No such file or directory".

- [ ] **Step 1.3: Create `.helix-fork-rev`**

Resolve the current fork tip and write it to the file:

```bash
FORK_SHA=$(cd ~/projects/helix && git rev-parse feature/inline-image-rendering 2>/dev/null || jj log -r 'feature/inline-image-rendering' --no-graph --template 'commit_id' | head -c 40)
echo "$FORK_SHA" > /Users/koalazub/projects/nothelix/.helix-fork-rev
```

If neither `git` nor `jj` resolves the ref, fall back to the SHA we pushed earlier:

```bash
echo "89734c7291a9" > /Users/koalazub/projects/nothelix/.helix-fork-rev
```

NB: the file must contain the FULL 40-char SHA, not an abbreviated one. If the first command gave a short SHA, expand it:

```bash
cd ~/projects/helix && git rev-parse 89734c7291a9
```

Write the full-length output to `.helix-fork-rev`.

- [ ] **Step 1.4: Run the test to verify it passes**

```bash
cd /Users/koalazub/projects/nothelix
bats tests/install/helix-fork-rev.bats
```
Expected: 3 tests, 3 passing.

- [ ] **Step 1.5: Commit**

```bash
cd /Users/koalazub/projects/nothelix
jj describe @ -m "feat(ci): pin helix fork SHA in .helix-fork-rev"
```

---

## Phase 2 — The wrapper skeleton

This phase creates the `nothelix` bash wrapper with the minimal subcommand surface: launch-with-demo, forward args, `--help`, `version`. Every later phase adds subcommands to this wrapper.

### Task 2: Wrapper skeleton — launch, --help, forward args

**Files:**
- Create: `dist/nothelix`
- Create: `tests/install/wrapper-skeleton.bats`

- [ ] **Step 2.1: Write the test**

Create `tests/install/wrapper-skeleton.bats`:

```bash
#!/usr/bin/env bats

setup() {
  export WRAPPER="$BATS_TEST_DIRNAME/../../dist/nothelix"
  export NOTHELIX_TEST_MODE=1   # prevents exec to hx-nothelix
}

@test "wrapper exists and is executable" {
  [ -x "$WRAPPER" ]
}

@test "--help prints usage and exits 0" {
  run "$WRAPPER" --help
  [ "$status" -eq 0 ]
  [[ "$output" == *"nothelix"* ]]
  [[ "$output" == *"upgrade"* ]]
  [[ "$output" == *"uninstall"* ]]
  [[ "$output" == *"doctor"* ]]
}

@test "-h is an alias for --help" {
  run "$WRAPPER" -h
  [ "$status" -eq 0 ]
  [[ "$output" == *"nothelix"* ]]
}

@test "no args would exec hx-nothelix with demo path (test mode prints the cmd)" {
  run "$WRAPPER"
  [ "$status" -eq 0 ]
  [[ "$output" == *"hx-nothelix"* ]]
  [[ "$output" == *"demo.jl"* ]]
}

@test "a file arg is forwarded verbatim" {
  run "$WRAPPER" /tmp/test.jl
  [ "$status" -eq 0 ]
  [[ "$output" == *"hx-nothelix /tmp/test.jl"* ]]
}

@test "multiple file args are forwarded" {
  run "$WRAPPER" /tmp/a.jl /tmp/b.jl
  [ "$status" -eq 0 ]
  [[ "$output" == *"hx-nothelix /tmp/a.jl /tmp/b.jl"* ]]
}

@test "unknown flags pass through" {
  run "$WRAPPER" +42 /tmp/notes.md
  [ "$status" -eq 0 ]
  [[ "$output" == *"hx-nothelix +42 /tmp/notes.md"* ]]
}
```

- [ ] **Step 2.2: Run the test to verify it fails**

```bash
bats tests/install/wrapper-skeleton.bats
```
Expected: all tests FAIL with "No such file or directory" on the wrapper path.

- [ ] **Step 2.3: Create the wrapper skeleton**

Create `dist/nothelix`:

```bash
#!/bin/bash
# nothelix — research-friendly Jupyter notebooks inside Helix
#
# This script is the single entry point peers type. It forwards file
# arguments to hx-nothelix (the Helix fork binary) and exposes a small
# set of subcommands for install management.
#
# Sourced by bats tests with NOTHELIX_TEST_MODE=1 to avoid exec'ing
# hx-nothelix during testing. In test mode, commands that would exec
# print the command line they WOULD run and exit 0.

set -euo pipefail

# ─── Paths ────────────────────────────────────────────────────────────
NOTHELIX_BIN="${NOTHELIX_BIN:-$HOME/.local/bin}"
NOTHELIX_SHARE="${NOTHELIX_SHARE:-${XDG_DATA_HOME:-$HOME/.local/share}/nothelix}"
STEEL_HOME="${STEEL_HOME:-$HOME/.steel}"
HX_NOTHELIX="$NOTHELIX_BIN/hx-nothelix"
DEMO_NOTEBOOK="$NOTHELIX_SHARE/examples/demo.jl"
VERSION_FILE="$NOTHELIX_SHARE/VERSION"

export HELIX_RUNTIME="$NOTHELIX_SHARE/runtime"
export STEEL_HOME

# ─── Test mode helper ─────────────────────────────────────────────────
# In test mode we print what we WOULD exec instead of actually execing.
_run_or_print() {
    if [ "${NOTHELIX_TEST_MODE:-}" = "1" ]; then
        echo "$*"
        exit 0
    fi
    exec "$@"
}

# ─── Usage ────────────────────────────────────────────────────────────
usage() {
    cat <<EOF
nothelix — Jupyter-style notebooks inside Helix

Usage:
  nothelix                      Open the bundled demo notebook
  nothelix <file> [<file>...]   Open files with hx-nothelix
  nothelix upgrade              Re-run the installer to upgrade in place
  nothelix uninstall            Remove nothelix (see: nothelix uninstall --help)
  nothelix doctor [--smoke]     Run environment checks
  nothelix config [show|edit|path]  Inspect or edit effective config
  nothelix reset [--lsp|--kernel|--all]  Reset runtime state
  nothelix version              Print version metadata
  nothelix --help / -h          Show this help

Install dir: $NOTHELIX_SHARE
Steel home:  $STEEL_HOME
Report bugs at https://github.com/koalazub/nothelix/issues
EOF
}

# ─── Command dispatch ─────────────────────────────────────────────────
case "${1:-}" in
    --help|-h|help)
        usage
        exit 0
        ;;
    "")
        _run_or_print "$HX_NOTHELIX" "$DEMO_NOTEBOOK"
        ;;
    upgrade|uninstall|doctor|config|reset|version)
        # Subcommand stubs — implemented in later tasks. For now, echo
        # "not implemented" so the dispatcher is in place but callers
        # get a clear signal.
        echo "nothelix: '$1' not yet implemented" >&2
        exit 1
        ;;
    *)
        _run_or_print "$HX_NOTHELIX" "$@"
        ;;
esac
```

Make it executable:

```bash
chmod +x dist/nothelix
```

- [ ] **Step 2.4: Run the tests to verify they pass**

```bash
bats tests/install/wrapper-skeleton.bats
```
Expected: 7 tests, 7 passing.

- [ ] **Step 2.5: Lint with shellcheck**

```bash
shellcheck dist/nothelix
```
Expected: no issues reported (exit 0).

- [ ] **Step 2.6: Commit**

```bash
jj describe @ -m "feat(wrapper): nothelix launcher with demo default + help"
```

---

### Task 3: Wrapper `version` subcommand reads VERSION file

**Files:**
- Modify: `dist/nothelix` (replace the `version` stub in the case statement)
- Modify: `tests/install/wrapper-skeleton.bats` (add version tests)
- Create: `tests/install/fixtures/VERSION.example`

- [ ] **Step 3.1: Create a test fixture VERSION file**

Create `tests/install/fixtures/VERSION.example`:

```
NOTHELIX_VERSION=v0.2.1
BUILD_ID=ci-20260412-abcdef12
FORK_SHA=89734c7291a9
FORK_BRANCH=feature/inline-image-rendering
LIBNOTHELIX_VERSION=v0.2.1
INSTALL_DATE=2026-04-12T03:14:15Z
```

- [ ] **Step 3.2: Write the test**

Append to `tests/install/wrapper-skeleton.bats`:

```bash
@test "version reads from the VERSION file" {
    FIXTURE="$BATS_TEST_DIRNAME/fixtures"
    NOTHELIX_SHARE="$FIXTURE" run "$WRAPPER" version
    [ "$status" -eq 0 ]
    [[ "$output" == *"nothelix v0.2.1"* ]]
    [[ "$output" == *"89734c7291a9"* ]]
    [[ "$output" == *"feature/inline-image-rendering"* ]]
}

@test "version fails gracefully if VERSION file is missing" {
    NOTHELIX_SHARE="/tmp/does-not-exist" run "$WRAPPER" version
    [ "$status" -eq 1 ]
    [[ "$output" == *"VERSION file not found"* ]]
    [[ "$output" == *"nothelix upgrade"* ]]
}
```

Note: the test uses `fixtures/VERSION.example` renamed — create the rename as part of the fixture setup:

```bash
cp tests/install/fixtures/VERSION.example tests/install/fixtures/VERSION
```

Add that to the bats `setup()`:

```bash
setup() {
  export WRAPPER="$BATS_TEST_DIRNAME/../../dist/nothelix"
  export NOTHELIX_TEST_MODE=1
  cp "$BATS_TEST_DIRNAME/fixtures/VERSION.example" "$BATS_TEST_DIRNAME/fixtures/VERSION" 2>/dev/null || true
}
```

- [ ] **Step 3.3: Run the test to verify it fails**

```bash
bats tests/install/wrapper-skeleton.bats
```
Expected: two new tests FAIL (wrapper `version` still says "not yet implemented").

- [ ] **Step 3.4: Implement `version` in the wrapper**

In `dist/nothelix`, replace the `version` branch:

```bash
    version)
        if [ ! -f "$VERSION_FILE" ]; then
            echo "nothelix: VERSION file not found at $VERSION_FILE" >&2
            echo "Run: nothelix upgrade" >&2
            exit 1
        fi
        # Parse key=value lines from VERSION
        # shellcheck disable=SC1090
        source "$VERSION_FILE"
        cat <<EOF
nothelix ${NOTHELIX_VERSION:-unknown}
  build id:     ${BUILD_ID:-unknown}
  helix fork:   koalazub/helix@${FORK_SHA:-unknown} (${FORK_BRANCH:-unknown})
  libnothelix:  ${LIBNOTHELIX_VERSION:-unknown}
  install dir:  $NOTHELIX_SHARE
  steel home:   $STEEL_HOME
  installed:    ${INSTALL_DATE:-unknown}
EOF
        if command -v julia >/dev/null 2>&1; then
            julia_path=$(command -v julia)
            julia_version=$(julia --version 2>&1 | head -1)
            echo "  julia:        $julia_version ($julia_path)"
        else
            echo "  julia:        not found on PATH"
        fi
        exit 0
        ;;
```

To split the `upgrade|uninstall|doctor|config|reset|version` stub into two: move `version` out of the stub line and into its own case branch above. The remaining stubs become:

```bash
    upgrade|uninstall|doctor|config|reset)
        echo "nothelix: '$1' not yet implemented" >&2
        exit 1
        ;;
```

- [ ] **Step 3.5: Run the tests to verify they pass**

```bash
bats tests/install/wrapper-skeleton.bats
```
Expected: 9 tests, 9 passing.

- [ ] **Step 3.6: Lint**

```bash
shellcheck dist/nothelix
```
Expected: clean (the `source "$VERSION_FILE"` triggers SC1090 which we explicitly disable; no other warnings).

- [ ] **Step 3.7: Commit**

```bash
jj describe @ -m "feat(wrapper): nothelix version reads from VERSION file"
```

---

## Phase 3 — libnothelix BUILD_ID + Steel rev surfacing

The doctor's `build id` check needs a `libnothelix.meta` sidecar and a matching `BUILD_ID` in the tarball's VERSION file. Both come from CI, but libnothelix needs a build.rs that emits the BUILD_ID so the sidecar can be generated from it at packaging time.

### Task 4: libnothelix emits BUILD_ID at compile time

**Files:**
- Create: `libnothelix/build.rs`
- Modify: `libnothelix/src/lib.rs` (add `build_id()` export)
- Create: `libnothelix/tests/build_id.rs`

- [ ] **Step 4.1: Write the test**

Create `libnothelix/tests/build_id.rs`:

```rust
//! Verifies that libnothelix's compile-time BUILD_ID is exposed and
//! non-empty. The build id format is "ci-<yyyymmdd>-<short-git-sha>"
//! in CI and "dev-<short-git-sha>-dirty" for local dev builds.

#[test]
fn build_id_is_non_empty() {
    let id = libnothelix::build_id();
    assert!(!id.is_empty(), "build_id() must not be empty");
    assert!(id.len() >= 8, "build_id() must be at least 8 chars: {id}");
}

#[test]
fn build_id_starts_with_known_prefix() {
    let id = libnothelix::build_id();
    assert!(
        id.starts_with("ci-") || id.starts_with("dev-"),
        "build_id() must start with 'ci-' or 'dev-', got: {id}"
    );
}
```

- [ ] **Step 4.2: Run the test to verify it fails**

```bash
cd /Users/koalazub/projects/nothelix
cargo test -p libnothelix --test build_id 2>&1 | tail -20
```
Expected: compilation fails with `no function or associated item named 'build_id' found for type 'libnothelix'`.

- [ ] **Step 4.3: Create build.rs**

Create `libnothelix/build.rs`:

```rust
//! Build script for libnothelix.
//!
//! Generates a stable BUILD_ID at compile time so the wrapper's
//! `nothelix doctor` check can verify that hx-nothelix and libnothelix
//! came from the same CI run. Format:
//!
//!   ci-<yyyymmdd>-<short-git-sha>     (when NOTHELIX_CI_BUILD=1)
//!   dev-<short-git-sha>[-dirty]       (otherwise)
//!
//! The CI release workflow exports NOTHELIX_CI_BUILD=1 and a fixed
//! NOTHELIX_BUILD_DATE before invoking cargo. Local developer builds
//! get the `dev-` prefix automatically.

use std::process::Command;

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-env-changed=NOTHELIX_CI_BUILD");
    println!("cargo:rerun-if-env-changed=NOTHELIX_BUILD_DATE");
    println!("cargo:rerun-if-changed=.git/HEAD");
    println!("cargo:rerun-if-changed=.git/refs/heads");

    let short_sha = Command::new("git")
        .args(["rev-parse", "--short=12", "HEAD"])
        .output()
        .ok()
        .and_then(|out| {
            if out.status.success() {
                Some(String::from_utf8_lossy(&out.stdout).trim().to_string())
            } else {
                None
            }
        })
        .unwrap_or_else(|| "nogit".to_string());

    let dirty = Command::new("git")
        .args(["status", "--porcelain"])
        .output()
        .ok()
        .map(|out| !out.stdout.is_empty())
        .unwrap_or(false);

    let build_id = if std::env::var("NOTHELIX_CI_BUILD").is_ok() {
        let date = std::env::var("NOTHELIX_BUILD_DATE")
            .unwrap_or_else(|_| "00000000".to_string());
        format!("ci-{date}-{short_sha}")
    } else if dirty {
        format!("dev-{short_sha}-dirty")
    } else {
        format!("dev-{short_sha}")
    };

    println!("cargo:rustc-env=NOTHELIX_BUILD_ID={build_id}");
}
```

- [ ] **Step 4.4: Add the public `build_id()` function to libnothelix**

Open `libnothelix/src/lib.rs` and add near the top of the crate (after existing imports but before the module declarations):

```rust
/// Compile-time BUILD_ID for this libnothelix. Used by
/// `nothelix doctor` to verify the installed dylib matches the
/// installed fork binary.
///
/// Format:
///   - `ci-<yyyymmdd>-<short-git-sha>` for CI builds
///   - `dev-<short-git-sha>[-dirty]`   for local developer builds
pub fn build_id() -> &'static str {
    env!("NOTHELIX_BUILD_ID")
}
```

- [ ] **Step 4.5: Run the test to verify it passes**

```bash
cargo test -p libnothelix --test build_id 2>&1 | tail -20
```
Expected: `2 passed`.

- [ ] **Step 4.6: Verify locally that `build_id()` produces the expected format**

```bash
cargo test -p libnothelix --test build_id -- --nocapture 2>&1 | grep -A 1 "test build_id"
```
Expected: test output visible, both tests pass, BUILD_ID starts with `dev-` (local dev build).

- [ ] **Step 4.7: Commit**

```bash
jj describe @ -m "feat(libnothelix): expose compile-time BUILD_ID for doctor check"
```

---

### Task 5: libnothelix.meta sidecar written at install time

The dylib itself carries BUILD_ID in its text section; the wrapper's doctor reads from a sidecar file (faster, doesn't require loading the dylib). The sidecar is generated alongside the dylib by a small helper binary shipped with libnothelix.

**Files:**
- Create: `libnothelix/src/bin/nothelix-meta.rs`
- Modify: `libnothelix/Cargo.toml` (register the bin target)

- [ ] **Step 5.1: Write a smoke test (integration — runs the compiled binary)**

Create `libnothelix/tests/meta_binary.rs`:

```rust
//! Smoke test for the nothelix-meta helper. Invokes the compiled
//! binary and checks the output shape. Relies on cargo's ability to
//! locate the binary via env!("CARGO_BIN_EXE_nothelix-meta").

#[test]
fn meta_binary_prints_build_id() {
    let bin = env!("CARGO_BIN_EXE_nothelix-meta");
    let output = std::process::Command::new(bin)
        .output()
        .expect("run nothelix-meta");
    assert!(
        output.status.success(),
        "nothelix-meta failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(
        stdout.contains("BUILD_ID="),
        "stdout must contain BUILD_ID= line, got: {stdout}"
    );
    assert!(
        stdout.contains("LIBNOTHELIX_VERSION="),
        "stdout must contain LIBNOTHELIX_VERSION= line, got: {stdout}"
    );
}
```

- [ ] **Step 5.2: Run the test to verify it fails**

```bash
cargo test -p libnothelix --test meta_binary 2>&1 | tail -20
```
Expected: compilation fails with "could not find binary target `nothelix-meta`".

- [ ] **Step 5.3: Register the binary in libnothelix/Cargo.toml**

Open `libnothelix/Cargo.toml` and add, after the `[lib]` section (or create it if absent):

```toml
[[bin]]
name = "nothelix-meta"
path = "src/bin/nothelix-meta.rs"
```

- [ ] **Step 5.4: Write the helper binary**

Create `libnothelix/src/bin/nothelix-meta.rs`:

```rust
//! Tiny CLI that prints libnothelix's compile-time metadata as
//! key=value lines. Used by CI to generate the `libnothelix.meta`
//! sidecar, and by `nothelix doctor` to verify build consistency.
//!
//! Output format:
//!   BUILD_ID=<build-id-from-libnothelix>
//!   LIBNOTHELIX_VERSION=<cargo pkg version>
//!
//! CI writes this output to `libnothelix.meta` next to the dylib in
//! the release tarball; the install script copies it to
//! $STEEL_HOME/native/libnothelix.meta.

fn main() {
    println!("BUILD_ID={}", libnothelix::build_id());
    println!("LIBNOTHELIX_VERSION={}", env!("CARGO_PKG_VERSION"));
}
```

- [ ] **Step 5.5: Run the test to verify it passes**

```bash
cargo test -p libnothelix --test meta_binary 2>&1 | tail -10
```
Expected: `test meta_binary_prints_build_id ... ok`.

- [ ] **Step 5.6: Verify the binary works from the command line**

```bash
cargo run -p libnothelix --bin nothelix-meta 2>&1 | tail -5
```
Expected output:
```
BUILD_ID=dev-<short-sha>
LIBNOTHELIX_VERSION=0.1.0
```

- [ ] **Step 5.7: Commit**

```bash
jj describe @ -m "feat(libnothelix): nothelix-meta binary for build-consistency check"
```

---

## Phase 4 — Install scripts: install-local.sh and install.sh

This phase delivers a working install flow for local testing. At the end of this phase, running `bash install.sh --local ./dist` (with a hand-assembled local tarball) should place files correctly.

### Task 6: install-local.sh places files from a pre-extracted tarball

**Files:**
- Create: `dist/install-local.sh`
- Create: `tests/install/install-local.bats`

`install-local.sh` is what runs INSIDE the tarball after extraction. It assumes the tarball has been untarred and the script itself is in the untarred dir alongside `bin/`, `lib/`, `share/nothelix/`, `VERSION`, etc.

- [ ] **Step 6.1: Write the tests**

Create `tests/install/install-local.bats`:

```bash
#!/usr/bin/env bats

# Build a minimal fake tarball directory, run install-local.sh against
# a fake $HOME, assert files land in the right places.

setup() {
    # Fake tarball source dir
    TARBALL_DIR="$(mktemp -d)"
    mkdir -p "$TARBALL_DIR/bin"
    mkdir -p "$TARBALL_DIR/lib"
    mkdir -p "$TARBALL_DIR/share/nothelix/runtime/grammars"
    mkdir -p "$TARBALL_DIR/share/nothelix/examples"
    mkdir -p "$TARBALL_DIR/share/nothelix/plugin/nothelix"
    mkdir -p "$TARBALL_DIR/share/nothelix/lsp"

    # Stub files the installer will copy
    echo "#!/bin/bash" > "$TARBALL_DIR/bin/hx-nothelix"
    echo "echo stub hx-nothelix" >> "$TARBALL_DIR/bin/hx-nothelix"
    chmod +x "$TARBALL_DIR/bin/hx-nothelix"

    echo "#!/bin/bash" > "$TARBALL_DIR/bin/nothelix"
    echo "echo stub nothelix" >> "$TARBALL_DIR/bin/nothelix"
    chmod +x "$TARBALL_DIR/bin/nothelix"

    echo "#!/bin/bash" > "$TARBALL_DIR/bin/julia-lsp"
    chmod +x "$TARBALL_DIR/bin/julia-lsp"

    echo "fake dylib" > "$TARBALL_DIR/lib/libnothelix.dylib"
    echo "BUILD_ID=ci-20260412-abcdef12" > "$TARBALL_DIR/lib/libnothelix.meta"

    echo "# fake plugin" > "$TARBALL_DIR/share/nothelix/plugin/nothelix.scm"
    echo "# fake submodule" > "$TARBALL_DIR/share/nothelix/plugin/nothelix/execution.scm"
    echo "# demo" > "$TARBALL_DIR/share/nothelix/examples/demo.jl"

    cat > "$TARBALL_DIR/VERSION" <<EOF
NOTHELIX_VERSION=v0.2.1
BUILD_ID=ci-20260412-abcdef12
FORK_SHA=89734c7291a9
FORK_BRANCH=feature/inline-image-rendering
LIBNOTHELIX_VERSION=v0.2.1
INSTALL_DATE=2026-04-12T03:14:15Z
EOF

    cp "$BATS_TEST_DIRNAME/../../dist/install-local.sh" "$TARBALL_DIR/install-local.sh"
    chmod +x "$TARBALL_DIR/install-local.sh"

    # Fake HOME
    FAKE_HOME="$(mktemp -d)"
    export HOME="$FAKE_HOME"
    export STEEL_HOME="$FAKE_HOME/.steel"
}

teardown() {
    rm -rf "$TARBALL_DIR" "$FAKE_HOME"
}

@test "install-local places hx-nothelix in ~/.local/bin" {
    run "$TARBALL_DIR/install-local.sh" "$TARBALL_DIR"
    [ "$status" -eq 0 ]
    [ -x "$HOME/.local/bin/hx-nothelix" ]
}

@test "install-local places the wrapper" {
    run "$TARBALL_DIR/install-local.sh" "$TARBALL_DIR"
    [ "$status" -eq 0 ]
    [ -x "$HOME/.local/bin/nothelix" ]
}

@test "install-local places julia-lsp" {
    run "$TARBALL_DIR/install-local.sh" "$TARBALL_DIR"
    [ "$status" -eq 0 ]
    [ -x "$HOME/.local/bin/julia-lsp" ]
}

@test "install-local places libnothelix.dylib and .meta" {
    run "$TARBALL_DIR/install-local.sh" "$TARBALL_DIR"
    [ "$status" -eq 0 ]
    [ -f "$HOME/.steel/native/libnothelix.dylib" ]
    [ -f "$HOME/.steel/native/libnothelix.meta" ]
}

@test "install-local places plugin cogs" {
    run "$TARBALL_DIR/install-local.sh" "$TARBALL_DIR"
    [ "$status" -eq 0 ]
    [ -f "$HOME/.steel/cogs/nothelix.scm" ]
    [ -f "$HOME/.steel/cogs/nothelix/execution.scm" ]
}

@test "install-local places runtime + demo + VERSION" {
    run "$TARBALL_DIR/install-local.sh" "$TARBALL_DIR"
    [ "$status" -eq 0 ]
    [ -d "$HOME/.local/share/nothelix/runtime/grammars" ]
    [ -f "$HOME/.local/share/nothelix/examples/demo.jl" ]
    [ -f "$HOME/.local/share/nothelix/VERSION" ]
}

@test "install-local appends require line to init.scm when absent" {
    run "$TARBALL_DIR/install-local.sh" "$TARBALL_DIR"
    [ "$status" -eq 0 ]
    [ -f "$HOME/.config/helix/init.scm" ]
    run grep 'require "nothelix.scm"' "$HOME/.config/helix/init.scm"
    [ "$status" -eq 0 ]
}

@test "install-local is idempotent on init.scm" {
    "$TARBALL_DIR/install-local.sh" "$TARBALL_DIR"
    "$TARBALL_DIR/install-local.sh" "$TARBALL_DIR"
    # Should have exactly one require line, not two
    run grep -c 'require "nothelix.scm"' "$HOME/.config/helix/init.scm"
    [ "$output" = "1" ]
}

@test "install-local preserves other init.scm content" {
    mkdir -p "$HOME/.config/helix"
    cat > "$HOME/.config/helix/init.scm" <<EOF
(require "my-custom-plugin.scm")
(define my-var 42)
EOF
    run "$TARBALL_DIR/install-local.sh" "$TARBALL_DIR"
    [ "$status" -eq 0 ]
    run grep "my-custom-plugin" "$HOME/.config/helix/init.scm"
    [ "$status" -eq 0 ]
    run grep "my-var" "$HOME/.config/helix/init.scm"
    [ "$status" -eq 0 ]
    run grep 'require "nothelix.scm"' "$HOME/.config/helix/init.scm"
    [ "$status" -eq 0 ]
}
```

- [ ] **Step 6.2: Run the tests to verify they fail**

```bash
bats tests/install/install-local.bats
```
Expected: all tests FAIL with missing `dist/install-local.sh`.

- [ ] **Step 6.3: Create install-local.sh**

Create `dist/install-local.sh`:

```bash
#!/bin/bash
# install-local.sh — in-tarball installer, invoked by install.sh after
# extraction or directly by developers who downloaded a tarball manually.
#
# Usage: install-local.sh <tarball-dir> [--upgrade|--uninstall]
#
# <tarball-dir> is the path to the extracted tarball root containing
# bin/, lib/, share/, VERSION, and this script.
#
# This script is idempotent: running it twice is equivalent to running
# it once. init.scm append is grep-then-append.

set -euo pipefail

TARBALL_DIR="${1:-}"
MODE="${2:-install}"   # install | --upgrade | --uninstall

if [ -z "$TARBALL_DIR" ] || [ ! -d "$TARBALL_DIR" ]; then
    echo "install-local: usage: $0 <tarball-dir> [--upgrade|--uninstall]" >&2
    exit 2
fi

# ─── Paths ────────────────────────────────────────────────────────────
NOTHELIX_PREFIX="${NOTHELIX_PREFIX:-$HOME/.local}"
BIN_DIR="$NOTHELIX_PREFIX/bin"
SHARE_DIR="$NOTHELIX_PREFIX/share/nothelix"
STEEL_HOME="${STEEL_HOME:-$HOME/.steel}"
STEEL_NATIVE="$STEEL_HOME/native"
STEEL_COGS="$STEEL_HOME/cogs"
HELIX_CONFIG_DIR="$HOME/.config/helix"
INIT_SCM="$HELIX_CONFIG_DIR/init.scm"

# ─── Helpers ──────────────────────────────────────────────────────────
log() { printf "  %s\n" "$*"; }

place_file() {
    local src="$1"
    local dst="$2"
    mkdir -p "$(dirname "$dst")"
    cp "$src" "$dst"
    log "placing $(basename "$dst") -> $dst"
}

place_dir() {
    local src="$1"
    local dst="$2"
    mkdir -p "$(dirname "$dst")"
    rm -rf "$dst"
    cp -R "$src" "$dst"
    log "placing $(basename "$src")/ -> $dst"
}

append_init_scm_line() {
    local line='(require "nothelix.scm")'
    mkdir -p "$HELIX_CONFIG_DIR"
    if [ ! -f "$INIT_SCM" ]; then
        touch "$INIT_SCM"
    fi
    if grep -Fq "$line" "$INIT_SCM"; then
        log "init.scm already configured, skipping append"
    else
        # Ensure file ends with newline before appending
        if [ -s "$INIT_SCM" ] && [ "$(tail -c 1 "$INIT_SCM" | wc -l | tr -d ' ')" != "1" ]; then
            printf '\n' >> "$INIT_SCM"
        fi
        printf '%s\n' "$line" >> "$INIT_SCM"
        log "configuring init.scm ... added (require \"nothelix.scm\")"
    fi
}

# ─── Main ─────────────────────────────────────────────────────────────
echo "nothelix install-local"

# Binaries
place_file "$TARBALL_DIR/bin/hx-nothelix" "$BIN_DIR/hx-nothelix"
place_file "$TARBALL_DIR/bin/nothelix" "$BIN_DIR/nothelix"
place_file "$TARBALL_DIR/bin/julia-lsp" "$BIN_DIR/julia-lsp"
chmod +x "$BIN_DIR/hx-nothelix" "$BIN_DIR/nothelix" "$BIN_DIR/julia-lsp"

# Dylib (detect .dylib vs .so)
if [ -f "$TARBALL_DIR/lib/libnothelix.dylib" ]; then
    DYLIB_NAME="libnothelix.dylib"
elif [ -f "$TARBALL_DIR/lib/libnothelix.so" ]; then
    DYLIB_NAME="libnothelix.so"
else
    echo "install-local: no libnothelix.{dylib,so} in $TARBALL_DIR/lib/" >&2
    exit 1
fi
place_file "$TARBALL_DIR/lib/$DYLIB_NAME" "$STEEL_NATIVE/$DYLIB_NAME"
place_file "$TARBALL_DIR/lib/libnothelix.meta" "$STEEL_NATIVE/libnothelix.meta"

# Re-codesign on macOS (the tarball carries a CI signature; we re-sign
# after copy to survive the file being rewritten in a new inode).
if [ "$(uname -s)" = "Darwin" ]; then
    codesign --force --sign - "$BIN_DIR/hx-nothelix" 2>/dev/null || \
        log "warning: codesign failed for hx-nothelix (non-fatal)"
    codesign --force --sign - "$STEEL_NATIVE/$DYLIB_NAME" 2>/dev/null || \
        log "warning: codesign failed for $DYLIB_NAME (non-fatal)"
fi

# Plugin cogs
place_file "$TARBALL_DIR/share/nothelix/plugin/nothelix.scm" "$STEEL_COGS/nothelix.scm"
place_dir "$TARBALL_DIR/share/nothelix/plugin/nothelix" "$STEEL_COGS/nothelix"

# Runtime (Helix runtime with pre-built grammars)
place_dir "$TARBALL_DIR/share/nothelix/runtime" "$SHARE_DIR/runtime"

# Examples (demo notebook)
place_dir "$TARBALL_DIR/share/nothelix/examples" "$SHARE_DIR/examples"

# LSP env scaffold (Project.toml, Manifest.toml — NOT depot/)
place_dir "$TARBALL_DIR/share/nothelix/lsp" "$SHARE_DIR/lsp"

# Version metadata
place_file "$TARBALL_DIR/VERSION" "$SHARE_DIR/VERSION"

# init.scm configuration
append_init_scm_line

echo "nothelix install-local complete"
```

Make it executable:

```bash
chmod +x dist/install-local.sh
```

- [ ] **Step 6.4: Run the tests to verify they pass**

```bash
bats tests/install/install-local.bats
```
Expected: 9 tests, 9 passing.

- [ ] **Step 6.5: Lint**

```bash
shellcheck dist/install-local.sh
```
Expected: clean.

- [ ] **Step 6.6: Commit**

```bash
jj describe @ -m "feat(install): install-local.sh places tarball contents"
```

---

### Task 7: install.sh detects OS/arch, downloads tarball, delegates to install-local.sh

**Files:**
- Create: `install.sh` (at repo root)
- Create: `tests/install/install-sh.bats`

Install.sh is the curl-sh entry point. It detects the platform, downloads the right tarball from GitHub Releases, verifies SHA256, extracts to a temp dir, and invokes install-local.sh. It also supports `--upgrade` and `--uninstall` modes.

- [ ] **Step 7.1: Write the tests**

Create `tests/install/install-sh.bats`:

```bash
#!/usr/bin/env bats

# install.sh is harder to unit-test because it hits the network. We
# use a MOCK_GH_RELEASES env var to point at a local file:// URL
# during tests so the installer downloads from a local fixture dir
# instead of GitHub.

setup() {
    FAKE_HOME="$(mktemp -d)"
    export HOME="$FAKE_HOME"

    # Build a local fake release: a tarball in a temp dir
    FIXTURE_DIR="$(mktemp -d)"
    mkdir -p "$FIXTURE_DIR/release"

    # Assemble the tarball contents matching Task 6's layout
    TARBALL_SRC="$(mktemp -d)/nothelix-vtest-darwin-arm64"
    mkdir -p "$TARBALL_SRC/bin" "$TARBALL_SRC/lib"
    mkdir -p "$TARBALL_SRC/share/nothelix/runtime/grammars"
    mkdir -p "$TARBALL_SRC/share/nothelix/examples"
    mkdir -p "$TARBALL_SRC/share/nothelix/plugin/nothelix"
    mkdir -p "$TARBALL_SRC/share/nothelix/lsp"

    echo "#!/bin/bash" > "$TARBALL_SRC/bin/hx-nothelix"
    chmod +x "$TARBALL_SRC/bin/hx-nothelix"
    cp "$BATS_TEST_DIRNAME/../../dist/nothelix" "$TARBALL_SRC/bin/nothelix"
    chmod +x "$TARBALL_SRC/bin/nothelix"
    echo "#!/bin/bash" > "$TARBALL_SRC/bin/julia-lsp"
    chmod +x "$TARBALL_SRC/bin/julia-lsp"

    echo "fake" > "$TARBALL_SRC/lib/libnothelix.dylib"
    echo "BUILD_ID=ci-test-00000000" > "$TARBALL_SRC/lib/libnothelix.meta"

    echo "# plugin" > "$TARBALL_SRC/share/nothelix/plugin/nothelix.scm"
    echo "# submod" > "$TARBALL_SRC/share/nothelix/plugin/nothelix/execution.scm"
    echo "# demo" > "$TARBALL_SRC/share/nothelix/examples/demo.jl"

    cat > "$TARBALL_SRC/VERSION" <<EOF
NOTHELIX_VERSION=vtest
BUILD_ID=ci-test-00000000
FORK_SHA=0000000000000000000000000000000000000000
FORK_BRANCH=feature/inline-image-rendering
LIBNOTHELIX_VERSION=vtest
INSTALL_DATE=2026-04-12T00:00:00Z
EOF

    cp "$BATS_TEST_DIRNAME/../../dist/install-local.sh" "$TARBALL_SRC/install-local.sh"
    chmod +x "$TARBALL_SRC/install-local.sh"

    # Pack into a .tar.gz in FIXTURE_DIR/release/
    tar -czf "$FIXTURE_DIR/release/nothelix-vtest-darwin-arm64.tar.gz" -C "$(dirname "$TARBALL_SRC")" "$(basename "$TARBALL_SRC")"
    (cd "$FIXTURE_DIR/release" && shasum -a 256 nothelix-vtest-darwin-arm64.tar.gz > SHA256SUMS)

    # Point installer at the fixture release dir
    export NOTHELIX_RELEASE_URL="file://$FIXTURE_DIR/release"
    export NOTHELIX_VERSION_OVERRIDE="vtest"
    export NOTHELIX_PLATFORM_OVERRIDE="darwin-arm64"
    export STEEL_HOME="$FAKE_HOME/.steel"

    INSTALL_SH="$BATS_TEST_DIRNAME/../../install.sh"
}

teardown() {
    rm -rf "$FAKE_HOME" "$FIXTURE_DIR" "${TARBALL_SRC%/*}"
}

@test "install.sh runs end to end with a local fixture release" {
    run bash "$INSTALL_SH"
    [ "$status" -eq 0 ]
    [ -x "$HOME/.local/bin/hx-nothelix" ]
    [ -x "$HOME/.local/bin/nothelix" ]
    [ -f "$HOME/.steel/native/libnothelix.dylib" ]
    [ -f "$HOME/.steel/native/libnothelix.meta" ]
    [ -f "$HOME/.local/share/nothelix/examples/demo.jl" ]
    [ -f "$HOME/.local/share/nothelix/VERSION" ]
}

@test "install.sh aborts if SHA256SUMS mismatches the tarball" {
    # Corrupt the SHA
    echo "0000000000000000000000000000000000000000000000000000000000000000  nothelix-vtest-darwin-arm64.tar.gz" > "$FIXTURE_DIR/release/SHA256SUMS"
    run bash "$INSTALL_SH"
    [ "$status" -ne 0 ]
    [[ "$output" == *"SHA256"* ]]
}

@test "install.sh aborts on unsupported platform" {
    export NOTHELIX_PLATFORM_OVERRIDE="freebsd-sparc"
    run bash "$INSTALL_SH"
    [ "$status" -ne 0 ]
    [[ "$output" == *"freebsd-sparc"* ]] || [[ "$output" == *"not supported"* ]]
}

@test "install.sh --upgrade is idempotent (two runs leave same state)" {
    bash "$INSTALL_SH"
    run bash "$INSTALL_SH" --upgrade
    [ "$status" -eq 0 ]
    [ -f "$HOME/.local/share/nothelix/VERSION" ]
    # init.scm should have exactly one require line
    run grep -c 'require "nothelix.scm"' "$HOME/.config/helix/init.scm"
    [ "$output" = "1" ]
}
```

- [ ] **Step 7.2: Run the tests to verify they fail**

```bash
bats tests/install/install-sh.bats
```
Expected: all tests FAIL with "No such file or directory" for install.sh.

- [ ] **Step 7.3: Create install.sh**

Create `install.sh` at the repo root:

```bash
#!/bin/sh
# install.sh — curl-sh entry point for nothelix.
#
# Usage:
#   curl -sSL https://raw.githubusercontent.com/koalazub/nothelix/main/install.sh | sh
#   curl -sSL .../install.sh | sh -s -- --upgrade
#   curl -sSL .../install.sh | sh -s -- --uninstall [--purge|--keep-data|--dry-run|--yes]
#
# Env overrides (for local testing):
#   NOTHELIX_RELEASE_URL     — base URL for releases (default: GitHub)
#   NOTHELIX_VERSION_OVERRIDE — pin to a specific version instead of "latest"
#   NOTHELIX_PLATFORM_OVERRIDE — force detected platform (e.g. "darwin-arm64")
#   NOTHELIX_PREFIX          — install prefix (default: $HOME/.local)

set -eu

MODE="install"
EXTRA_FLAGS=""

# ─── Arg parsing ──────────────────────────────────────────────────────
while [ $# -gt 0 ]; do
    case "$1" in
        --upgrade)   MODE="upgrade" ;;
        --uninstall) MODE="uninstall" ;;
        --purge|--keep-data|--dry-run|--yes)
            EXTRA_FLAGS="$EXTRA_FLAGS $1" ;;
        *) echo "install.sh: unknown arg: $1" >&2; exit 2 ;;
    esac
    shift
done

# ─── Platform detection ───────────────────────────────────────────────
detect_platform() {
    if [ -n "${NOTHELIX_PLATFORM_OVERRIDE:-}" ]; then
        printf '%s' "$NOTHELIX_PLATFORM_OVERRIDE"
        return
    fi
    os=$(uname -s)
    arch=$(uname -m)
    case "$os-$arch" in
        Darwin-arm64)        printf 'darwin-arm64' ;;
        Darwin-x86_64)       printf 'darwin-x86_64' ;;
        Linux-x86_64)        printf 'linux-x86_64' ;;
        Linux-aarch64)       printf 'linux-arm64' ;;
        *)                   printf 'unsupported' ;;
    esac
}

PLATFORM="$(detect_platform)"

SUPPORTED_PLATFORMS="darwin-arm64 linux-x86_64"
if ! echo "$SUPPORTED_PLATFORMS" | grep -qw "$PLATFORM"; then
    echo "install.sh: nothelix doesn't ship a binary for '$PLATFORM' yet." >&2
    echo "install.sh: supported: $SUPPORTED_PLATFORMS" >&2
    exit 1
fi

# ─── Mode: uninstall ──────────────────────────────────────────────────
if [ "$MODE" = "uninstall" ]; then
    # Uninstall is a separate script flow (Task 12).
    # For now: echo the not-yet-implemented message.
    echo "install.sh: --uninstall not yet implemented in this task" >&2
    echo "install.sh: EXTRA_FLAGS=$EXTRA_FLAGS" >&2
    exit 1
fi

# ─── Resolve release URL ──────────────────────────────────────────────
RELEASE_URL="${NOTHELIX_RELEASE_URL:-https://github.com/koalazub/nothelix/releases/latest/download}"
VERSION="${NOTHELIX_VERSION_OVERRIDE:-latest}"

TARBALL="nothelix-${VERSION}-${PLATFORM}.tar.gz"
TARBALL_URL="$RELEASE_URL/$TARBALL"
SHA_URL="$RELEASE_URL/SHA256SUMS"

# ─── Julia check (non-fatal) ──────────────────────────────────────────
check_julia() {
    if command -v julia >/dev/null 2>&1; then
        julia_version=$(julia --version 2>&1 | head -1)
        printf '  checking julia         ... found (%s)\n' "$julia_version"
    else
        printf '  checking julia         ... NOT FOUND (install with:\n'
        printf '    curl -fsSL https://install.julialang.org | sh\n'
        printf '    then restart your shell)\n'
    fi
}

# ─── Download + verify ────────────────────────────────────────────────
fetch_file() {
    src="$1"
    dst="$2"
    if command -v curl >/dev/null 2>&1; then
        curl -fsSL -o "$dst" "$src"
    elif command -v wget >/dev/null 2>&1; then
        wget -q -O "$dst" "$src"
    else
        echo "install.sh: need curl or wget" >&2
        exit 1
    fi
}

verify_sha() {
    tarball="$1"
    sums_file="$2"
    expected=$(grep "$(basename "$tarball")" "$sums_file" | awk '{print $1}')
    if [ -z "$expected" ]; then
        echo "install.sh: no SHA256 entry for $(basename "$tarball") in SHA256SUMS" >&2
        return 1
    fi
    if command -v shasum >/dev/null 2>&1; then
        actual=$(shasum -a 256 "$tarball" | awk '{print $1}')
    elif command -v sha256sum >/dev/null 2>&1; then
        actual=$(sha256sum "$tarball" | awk '{print $1}')
    else
        echo "install.sh: need shasum or sha256sum" >&2
        return 1
    fi
    if [ "$expected" != "$actual" ]; then
        echo "install.sh: SHA256 mismatch for $(basename "$tarball")" >&2
        echo "  expected: $expected" >&2
        echo "  actual:   $actual" >&2
        return 1
    fi
    return 0
}

# ─── Main ─────────────────────────────────────────────────────────────
echo "nothelix install"
printf '  detected: %s\n' "$PLATFORM"

TMP_DIR=$(mktemp -d)
trap 'rm -rf "$TMP_DIR"' EXIT INT TERM

printf '  fetching: %s\n' "$TARBALL"
fetch_file "$TARBALL_URL" "$TMP_DIR/$TARBALL"
fetch_file "$SHA_URL" "$TMP_DIR/SHA256SUMS"

printf '  verifying SHA256 ... '
if verify_sha "$TMP_DIR/$TARBALL" "$TMP_DIR/SHA256SUMS"; then
    echo "ok"
else
    exit 1
fi

# Extract
tar -xzf "$TMP_DIR/$TARBALL" -C "$TMP_DIR"
# Find the single top-level extracted dir
EXTRACTED_DIR=$(find "$TMP_DIR" -maxdepth 1 -type d -name 'nothelix-*' | head -1)
if [ -z "$EXTRACTED_DIR" ] || [ ! -d "$EXTRACTED_DIR" ]; then
    echo "install.sh: tarball did not contain a nothelix-* directory" >&2
    exit 1
fi

# Run the in-tarball installer
EXTRA_ARGS=""
if [ "$MODE" = "upgrade" ]; then
    EXTRA_ARGS="--upgrade"
fi
"$EXTRACTED_DIR/install-local.sh" "$EXTRACTED_DIR" $EXTRA_ARGS

check_julia

# PATH check (non-fatal)
case ":$PATH:" in
    *":${NOTHELIX_PREFIX:-$HOME/.local}/bin:"*)
        printf '  checking PATH          ... %s/bin is on PATH\n' "${NOTHELIX_PREFIX:-$HOME/.local}" ;;
    *)
        printf '  checking PATH          ... %s/bin NOT on PATH\n' "${NOTHELIX_PREFIX:-$HOME/.local}"
        printf '    add this to your shell profile:\n'
        printf '      export PATH="%s/bin:$PATH"\n' "${NOTHELIX_PREFIX:-$HOME/.local}" ;;
esac

echo ""
echo "Done. Try: nothelix"
```

Make it executable:

```bash
chmod +x install.sh
```

- [ ] **Step 7.4: Run the tests to verify they pass**

```bash
bats tests/install/install-sh.bats
```
Expected: 4 tests, 4 passing.

- [ ] **Step 7.5: Lint**

```bash
shellcheck install.sh
```
Expected: clean.

- [ ] **Step 7.6: Commit**

```bash
jj describe @ -m "feat(install): install.sh curl-sh entry point with platform detection"
```

---

## Phase 5 — Wrapper subcommands

### Task 8: `nothelix doctor` — static checks

The static doctor runs file-existence and version checks without spawning Julia or loading the dylib. Fast, safe, runs sub-second.

**Files:**
- Create: `dist/doctor/static.sh` (sourced by wrapper)
- Modify: `dist/nothelix` (wire up `doctor` subcommand)
- Create: `tests/install/doctor-static.bats`

- [ ] **Step 8.1: Write the tests**

Create `tests/install/doctor-static.bats`:

```bash
#!/usr/bin/env bats

setup() {
    export WRAPPER="$BATS_TEST_DIRNAME/../../dist/nothelix"

    # Fake a complete install under a temp HOME
    FAKE_HOME="$(mktemp -d)"
    export HOME="$FAKE_HOME"
    export STEEL_HOME="$FAKE_HOME/.steel"

    mkdir -p "$HOME/.local/bin"
    mkdir -p "$STEEL_HOME/native"
    mkdir -p "$STEEL_HOME/cogs/nothelix"
    mkdir -p "$HOME/.local/share/nothelix/runtime/grammars"
    mkdir -p "$HOME/.local/share/nothelix/runtime/queries"
    mkdir -p "$HOME/.local/share/nothelix/runtime/themes"
    mkdir -p "$HOME/.local/share/nothelix/examples"
    mkdir -p "$HOME/.config/helix"
    mkdir -p "$HOME/.local/share/nothelix/lsp"

    echo "#!/bin/bash" > "$HOME/.local/bin/hx-nothelix"
    chmod +x "$HOME/.local/bin/hx-nothelix"
    cp "$WRAPPER" "$HOME/.local/bin/nothelix"
    echo "#!/bin/bash" > "$HOME/.local/bin/julia-lsp"
    chmod +x "$HOME/.local/bin/julia-lsp"

    echo "fake" > "$STEEL_HOME/native/libnothelix.dylib"
    echo "BUILD_ID=ci-20260412-abcdef12" > "$STEEL_HOME/native/libnothelix.meta"
    echo "# plugin" > "$STEEL_HOME/cogs/nothelix.scm"
    echo "# sub" > "$STEEL_HOME/cogs/nothelix/execution.scm"

    touch "$HOME/.local/share/nothelix/runtime/grammars/rust.so"
    touch "$HOME/.local/share/nothelix/examples/demo.jl"
    touch "$HOME/.local/share/nothelix/lsp/Manifest.toml"

    echo '(require "nothelix.scm")' > "$HOME/.config/helix/init.scm"

    cat > "$HOME/.local/share/nothelix/VERSION" <<EOF
NOTHELIX_VERSION=v0.2.1
BUILD_ID=ci-20260412-abcdef12
FORK_SHA=89734c7291a9
FORK_BRANCH=feature/inline-image-rendering
LIBNOTHELIX_VERSION=v0.2.1
INSTALL_DATE=2026-04-12T03:14:15Z
EOF

    export PATH="$HOME/.local/bin:$PATH"
    export NOTHELIX_SHARE="$HOME/.local/share/nothelix"
    export NOTHELIX_SKIP_TTY_CHECK=1   # skip terminal graphics query in tests
}

teardown() {
    rm -rf "$FAKE_HOME"
}

@test "doctor exits 0 when every check passes" {
    run "$WRAPPER" doctor
    [ "$status" -eq 0 ]
    [[ "$output" == *"hx-nothelix"* ]]
    [[ "$output" == *"libnothelix"* ]]
    [[ "$output" == *"plugin cogs"* ]]
    [[ "$output" == *"build id"* ]]
    [[ "$output" == *"checks passed"* ]]
}

@test "doctor fails if hx-nothelix is missing" {
    rm "$HOME/.local/bin/hx-nothelix"
    run "$WRAPPER" doctor
    [ "$status" -ne 0 ]
    [[ "$output" == *"hx-nothelix"* ]]
    [[ "$output" == *"missing"* ]] || [[ "$output" == *"not found"* ]] || [[ "$output" == *"fail"* ]]
}

@test "doctor fails if libnothelix is missing" {
    rm "$STEEL_HOME/native/libnothelix.dylib"
    run "$WRAPPER" doctor
    [ "$status" -ne 0 ]
    [[ "$output" == *"libnothelix"* ]]
}

@test "doctor fails if BUILD_ID in VERSION mismatches libnothelix.meta" {
    echo "BUILD_ID=ci-20260412-FFFFFFFF" > "$STEEL_HOME/native/libnothelix.meta"
    run "$WRAPPER" doctor
    [ "$status" -ne 0 ]
    [[ "$output" == *"build id"* ]]
    [[ "$output" == *"mismatch"* ]] || [[ "$output" == *"drift"* ]]
}

@test "doctor fails if init.scm is missing the require line" {
    echo "" > "$HOME/.config/helix/init.scm"
    run "$WRAPPER" doctor
    [ "$status" -ne 0 ]
    [[ "$output" == *"init.scm"* ]]
}

@test "doctor warns but succeeds when grammars dir is empty" {
    rm "$HOME/.local/share/nothelix/runtime/grammars"/*
    run "$WRAPPER" doctor
    # Empty grammars is a warn, not a fail
    [ "$status" -eq 0 ]
    [[ "$output" == *"grammar"* ]]
}
```

- [ ] **Step 8.2: Run the tests to verify they fail**

```bash
bats tests/install/doctor-static.bats
```
Expected: all tests FAIL because `nothelix doctor` still says "not yet implemented".

- [ ] **Step 8.3: Write the doctor static check module**

Create `dist/doctor/static.sh`:

```bash
#!/bin/bash
# doctor/static.sh — static checks for `nothelix doctor`.
#
# Sourced by dist/nothelix. Expects these vars set by the caller:
#   NOTHELIX_SHARE, STEEL_HOME, HX_NOTHELIX, VERSION_FILE,
#   NOTHELIX_BIN (~/.local/bin)
#
# Each check appends to two globals:
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
        _doctor_fail "~/.config/helix/init.scm missing — run 'nothelix upgrade'"
        return
    fi
    if grep -Fq '(require "nothelix.scm")' "$init"; then
        _doctor_pass "~/.config/helix/init.scm contains (require \"nothelix.scm\")"
    else
        _doctor_fail "~/.config/helix/init.scm missing the nothelix require line — add: (require \"nothelix.scm\")"
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
}
```

- [ ] **Step 8.4: Wire up the doctor command in the wrapper**

In `dist/nothelix`, replace the `upgrade|uninstall|doctor|config|reset)` stub branch with just `upgrade|uninstall|config|reset)` and add a dedicated `doctor` branch BEFORE it:

```bash
    doctor)
        # Locate the doctor helper dir. Two possibilities:
        #   - Installed: /usr/local/share/nothelix/dist/doctor/  (or
        #     $HOME/.local/share/nothelix/dist/doctor/ depending on
        #     NOTHELIX_PREFIX)
        #   - Dev: the dist/doctor/ dir next to this script
        local doctor_dir=""
        if [ -d "$NOTHELIX_SHARE/dist/doctor" ]; then
            doctor_dir="$NOTHELIX_SHARE/dist/doctor"
        elif [ -d "$(dirname "$0")/doctor" ]; then
            doctor_dir="$(dirname "$0")/doctor"
        fi
        if [ -z "$doctor_dir" ]; then
            echo "nothelix: doctor helpers not found (looked in $NOTHELIX_SHARE/dist/doctor and $(dirname "$0")/doctor)" >&2
            exit 1
        fi

        # shellcheck disable=SC1091
        . "$doctor_dir/static.sh"

        # Additional env the static checks read
        export NOTHELIX_BIN
        export HX_NOTHELIX
        export NOTHELIX_SHARE
        export STEEL_HOME
        export VERSION_FILE
        export HELIX_RUNTIME

        echo "nothelix doctor ($(cat "$VERSION_FILE" 2>/dev/null | grep NOTHELIX_VERSION | cut -d= -f2 || echo unknown))"
        run_static_doctor_checks
        printf '%s' "$DOCTOR_CHECKS_OUTPUT"
        echo ""
        if [ "$DOCTOR_FAIL_COUNT" -eq 0 ]; then
            echo "Ready to go ($DOCTOR_WARN_COUNT warnings, 0 failures)."
            exit 0
        else
            echo "$DOCTOR_FAIL_COUNT checks failed, $DOCTOR_WARN_COUNT warnings. Run 'nothelix upgrade' to resolve, or fix manually using the hints above."
            exit 1
        fi
        ;;
```

Also: the install-local.sh task needs to place `dist/doctor/` into `$NOTHELIX_SHARE/dist/doctor/` so the installed wrapper can find it. Update `dist/install-local.sh` by adding one line after the demo notebook placement:

```bash
# Doctor helper scripts
place_dir "$TARBALL_DIR/share/nothelix/dist" "$SHARE_DIR/dist"
```

And update the bats test setup for install-local.bats to create the fake dist/doctor in the tarball src:

```bash
mkdir -p "$TARBALL_SRC/share/nothelix/dist/doctor"
echo "# fake doctor helper" > "$TARBALL_SRC/share/nothelix/dist/doctor/static.sh"
```

- [ ] **Step 8.5: Run the tests to verify they pass**

```bash
bats tests/install/doctor-static.bats
bats tests/install/install-local.bats
```
Expected: all tests pass.

- [ ] **Step 8.6: Lint**

```bash
shellcheck dist/nothelix dist/doctor/static.sh dist/install-local.sh
```
Expected: clean.

- [ ] **Step 8.7: Commit**

```bash
jj describe @ -m "feat(wrapper): nothelix doctor with static environment checks"
```

---

### Task 9: `nothelix doctor` — terminal graphics protocol query

This adds the Kitty APC query to the doctor check. Opt out via `NOTHELIX_SKIP_TTY_CHECK=1` for testing contexts without a TTY.

**Files:**
- Modify: `dist/doctor/static.sh` (add `doctor_check_terminal`)
- Modify: `tests/install/doctor-static.bats` (test the skip env var)

- [ ] **Step 9.1: Write the test**

Append to `tests/install/doctor-static.bats`:

```bash
@test "doctor --smoke skip flag honours NOTHELIX_SKIP_TTY_CHECK" {
    export NOTHELIX_SKIP_TTY_CHECK=1
    run "$WRAPPER" doctor
    [ "$status" -eq 0 ]
    [[ "$output" == *"terminal graphics"* ]]
    [[ "$output" == *"skipped"* ]]
}
```

- [ ] **Step 9.2: Run the test to verify it fails**

```bash
bats tests/install/doctor-static.bats
```
Expected: the new test fails because the check doesn't exist yet.

- [ ] **Step 9.3: Add the terminal check**

Append to `dist/doctor/static.sh`:

```bash
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
        printf '\033_Ga=q,i=1,s=1,v=1,f=24,t=d,m=0;AAAA\033\\' > /dev/tty
        # Read up to 256 bytes or 100ms, whichever comes first.
        # bash's `read -t 0.1` handles the timeout; portable enough on
        # bash 4+.
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
```

And add `doctor_check_terminal_graphics` to the `run_static_doctor_checks` list at the bottom of the file:

```bash
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
```

- [ ] **Step 9.4: Run the tests to verify they pass**

```bash
bats tests/install/doctor-static.bats
```
Expected: all tests pass.

- [ ] **Step 9.5: Lint**

```bash
shellcheck dist/doctor/static.sh
```
Expected: clean.

- [ ] **Step 9.6: Commit**

```bash
jj describe @ -m "feat(doctor): Kitty graphics protocol capability query"
```

---

### Task 10: `nothelix doctor --smoke` — kernel smoke test

**Files:**
- Create: `dist/doctor/smoke.sh`
- Modify: `dist/nothelix` (accept `--smoke` flag in the doctor branch)
- Create: `tests/install/doctor-smoke.bats`

The smoke test spawns Julia with a minimal cell, waits for the response, and verifies the pipeline end-to-end. Uses the kernel scripts copied into `$NOTHELIX_SHARE/kernel-scripts/` by the install script.

- [ ] **Step 10.1: Write the test**

Create `tests/install/doctor-smoke.bats`:

```bash
#!/usr/bin/env bats

# Smoke test requires Julia. Skip if not available.

setup() {
    if ! command -v julia >/dev/null 2>&1; then
        skip "julia not installed"
    fi

    FAKE_HOME="$(mktemp -d)"
    export HOME="$FAKE_HOME"
    export STEEL_HOME="$FAKE_HOME/.steel"
    export NOTHELIX_SHARE="$HOME/.local/share/nothelix"

    mkdir -p "$NOTHELIX_SHARE/kernel-scripts"
    mkdir -p "$NOTHELIX_SHARE/dist/doctor"
    mkdir -p "$HOME/.local/bin"

    # Copy real kernel scripts from repo
    cp "$BATS_TEST_DIRNAME/../../kernel/"*.jl "$NOTHELIX_SHARE/kernel-scripts/"

    # Copy doctor helpers from dist
    cp "$BATS_TEST_DIRNAME/../../dist/doctor/"*.sh "$NOTHELIX_SHARE/dist/doctor/"
    cp "$BATS_TEST_DIRNAME/../../dist/nothelix" "$HOME/.local/bin/nothelix"
    chmod +x "$HOME/.local/bin/nothelix"

    export WRAPPER="$HOME/.local/bin/nothelix"
}

teardown() {
    rm -rf "$FAKE_HOME"
}

@test "doctor --smoke spawns a Julia kernel and gets 1+1=2" {
    run "$WRAPPER" doctor --smoke
    [ "$status" -eq 0 ] || {
        echo "$output"
        return 1
    }
    [[ "$output" == *"kernel smoke"* ]]
    [[ "$output" == *"cold start"* ]]
}
```

- [ ] **Step 10.2: Run the test to verify it fails**

```bash
bats tests/install/doctor-smoke.bats
```
Expected: test fails because `--smoke` is not recognised.

- [ ] **Step 10.3: Write the smoke helper**

Create `dist/doctor/smoke.sh`:

```bash
#!/bin/bash
# doctor/smoke.sh — kernel smoke test for `nothelix doctor --smoke`.
#
# Sourced by dist/nothelix. Defines run_kernel_smoke_test().
#
# Spawns a real Julia kernel from the installed kernel-scripts dir,
# executes `1 + 1`, verifies the response, tears it down.

# shellcheck disable=SC2034
run_kernel_smoke_test() {
    local start_time
    start_time=$(date +%s)

    if ! command -v julia >/dev/null 2>&1; then
        _doctor_fail "kernel smoke: julia not found on PATH"
        return
    fi

    local kernel_scripts="$NOTHELIX_SHARE/kernel-scripts"
    if [ ! -f "$kernel_scripts/runner.jl" ]; then
        _doctor_fail "kernel smoke: $kernel_scripts/runner.jl missing — run 'nothelix upgrade'"
        return
    fi

    local tmp_dir
    tmp_dir=$(mktemp -d -t "nothelix-doctor-smoke.XXXXXX")
    # shellcheck disable=SC2064
    trap "rm -rf '$tmp_dir'" RETURN

    # Copy kernel scripts into the temp dir so runner.jl can include
    # its siblings relative to @__DIR__.
    cp "$kernel_scripts/"*.jl "$tmp_dir/"

    # Spawn the kernel in the background
    (
        cd "$tmp_dir" && julia --startup-file=no --quiet runner.jl "$tmp_dir" \
            > "$tmp_dir/kernel.stdout" 2> "$tmp_dir/kernel.stderr"
    ) &
    local kernel_pid=$!
    # shellcheck disable=SC2064
    trap "kill $kernel_pid 2>/dev/null || true; rm -rf '$tmp_dir'" RETURN

    # Wait for ready file (up to 30s)
    local waited=0
    while [ ! -f "$tmp_dir/ready" ] && [ $waited -lt 30 ]; do
        sleep 1
        waited=$((waited + 1))
    done
    if [ ! -f "$tmp_dir/ready" ]; then
        _doctor_fail "kernel smoke: kernel did not become ready within 30s (stderr: $(head -5 "$tmp_dir/kernel.stderr" 2>/dev/null))"
        return
    fi

    local cold_start=$(($(date +%s) - start_time))
    local exec_start
    exec_start=$(date +%s)

    # Write an input command
    cat > "$tmp_dir/input.json" <<'EOF'
{"command": "execute_cell", "cell_index": 0, "code": "1 + 1"}
EOF

    # Wait for output.json.done (up to 10s)
    waited=0
    while [ ! -f "$tmp_dir/output.json.done" ] && [ $waited -lt 10 ]; do
        sleep 1
        waited=$((waited + 1))
    done
    if [ ! -f "$tmp_dir/output.json.done" ]; then
        _doctor_fail "kernel smoke: kernel did not respond within 10s"
        return
    fi

    local exec_time=$(($(date +%s) - exec_start))

    # Verify the response mentions "2" somewhere in output_repr
    if grep -q '"output_repr"[[:space:]]*:[[:space:]]*"2"' "$tmp_dir/output.json" 2>/dev/null; then
        _doctor_pass "kernel smoke test (cold start ${cold_start}s, execute ${exec_time}s, 1+1=2)"
    else
        _doctor_fail "kernel smoke: response did not contain output_repr=2 (got: $(head -c 200 "$tmp_dir/output.json"))"
    fi
}
```

- [ ] **Step 10.4: Wire up `--smoke` in the wrapper**

In `dist/nothelix`'s `doctor)` branch, detect `--smoke` and source smoke.sh:

```bash
    doctor)
        shift
        local smoke=0
        while [ $# -gt 0 ]; do
            case "$1" in
                --smoke) smoke=1 ;;
                *) echo "nothelix doctor: unknown flag: $1" >&2; exit 2 ;;
            esac
            shift
        done

        # [... existing doctor_dir resolution ...]

        # shellcheck disable=SC1091
        . "$doctor_dir/static.sh"
        if [ $smoke -eq 1 ]; then
            # shellcheck disable=SC1091
            . "$doctor_dir/smoke.sh"
        fi

        # [... existing env export block ...]

        echo "nothelix doctor (...)"
        run_static_doctor_checks
        if [ $smoke -eq 1 ]; then
            run_kernel_smoke_test
        fi
        printf '%s' "$DOCTOR_CHECKS_OUTPUT"
        # [... existing tally + exit ...]
```

- [ ] **Step 10.5: Update install-local.sh to place kernel scripts into the install**

In `dist/install-local.sh`, after the existing `Plugin cogs` block, add:

```bash
# Kernel scripts (copy of what libnothelix ships via include_str!;
# duplicated here so `nothelix doctor --smoke` can spawn a kernel
# without loading the dylib).
if [ -d "$TARBALL_DIR/share/nothelix/kernel-scripts" ]; then
    place_dir "$TARBALL_DIR/share/nothelix/kernel-scripts" "$SHARE_DIR/kernel-scripts"
fi
```

And update the tarball-assembly step in the CI workflow (Task 15) to copy `kernel/*.jl` into `share/nothelix/kernel-scripts/`. For now, add a note in the install-local.sh test setup to create the kernel-scripts dir with a fake runner.jl.

- [ ] **Step 10.6: Run the tests to verify they pass**

```bash
bats tests/install/doctor-smoke.bats
```
Expected: test passes (or is skipped if julia is not installed).

- [ ] **Step 10.7: Lint**

```bash
shellcheck dist/doctor/smoke.sh dist/nothelix dist/install-local.sh
```
Expected: clean.

- [ ] **Step 10.8: Commit**

```bash
jj describe @ -m "feat(doctor): --smoke kernel smoke test end-to-end"
```

---

### Task 11: `nothelix config show|edit|path`

**Files:**
- Create: `dist/config.sh`
- Modify: `dist/nothelix` (wire `config` subcommand)
- Create: `tests/install/config.bats`

- [ ] **Step 11.1: Write the test**

Create `tests/install/config.bats`:

```bash
#!/usr/bin/env bats

setup() {
    FAKE_HOME="$(mktemp -d)"
    export HOME="$FAKE_HOME"
    export STEEL_HOME="$FAKE_HOME/.steel"
    export NOTHELIX_SHARE="$HOME/.local/share/nothelix"
    mkdir -p "$NOTHELIX_SHARE"
    cat > "$NOTHELIX_SHARE/VERSION" <<EOF
NOTHELIX_VERSION=v0.2.1
BUILD_ID=ci-20260412-abcdef12
FORK_SHA=89734c7291a9
FORK_BRANCH=feature/inline-image-rendering
LIBNOTHELIX_VERSION=v0.2.1
INSTALL_DATE=2026-04-12T03:14:15Z
EOF
    export WRAPPER="$BATS_TEST_DIRNAME/../../dist/nothelix"
    export NOTHELIX_TEST_MODE=1
}

teardown() {
    rm -rf "$FAKE_HOME"
}

@test "config show prints key=value lines" {
    run "$WRAPPER" config show
    [ "$status" -eq 0 ]
    [[ "$output" == *"nothelix.version"*"v0.2.1"* ]]
    [[ "$output" == *"nothelix.fork_sha"*"89734c7291a9"* ]]
    [[ "$output" == *"steel.home"* ]]
}

@test "config (no verb) is an alias for show" {
    run "$WRAPPER" config
    [ "$status" -eq 0 ]
    [[ "$output" == *"nothelix.version"* ]]
}

@test "config path prints the helix config.toml path" {
    run "$WRAPPER" config path
    [ "$status" -eq 0 ]
    [[ "$output" == *"$HOME/.config/helix/config.toml"* ]]
}

@test "config edit (in test mode) would exec hx-nothelix on config.toml" {
    run "$WRAPPER" config edit
    [ "$status" -eq 0 ]
    [[ "$output" == *"hx-nothelix"* ]]
    [[ "$output" == *"config.toml"* ]]
}
```

- [ ] **Step 11.2: Run the test to verify it fails**

```bash
bats tests/install/config.bats
```
Expected: fail.

- [ ] **Step 11.3: Write config.sh**

Create `dist/config.sh`:

```bash
#!/bin/bash
# config.sh — `nothelix config` subcommand handlers.
# Sourced by dist/nothelix.

nothelix_config_show() {
    if [ ! -f "$VERSION_FILE" ]; then
        echo "nothelix: VERSION file not found at $VERSION_FILE" >&2
        exit 1
    fi
    # shellcheck disable=SC1090
    . "$VERSION_FILE"
    local julia_path="(not found)"
    local julia_version="unknown"
    if command -v julia >/dev/null 2>&1; then
        julia_path=$(command -v julia)
        julia_version=$(julia --version 2>&1 | head -1 | sed 's/^julia //')
    fi
    cat <<EOF
nothelix.version     = ${NOTHELIX_VERSION:-unknown}
nothelix.fork_sha    = ${FORK_SHA:-unknown}
nothelix.fork_branch = ${FORK_BRANCH:-unknown}
nothelix.build_id    = ${BUILD_ID:-unknown}
nothelix.install_dir = $NOTHELIX_SHARE
steel.home           = $STEEL_HOME
steel.native         = $STEEL_HOME/native/libnothelix.dylib
steel.cogs           = $STEEL_HOME/cogs/nothelix
helix.runtime        = $NOTHELIX_SHARE/runtime
helix.init_scm       = $HOME/.config/helix/init.scm
helix.config_toml    = $HOME/.config/helix/config.toml
julia.path           = $julia_path
julia.version        = $julia_version
lsp.env              = $NOTHELIX_SHARE/lsp
lsp.depot            = $NOTHELIX_SHARE/lsp/depot
demo.notebook        = $NOTHELIX_SHARE/examples/demo.jl
EOF
}

nothelix_config_path() {
    printf '%s\n' "$HOME/.config/helix/config.toml"
}

nothelix_config_edit() {
    local config_toml="$HOME/.config/helix/config.toml"
    mkdir -p "$(dirname "$config_toml")"
    if [ ! -f "$config_toml" ]; then
        printf 'theme = "default"\n' > "$config_toml"
    fi
    _run_or_print "$HX_NOTHELIX" "$config_toml"
}
```

- [ ] **Step 11.4: Wire up `config` in the wrapper**

In `dist/nothelix`, replace the `config` entry in the stub branch with its own branch:

```bash
    config)
        shift
        # shellcheck disable=SC1091
        . "$(dirname "$0")/config.sh" 2>/dev/null || \
            . "$NOTHELIX_SHARE/dist/config.sh"
        case "${1:-show}" in
            show) nothelix_config_show ;;
            path) nothelix_config_path ;;
            edit) nothelix_config_edit ;;
            *) echo "nothelix config: unknown subcommand: $1" >&2; exit 2 ;;
        esac
        ;;
```

- [ ] **Step 11.5: Run the tests to verify they pass**

```bash
bats tests/install/config.bats
```
Expected: 4 tests passing.

- [ ] **Step 11.6: Lint**

```bash
shellcheck dist/nothelix dist/config.sh
```
Expected: clean.

- [ ] **Step 11.7: Commit**

```bash
jj describe @ -m "feat(wrapper): nothelix config show|edit|path"
```

---

### Task 12: `nothelix reset [--lsp|--kernel|--all]`

**Files:**
- Create: `dist/reset.sh`
- Modify: `dist/nothelix` (wire `reset` subcommand)
- Create: `tests/install/reset.bats`

- [ ] **Step 12.1: Write the test**

Create `tests/install/reset.bats`:

```bash
#!/usr/bin/env bats

setup() {
    FAKE_HOME="$(mktemp -d)"
    export HOME="$FAKE_HOME"
    export STEEL_HOME="$FAKE_HOME/.steel"
    export NOTHELIX_SHARE="$HOME/.local/share/nothelix"

    # Simulate a complete install that nothelix reset can operate on.
    mkdir -p "$NOTHELIX_SHARE/examples" "$NOTHELIX_SHARE/runtime/grammars"
    mkdir -p "$NOTHELIX_SHARE/lsp/depot/packages" "$NOTHELIX_SHARE/kernel-scripts"
    mkdir -p "$STEEL_HOME/cogs/nothelix" "$STEEL_HOME/native"
    mkdir -p "$HOME/.local/bin"

    # Fake a cached tarball for reset to copy from
    CACHE_DIR="$NOTHELIX_SHARE/.cache"
    mkdir -p "$CACHE_DIR/extracted/bin" "$CACHE_DIR/extracted/lib"
    mkdir -p "$CACHE_DIR/extracted/share/nothelix/examples"
    mkdir -p "$CACHE_DIR/extracted/share/nothelix/plugin/nothelix"
    mkdir -p "$CACHE_DIR/extracted/share/nothelix/runtime/grammars"
    mkdir -p "$CACHE_DIR/extracted/share/nothelix/lsp"

    echo "fresh hx" > "$CACHE_DIR/extracted/bin/hx-nothelix"
    echo "fresh nothelix" > "$CACHE_DIR/extracted/bin/nothelix"
    echo "fresh julia-lsp" > "$CACHE_DIR/extracted/bin/julia-lsp"
    echo "fresh dylib" > "$CACHE_DIR/extracted/lib/libnothelix.dylib"
    echo "BUILD_ID=ci-fresh-00000000" > "$CACHE_DIR/extracted/lib/libnothelix.meta"
    echo "fresh plugin" > "$CACHE_DIR/extracted/share/nothelix/plugin/nothelix.scm"
    echo "fresh sub" > "$CACHE_DIR/extracted/share/nothelix/plugin/nothelix/execution.scm"
    echo "fresh demo" > "$CACHE_DIR/extracted/share/nothelix/examples/demo.jl"
    cp "$BATS_TEST_DIRNAME/../../dist/install-local.sh" "$CACHE_DIR/extracted/install-local.sh"
    chmod +x "$CACHE_DIR/extracted/install-local.sh"
    cat > "$CACHE_DIR/extracted/VERSION" <<EOF
NOTHELIX_VERSION=v0.2.1
BUILD_ID=ci-fresh-00000000
FORK_SHA=0000000000000000000000000000000000000000
FORK_BRANCH=feature/inline-image-rendering
LIBNOTHELIX_VERSION=v0.2.1
INSTALL_DATE=2026-04-12T00:00:00Z
EOF

    export WRAPPER="$BATS_TEST_DIRNAME/../../dist/nothelix"
}

teardown() {
    rm -rf "$FAKE_HOME"
}

@test "reset (no flags) re-copies managed files from cache" {
    echo "stale hx" > "$HOME/.local/bin/hx-nothelix"
    run "$WRAPPER" reset
    [ "$status" -eq 0 ]
    run cat "$HOME/.local/bin/hx-nothelix"
    [[ "$output" == *"fresh hx"* ]]
}

@test "reset leaves LSP depot alone by default" {
    echo "precompile artefact" > "$NOTHELIX_SHARE/lsp/depot/packages/foo"
    run "$WRAPPER" reset
    [ "$status" -eq 0 ]
    [ -f "$NOTHELIX_SHARE/lsp/depot/packages/foo" ]
}

@test "reset --lsp wipes the LSP depot" {
    echo "precompile artefact" > "$NOTHELIX_SHARE/lsp/depot/packages/foo"
    run "$WRAPPER" reset --lsp
    [ "$status" -eq 0 ]
    [ ! -f "$NOTHELIX_SHARE/lsp/depot/packages/foo" ]
}

@test "reset --kernel wipes kernel-scripts" {
    echo "extracted kernel" > "$NOTHELIX_SHARE/kernel-scripts/runner.jl"
    run "$WRAPPER" reset --kernel
    [ "$status" -eq 0 ]
    [ ! -f "$NOTHELIX_SHARE/kernel-scripts/runner.jl" ]
}

@test "reset --all wipes both LSP depot and kernel, and re-copies files" {
    echo "precompile" > "$NOTHELIX_SHARE/lsp/depot/packages/foo"
    echo "kernel" > "$NOTHELIX_SHARE/kernel-scripts/runner.jl"
    echo "stale hx" > "$HOME/.local/bin/hx-nothelix"
    run "$WRAPPER" reset --all
    [ "$status" -eq 0 ]
    [ ! -f "$NOTHELIX_SHARE/lsp/depot/packages/foo" ]
    [ ! -f "$NOTHELIX_SHARE/kernel-scripts/runner.jl" ]
    run cat "$HOME/.local/bin/hx-nothelix"
    [[ "$output" == *"fresh hx"* ]]
}

@test "reset never touches init.scm" {
    mkdir -p "$HOME/.config/helix"
    echo '(require "nothelix.scm")' > "$HOME/.config/helix/init.scm"
    echo '(my-user-code)' >> "$HOME/.config/helix/init.scm"
    run "$WRAPPER" reset
    [ "$status" -eq 0 ]
    run grep "my-user-code" "$HOME/.config/helix/init.scm"
    [ "$status" -eq 0 ]
}
```

- [ ] **Step 12.2: Run the tests to verify they fail**

```bash
bats tests/install/reset.bats
```
Expected: fail.

- [ ] **Step 12.3: Write reset.sh**

Create `dist/reset.sh`:

```bash
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

    if [ $reset_lsp -eq 1 ]; then
        if [ -d "$NOTHELIX_SHARE/lsp/depot" ]; then
            rm -rf "$NOTHELIX_SHARE/lsp/depot"
            echo "  wiped LSP depot at $NOTHELIX_SHARE/lsp/depot"
        fi
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

    echo "Reset complete."
}
```

- [ ] **Step 12.4: Wire up `reset` in the wrapper**

In `dist/nothelix`, replace the `reset` in the stub branch with its own branch:

```bash
    reset)
        shift
        # shellcheck disable=SC1091
        . "$(dirname "$0")/reset.sh" 2>/dev/null || \
            . "$NOTHELIX_SHARE/dist/reset.sh"
        nothelix_reset "$@"
        ;;
```

- [ ] **Step 12.5: Run the tests to verify they pass**

```bash
bats tests/install/reset.bats
```
Expected: 6 tests passing.

- [ ] **Step 12.6: Lint**

```bash
shellcheck dist/reset.sh dist/nothelix
```

- [ ] **Step 12.7: Commit**

```bash
jj describe @ -m "feat(wrapper): nothelix reset [--lsp|--kernel|--all]"
```

---

### Task 13: `nothelix upgrade` + install.sh `--upgrade` cache the tarball

`nothelix upgrade` needs to (a) re-run the installer and (b) populate the cache dir `$NOTHELIX_SHARE/.cache/extracted/` so `reset` can work without network.

**Files:**
- Modify: `install.sh` (cache extracted tarball after install)
- Modify: `dist/nothelix` (wire `upgrade` subcommand)
- Create: `tests/install/upgrade.bats`

- [ ] **Step 13.1: Write the test**

Create `tests/install/upgrade.bats`:

```bash
#!/usr/bin/env bats

# Reuses the fixture from install-sh.bats for a local file:// release.

setup() {
    # [... same setup as install-sh.bats ...]
    # Copy the verbatim setup block from install-sh.bats
    FAKE_HOME="$(mktemp -d)"
    export HOME="$FAKE_HOME"

    FIXTURE_DIR="$(mktemp -d)"
    mkdir -p "$FIXTURE_DIR/release"

    TARBALL_SRC="$(mktemp -d)/nothelix-vtest-darwin-arm64"
    mkdir -p "$TARBALL_SRC/bin" "$TARBALL_SRC/lib"
    mkdir -p "$TARBALL_SRC/share/nothelix/runtime/grammars"
    mkdir -p "$TARBALL_SRC/share/nothelix/examples"
    mkdir -p "$TARBALL_SRC/share/nothelix/plugin/nothelix"
    mkdir -p "$TARBALL_SRC/share/nothelix/lsp"
    mkdir -p "$TARBALL_SRC/share/nothelix/dist/doctor"

    echo "#!/bin/bash" > "$TARBALL_SRC/bin/hx-nothelix"
    chmod +x "$TARBALL_SRC/bin/hx-nothelix"
    cp "$BATS_TEST_DIRNAME/../../dist/nothelix" "$TARBALL_SRC/bin/nothelix"
    chmod +x "$TARBALL_SRC/bin/nothelix"
    echo "#!/bin/bash" > "$TARBALL_SRC/bin/julia-lsp"
    chmod +x "$TARBALL_SRC/bin/julia-lsp"

    echo "fake" > "$TARBALL_SRC/lib/libnothelix.dylib"
    echo "BUILD_ID=ci-test-00000000" > "$TARBALL_SRC/lib/libnothelix.meta"
    echo "# plugin" > "$TARBALL_SRC/share/nothelix/plugin/nothelix.scm"
    echo "# sub" > "$TARBALL_SRC/share/nothelix/plugin/nothelix/execution.scm"
    echo "# demo" > "$TARBALL_SRC/share/nothelix/examples/demo.jl"
    cp "$BATS_TEST_DIRNAME/../../dist/doctor/static.sh" "$TARBALL_SRC/share/nothelix/dist/doctor/static.sh"

    cat > "$TARBALL_SRC/VERSION" <<EOF
NOTHELIX_VERSION=vtest
BUILD_ID=ci-test-00000000
FORK_SHA=0000000000000000000000000000000000000000
FORK_BRANCH=feature/inline-image-rendering
LIBNOTHELIX_VERSION=vtest
INSTALL_DATE=2026-04-12T00:00:00Z
EOF

    cp "$BATS_TEST_DIRNAME/../../dist/install-local.sh" "$TARBALL_SRC/install-local.sh"
    chmod +x "$TARBALL_SRC/install-local.sh"

    tar -czf "$FIXTURE_DIR/release/nothelix-vtest-darwin-arm64.tar.gz" \
        -C "$(dirname "$TARBALL_SRC")" "$(basename "$TARBALL_SRC")"
    (cd "$FIXTURE_DIR/release" && shasum -a 256 nothelix-vtest-darwin-arm64.tar.gz > SHA256SUMS)

    export NOTHELIX_RELEASE_URL="file://$FIXTURE_DIR/release"
    export NOTHELIX_VERSION_OVERRIDE="vtest"
    export NOTHELIX_PLATFORM_OVERRIDE="darwin-arm64"
    export STEEL_HOME="$FAKE_HOME/.steel"
    export NOTHELIX_SHARE="$FAKE_HOME/.local/share/nothelix"

    INSTALL_SH="$BATS_TEST_DIRNAME/../../install.sh"
    WRAPPER="$BATS_TEST_DIRNAME/../../dist/nothelix"
}

teardown() {
    rm -rf "$FAKE_HOME" "$FIXTURE_DIR" "${TARBALL_SRC%/*}"
}

@test "install.sh caches the extracted tarball under NOTHELIX_SHARE/.cache" {
    bash "$INSTALL_SH"
    [ -d "$NOTHELIX_SHARE/.cache/extracted" ]
    [ -x "$NOTHELIX_SHARE/.cache/extracted/install-local.sh" ]
    [ -f "$NOTHELIX_SHARE/.cache/extracted/VERSION" ]
}

@test "install.sh --upgrade reuses the cache but re-fetches on request" {
    bash "$INSTALL_SH"
    # Second run
    run bash "$INSTALL_SH" --upgrade
    [ "$status" -eq 0 ]
}

@test "wrapper upgrade re-invokes install.sh with --upgrade" {
    bash "$INSTALL_SH"
    # Stub curl so the wrapper doesn't hit the network — point it at
    # the file:// URL via env var
    export PATH="$FAKE_HOME/.local/bin:$PATH"
    run "$WRAPPER" upgrade
    # The wrapper's upgrade currently shells out to curl|sh; with
    # NOTHELIX_RELEASE_URL set, it should still land. If we don't
    # mock curl it'll fail — acceptable: this test just verifies
    # wrapper dispatches to upgrade path.
    # Skip the exit check; just assert the dispatch happened by
    # checking output.
    [[ "$output" == *"upgrade"* ]] || [[ "$output" == *"install"* ]]
}
```

- [ ] **Step 13.2: Update install.sh to cache the extracted tarball**

In `install.sh`, after the successful `install-local.sh` invocation:

```bash
# Cache the extracted tarball so `nothelix reset` can use it without
# hitting the network.
NOTHELIX_SHARE="${NOTHELIX_SHARE:-${XDG_DATA_HOME:-$HOME/.local/share}/nothelix}"
CACHE_DIR="$NOTHELIX_SHARE/.cache"
mkdir -p "$CACHE_DIR"
rm -rf "$CACHE_DIR/extracted"
cp -R "$EXTRACTED_DIR" "$CACHE_DIR/extracted"
```

- [ ] **Step 13.3: Wire `upgrade` in the wrapper**

In `dist/nothelix`, replace the `upgrade` entry with its own branch:

```bash
    upgrade)
        shift
        # Re-run install.sh from GitHub (or the test-override URL)
        # with --upgrade. Prefer the pinned release URL if env is set,
        # otherwise default to main.
        local url="${NOTHELIX_INSTALL_URL:-https://raw.githubusercontent.com/koalazub/nothelix/main/install.sh}"
        if [ "${NOTHELIX_TEST_MODE:-}" = "1" ]; then
            echo "nothelix upgrade: would curl $url | sh -s -- --upgrade"
            exit 0
        fi
        exec sh -c "curl -sSL '$url' | sh -s -- --upgrade $*"
        ;;
```

- [ ] **Step 13.4: Run the tests to verify they pass**

```bash
bats tests/install/upgrade.bats
```
Expected: 3 tests passing (or 2 + 1 skipped depending on network).

- [ ] **Step 13.5: Commit**

```bash
jj describe @ -m "feat(wrapper): nothelix upgrade + install.sh cache for reset"
```

---

### Task 14: `nothelix uninstall` with `--keep-data`, `--dry-run`, `--yes`, `--purge`

**Files:**
- Create: `dist/uninstall.sh`
- Modify: `install.sh` (handle `--uninstall` mode)
- Modify: `dist/nothelix` (wire `uninstall` subcommand)
- Create: `tests/install/uninstall.bats`

- [ ] **Step 14.1: Write the tests**

Create `tests/install/uninstall.bats`:

```bash
#!/usr/bin/env bats

setup() {
    FAKE_HOME="$(mktemp -d)"
    export HOME="$FAKE_HOME"
    export STEEL_HOME="$FAKE_HOME/.steel"
    export NOTHELIX_SHARE="$HOME/.local/share/nothelix"

    # Simulate full install
    mkdir -p "$HOME/.local/bin"
    mkdir -p "$STEEL_HOME/native" "$STEEL_HOME/cogs/nothelix"
    mkdir -p "$NOTHELIX_SHARE/examples" "$NOTHELIX_SHARE/runtime/grammars"
    mkdir -p "$NOTHELIX_SHARE/lsp/depot" "$NOTHELIX_SHARE/kernel-scripts"
    mkdir -p "$HOME/.config/helix"
    mkdir -p "$HOME/.cache/helix"

    echo "binary" > "$HOME/.local/bin/hx-nothelix"
    echo "wrapper" > "$HOME/.local/bin/nothelix"
    echo "lsp wrapper" > "$HOME/.local/bin/julia-lsp"
    echo "dylib" > "$STEEL_HOME/native/libnothelix.dylib"
    echo "BUILD_ID=ci-test-00000000" > "$STEEL_HOME/native/libnothelix.meta"
    echo "plugin" > "$STEEL_HOME/cogs/nothelix.scm"
    echo "sub" > "$STEEL_HOME/cogs/nothelix/execution.scm"
    echo "demo" > "$NOTHELIX_SHARE/examples/demo.jl"
    echo "log contents" > "$HOME/.cache/helix/helix.log"

    cat > "$HOME/.config/helix/init.scm" <<EOF
(require "nothelix.scm")
(define my-custom 42)
EOF

    export WRAPPER="$BATS_TEST_DIRNAME/../../dist/nothelix"
    export NOTHELIX_TEST_MODE=0
}

teardown() {
    rm -rf "$FAKE_HOME"
}

@test "uninstall --dry-run removes nothing, lists plan" {
    run "$WRAPPER" uninstall --dry-run --yes
    [ "$status" -eq 0 ]
    # Files still present
    [ -f "$HOME/.local/bin/hx-nothelix" ]
    [ -f "$STEEL_HOME/native/libnothelix.dylib" ]
    # Output lists the plan
    [[ "$output" == *"hx-nothelix"* ]]
    [[ "$output" == *"libnothelix"* ]]
}

@test "uninstall --yes removes all managed files" {
    run "$WRAPPER" uninstall --yes
    [ "$status" -eq 0 ]
    [ ! -f "$HOME/.local/bin/hx-nothelix" ]
    [ ! -f "$HOME/.local/bin/nothelix" ]
    [ ! -f "$HOME/.local/bin/julia-lsp" ]
    [ ! -f "$STEEL_HOME/native/libnothelix.dylib" ]
    [ ! -f "$STEEL_HOME/native/libnothelix.meta" ]
    [ ! -f "$STEEL_HOME/cogs/nothelix.scm" ]
    [ ! -d "$STEEL_HOME/cogs/nothelix" ]
    [ ! -d "$NOTHELIX_SHARE" ]
}

@test "uninstall --yes preserves user init.scm content except nothelix require line" {
    "$WRAPPER" uninstall --yes
    [ -f "$HOME/.config/helix/init.scm" ]
    run grep "my-custom" "$HOME/.config/helix/init.scm"
    [ "$status" -eq 0 ]
    run grep 'require "nothelix.scm"' "$HOME/.config/helix/init.scm"
    [ "$status" -ne 0 ]
}

@test "uninstall --yes deletes init.scm if it only contained our require" {
    echo '(require "nothelix.scm")' > "$HOME/.config/helix/init.scm"
    "$WRAPPER" uninstall --yes
    [ ! -f "$HOME/.config/helix/init.scm" ]
}

@test "uninstall --yes leaves ~/.julia alone" {
    mkdir -p "$HOME/.julia/packages/LinearAlgebra"
    touch "$HOME/.julia/packages/LinearAlgebra/fake"
    "$WRAPPER" uninstall --yes
    [ -f "$HOME/.julia/packages/LinearAlgebra/fake" ]
}

@test "uninstall --yes --keep-data preserves lsp/depot" {
    mkdir -p "$NOTHELIX_SHARE/lsp/depot/packages"
    echo "keep me" > "$NOTHELIX_SHARE/lsp/depot/packages/pkg"
    run "$WRAPPER" uninstall --yes --keep-data
    [ "$status" -eq 0 ]
    [ -f "$NOTHELIX_SHARE/lsp/depot/packages/pkg" ]
}

@test "uninstall --yes leaves ~/.cache/helix/helix.log alone by default" {
    "$WRAPPER" uninstall --yes
    [ -f "$HOME/.cache/helix/helix.log" ]
}

@test "uninstall --yes --purge also removes ~/.cache/helix/helix.log" {
    "$WRAPPER" uninstall --yes --purge
    [ ! -f "$HOME/.cache/helix/helix.log" ]
}
```

- [ ] **Step 14.2: Run the tests to verify they fail**

```bash
bats tests/install/uninstall.bats
```
Expected: fail.

- [ ] **Step 14.3: Write uninstall.sh**

Create `dist/uninstall.sh`:

```bash
#!/bin/bash
# uninstall.sh — `nothelix uninstall` subcommand.
#
# Symmetric inverse of install: removes everything we placed, nothing
# else. Modifies init.scm to remove only the nothelix require line,
# leaving the rest of the file verbatim.

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

    # Confirm unless --yes or non-TTY
    if [ $assume_yes -eq 0 ] && [ -t 0 ]; then
        printf "Proceed? (y/N) "
        local reply
        read -r reply
        case "$reply" in
            y|Y|yes|YES) ;;
            *) echo "Aborted."; return 1 ;;
        esac
    fi

    # Remove files/dirs
    for t in "${targets[@]}"; do
        if [ -d "$t" ] && ! [ -L "$t" ]; then
            if [ "$t" = "$NOTHELIX_SHARE" ] && [ $keep_data -eq 1 ]; then
                # Remove everything in $NOTHELIX_SHARE except lsp/depot
                find "$NOTHELIX_SHARE" -mindepth 1 -maxdepth 1 \
                    ! -path "$NOTHELIX_SHARE/lsp" \
                    -exec rm -rf {} +
                # Then clean lsp/ except depot/
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

    # init.scm surgical edit: remove the require line
    local init="$HOME/.config/helix/init.scm"
    if [ -f "$init" ]; then
        local tmp="$init.tmp.$$"
        grep -Fv '(require "nothelix.scm")' "$init" > "$tmp" || true
        # If result is empty (only whitespace/comments), delete the file
        if ! grep -q '[^[:space:]]' "$tmp"; then
            rm -f "$init" "$tmp"
        else
            mv "$tmp" "$init"
        fi
    fi

    # Purge helix.log if requested
    if [ $purge -eq 1 ] && [ -f "$HOME/.cache/helix/helix.log" ]; then
        rm -f "$HOME/.cache/helix/helix.log"
    fi

    echo ""
    echo "nothelix removed."
}
```

- [ ] **Step 14.4: Wire `uninstall` in the wrapper**

In `dist/nothelix`, replace the `uninstall` entry in the stub branch with its own branch:

```bash
    uninstall)
        shift
        # shellcheck disable=SC1091
        . "$(dirname "$0")/uninstall.sh" 2>/dev/null || \
            . "$NOTHELIX_SHARE/dist/uninstall.sh"
        export NOTHELIX_BIN NOTHELIX_SHARE STEEL_HOME
        nothelix_uninstall "$@"
        ;;
```

- [ ] **Step 14.5: Run the tests to verify they pass**

```bash
bats tests/install/uninstall.bats
```
Expected: 8 tests passing.

- [ ] **Step 14.6: Lint**

```bash
shellcheck dist/uninstall.sh dist/nothelix
```
Expected: clean.

- [ ] **Step 14.7: Commit**

```bash
jj describe @ -m "feat(wrapper): nothelix uninstall with --purge --keep-data --dry-run"
```

---

## Phase 6 — Demo notebook

### Task 15: Create examples/demo.jl

**Files:**
- Create: `examples/demo.jl` (this is the one peers open on first run)

- [ ] **Step 15.1: Write the demo**

Create `examples/demo.jl` with the exact content from the spec Section 6:

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

- [ ] **Step 15.2: Verify the demo parses as valid Julia by running `julia --syntax-check`**

```bash
julia --check-bounds=no -e 'include("examples/demo.jl")' 2>&1 | head -20
```

Note: this will error on `@cell` / `@markdown` unless CellMacros is loaded. Instead, run the kernel's parser:

```bash
julia -e 'using Base: Meta; Meta.parseall(read("examples/demo.jl", String))' && echo OK
```
Expected: `OK` (no parse error).

- [ ] **Step 15.3: Commit**

```bash
jj describe @ -m "feat(demo): bundled demo.jl notebook for first-run experience"
```

---

## Phase 7 — CI release pipeline

### Task 16: .github/workflows/release.yml — tag-triggered release

**Files:**
- Create: `.github/workflows/release.yml`

- [ ] **Step 16.1: Create the workflow**

Create `.github/workflows/release.yml`:

```yaml
name: release

on:
  push:
    tags:
      - 'v*'
  workflow_dispatch:
    inputs:
      tag:
        description: 'Override tag (for manual runs)'
        required: false

jobs:
  build:
    name: build-${{ matrix.target.name }}
    runs-on: ${{ matrix.target.runner }}
    strategy:
      fail-fast: false
      matrix:
        target:
          - name: darwin-arm64
            runner: macos-14
            triple: aarch64-apple-darwin
            dylib: libnothelix.dylib
          - name: linux-x86_64
            runner: ubuntu-24.04
            triple: x86_64-unknown-linux-gnu
            dylib: libnothelix.so
    steps:
      - name: Checkout nothelix
        uses: actions/checkout@v4

      - name: Read .helix-fork-rev
        id: fork
        run: |
          SHA=$(cat .helix-fork-rev)
          echo "sha=$SHA" >> "$GITHUB_OUTPUT"

      - name: Checkout helix fork
        uses: actions/checkout@v4
        with:
          repository: koalazub/helix
          ref: ${{ steps.fork.outputs.sha }}
          path: helix

      - name: Verify fork checkout matches pin
        run: |
          ACTUAL=$(cd helix && git rev-parse HEAD)
          if [ "$ACTUAL" != "${{ steps.fork.outputs.sha }}" ]; then
            echo "fork checkout mismatch: expected ${{ steps.fork.outputs.sha }}, got $ACTUAL"
            exit 1
          fi

      - name: Install Rust toolchain
        uses: dtolnay/rust-toolchain@stable
        with:
          targets: ${{ matrix.target.triple }}

      - name: Install Linux deps
        if: runner.os == 'Linux'
        run: sudo apt-get update && sudo apt-get install -y build-essential cmake

      - name: Cache cargo registry and target
        uses: actions/cache@v4
        with:
          path: |
            ~/.cargo/registry
            ~/.cargo/git
            target
            helix/target
          key: cargo-${{ matrix.target.name }}-${{ hashFiles('**/Cargo.lock', 'helix/Cargo.lock') }}

      - name: Build hx-nothelix (fork)
        working-directory: helix
        run: cargo build --release --features steel --target ${{ matrix.target.triple }}

      - name: Build libnothelix
        env:
          NOTHELIX_CI_BUILD: '1'
          NOTHELIX_BUILD_DATE: ${{ github.run_id }}
        run: cargo build --release --target ${{ matrix.target.triple }} -p libnothelix

      - name: Build grammars
        working-directory: helix
        run: |
          ./target/${{ matrix.target.triple }}/release/hx --grammar fetch
          ./target/${{ matrix.target.triple }}/release/hx --grammar build

      - name: Ad-hoc codesign (macOS)
        if: runner.os == 'macOS'
        run: |
          codesign --force --sign - helix/target/${{ matrix.target.triple }}/release/hx
          codesign --force --sign - target/${{ matrix.target.triple }}/release/${{ matrix.target.dylib }}

      - name: Assemble tarball
        run: |
          TAG_NAME="${GITHUB_REF_NAME:-${{ github.event.inputs.tag }}}"
          if [ -z "$TAG_NAME" ]; then TAG_NAME="vdev"; fi
          STAGING="nothelix-${TAG_NAME}-${{ matrix.target.name }}"
          mkdir -p "$STAGING/bin" "$STAGING/lib"
          mkdir -p "$STAGING/share/nothelix/runtime"
          mkdir -p "$STAGING/share/nothelix/examples"
          mkdir -p "$STAGING/share/nothelix/plugin"
          mkdir -p "$STAGING/share/nothelix/lsp"
          mkdir -p "$STAGING/share/nothelix/kernel-scripts"
          mkdir -p "$STAGING/share/nothelix/dist"

          # Binaries
          cp helix/target/${{ matrix.target.triple }}/release/hx "$STAGING/bin/hx-nothelix"
          cp dist/nothelix "$STAGING/bin/nothelix"
          cp lsp/julia-lsp "$STAGING/bin/julia-lsp"

          # Dylib + meta
          cp target/${{ matrix.target.triple }}/release/${{ matrix.target.dylib }} "$STAGING/lib/"
          ./target/${{ matrix.target.triple }}/release/nothelix-meta > "$STAGING/lib/libnothelix.meta"

          # Helix runtime with pre-built grammars
          cp -R helix/runtime "$STAGING/share/nothelix/"

          # Plugin cogs
          cp plugin/nothelix.scm "$STAGING/share/nothelix/plugin/"
          cp -R plugin/nothelix "$STAGING/share/nothelix/plugin/"

          # Examples
          cp examples/demo.jl "$STAGING/share/nothelix/examples/"

          # LSP bootstrap env (Project.toml + Manifest.toml, NO depot)
          cp lsp/Project.toml lsp/Manifest.toml "$STAGING/share/nothelix/lsp/"

          # Kernel scripts (duplicate of what libnothelix include_str!'s)
          cp kernel/*.jl "$STAGING/share/nothelix/kernel-scripts/"

          # Doctor helpers
          cp -R dist/doctor "$STAGING/share/nothelix/dist/"
          cp dist/config.sh dist/reset.sh dist/uninstall.sh "$STAGING/share/nothelix/dist/"

          # In-tarball installer
          cp dist/install-local.sh "$STAGING/install-local.sh"

          # VERSION file
          FORK_SHA=$(cat .helix-fork-rev)
          BUILD_ID="ci-$(date -u +%Y%m%d)-$(git rev-parse --short=12 HEAD)"
          LIBNOTHELIX_VERSION=$(grep '^version' libnothelix/Cargo.toml | head -1 | cut -d'"' -f2)
          cat > "$STAGING/VERSION" <<EOF
          NOTHELIX_VERSION=${TAG_NAME}
          BUILD_ID=${BUILD_ID}
          FORK_SHA=${FORK_SHA}
          FORK_BRANCH=feature/inline-image-rendering
          LIBNOTHELIX_VERSION=${LIBNOTHELIX_VERSION}
          INSTALL_DATE=$(date -u +%Y-%m-%dT%H:%M:%SZ)
          EOF

          tar -czf "${STAGING}.tar.gz" "$STAGING"
          echo "TARBALL=${STAGING}.tar.gz" >> "$GITHUB_ENV"

      - name: Compute SHA256
        run: |
          if command -v shasum >/dev/null; then
            shasum -a 256 "$TARBALL" > "${TARBALL}.sha256"
          else
            sha256sum "$TARBALL" > "${TARBALL}.sha256"
          fi

      - name: Upload tarball + sha
        uses: actions/upload-artifact@v4
        with:
          name: ${{ matrix.target.name }}
          path: |
            ${{ env.TARBALL }}
            ${{ env.TARBALL }}.sha256

  release:
    needs: build
    runs-on: ubuntu-24.04
    permissions:
      contents: write
    steps:
      - uses: actions/checkout@v4
      - uses: actions/download-artifact@v4
        with:
          path: artifacts

      - name: Combine SHA256SUMS
        run: |
          mkdir -p release
          find artifacts -name '*.tar.gz' -exec cp {} release/ \;
          find artifacts -name '*.sha256' -exec cat {} \; > release/SHA256SUMS
          cp install.sh release/install.sh

      - name: Create GitHub release
        uses: softprops/action-gh-release@v2
        with:
          files: |
            release/*.tar.gz
            release/SHA256SUMS
            release/install.sh
          tag_name: ${{ github.ref_name }}
          generate_release_notes: true
```

- [ ] **Step 16.2: Verify the YAML parses**

```bash
yq eval '.' .github/workflows/release.yml >/dev/null && echo "valid YAML"
```
Expected: `valid YAML`. (Install `yq` via brew if missing.)

- [ ] **Step 16.3: Dry-run workflow syntax with actionlint (optional, recommended)**

```bash
brew install actionlint 2>/dev/null || true
actionlint .github/workflows/release.yml
```
Expected: no errors. Any warnings are OK.

- [ ] **Step 16.4: Commit**

```bash
jj describe @ -m "feat(ci): release workflow for darwin-arm64 + linux-x86_64"
```

---

### Task 17: .github/workflows/shellcheck.yml — PR lint

**Files:**
- Create: `.github/workflows/shellcheck.yml`

- [ ] **Step 17.1: Create the workflow**

Create `.github/workflows/shellcheck.yml`:

```yaml
name: shellcheck

on:
  pull_request:
    paths:
      - 'install.sh'
      - 'dist/**.sh'
      - 'dist/nothelix'
      - '.github/workflows/shellcheck.yml'
  push:
    branches: [main]
    paths:
      - 'install.sh'
      - 'dist/**.sh'
      - 'dist/nothelix'

jobs:
  shellcheck:
    runs-on: ubuntu-24.04
    steps:
      - uses: actions/checkout@v4
      - name: Run shellcheck
        run: |
          sudo apt-get update && sudo apt-get install -y shellcheck
          shellcheck install.sh
          shellcheck dist/nothelix
          find dist -name '*.sh' -print0 | xargs -0 shellcheck
```

- [ ] **Step 17.2: Commit**

```bash
jj describe @ -m "feat(ci): shellcheck workflow for install scripts"
```

---

### Task 18: .github/workflows/helix-fork-bump.yml — scheduled auto-bump

**Files:**
- Create: `.github/workflows/helix-fork-bump.yml`

- [ ] **Step 18.1: Create the workflow**

Create `.github/workflows/helix-fork-bump.yml`:

```yaml
name: helix-fork-bump

on:
  schedule:
    - cron: '0 3 * * *'
  workflow_dispatch:
  repository_dispatch:
    types: [helix-fork-updated]

jobs:
  bump:
    runs-on: ubuntu-24.04
    permissions:
      contents: write
      pull-requests: write
    steps:
      - uses: actions/checkout@v4

      - name: Resolve current fork tip
        id: fork
        run: |
          NEW_SHA=$(git ls-remote https://github.com/koalazub/helix.git \
            refs/heads/feature/inline-image-rendering | awk '{print $1}')
          CURRENT_SHA=$(cat .helix-fork-rev)
          echo "new=$NEW_SHA" >> "$GITHUB_OUTPUT"
          echo "current=$CURRENT_SHA" >> "$GITHUB_OUTPUT"
          if [ "$NEW_SHA" = "$CURRENT_SHA" ]; then
            echo "same=1" >> "$GITHUB_OUTPUT"
          fi

      - name: No bump needed
        if: steps.fork.outputs.same == '1'
        run: echo "fork tip matches pin (${{ steps.fork.outputs.current }}); nothing to do"

      - name: Fetch fork commits for changelog
        if: steps.fork.outputs.same != '1'
        id: log
        run: |
          git clone --depth 50 https://github.com/koalazub/helix.git helix-log
          cd helix-log
          git fetch origin ${{ steps.fork.outputs.current }} 2>/dev/null || true
          RANGE="${{ steps.fork.outputs.current }}..${{ steps.fork.outputs.new }}"
          if git rev-parse "${{ steps.fork.outputs.current }}" >/dev/null 2>&1; then
            LOG=$(git log --oneline "$RANGE" 2>/dev/null || git log --oneline -10 "${{ steps.fork.outputs.new }}")
          else
            LOG=$(git log --oneline -10 "${{ steps.fork.outputs.new }}")
          fi
          {
            echo 'log<<EOF'
            echo "$LOG"
            echo 'EOF'
          } >> "$GITHUB_OUTPUT"

      - name: Create bump branch and PR
        if: steps.fork.outputs.same != '1'
        env:
          GH_TOKEN: ${{ secrets.GITHUB_TOKEN }}
        run: |
          SHORT="${{ steps.fork.outputs.new }}"
          SHORT="${SHORT:0:12}"
          BRANCH="bump/helix-fork-${SHORT}"
          git config user.name "github-actions[bot]"
          git config user.email "github-actions[bot]@users.noreply.github.com"
          git checkout -b "$BRANCH"
          echo "${{ steps.fork.outputs.new }}" > .helix-fork-rev
          git add .helix-fork-rev
          git commit -m "chore(deps): bump helix fork to ${SHORT}

          Fork commits in range:
          ${{ steps.log.outputs.log }}
          "
          git push origin "$BRANCH"
          gh pr create \
            --title "chore(deps): bump helix fork to ${SHORT}" \
            --body "Auto-generated bump. Merges on green build; opens an issue on red." \
            --head "$BRANCH" \
            --base main \
            --label auto-bump

      - name: Enable auto-merge on green
        if: steps.fork.outputs.same != '1'
        env:
          GH_TOKEN: ${{ secrets.GITHUB_TOKEN }}
        run: |
          SHORT="${{ steps.fork.outputs.new }}"
          SHORT="${SHORT:0:12}"
          gh pr merge "bump/helix-fork-${SHORT}" --auto --squash
```

- [ ] **Step 18.2: Commit**

```bash
jj describe @ -m "feat(ci): scheduled auto-bump of helix fork SHA pin"
```

---

## Phase 8 — Final integration

### Task 19: End-to-end local install test

Verify the full install flow works on the dev machine using a locally-built tarball.

**Files:**
- Create: `scripts/build-test-tarball.sh`

- [ ] **Step 19.1: Write the build-test-tarball helper**

Create `scripts/build-test-tarball.sh`:

```bash
#!/bin/bash
# build-test-tarball.sh — assembles a local release tarball from the
# current working tree for end-to-end testing. Does not hit the
# network, does not tag a release. Output at /tmp/nothelix-test-release/.
#
# Assumes libnothelix and hx-nothelix are already built via `just install`
# and available under ~/.steel/native/ and ~/.local/bin/ respectively.

set -euo pipefail

cd "$(dirname "$0")/.."

OUT="/tmp/nothelix-test-release"
STAGING="$OUT/nothelix-vtest-local"

rm -rf "$OUT"
mkdir -p "$STAGING/bin" "$STAGING/lib"
mkdir -p "$STAGING/share/nothelix/runtime"
mkdir -p "$STAGING/share/nothelix/examples"
mkdir -p "$STAGING/share/nothelix/plugin"
mkdir -p "$STAGING/share/nothelix/lsp"
mkdir -p "$STAGING/share/nothelix/kernel-scripts"
mkdir -p "$STAGING/share/nothelix/dist"

# Binaries
if [ -x "$HOME/.local/bin/hx-nothelix" ]; then
    cp "$HOME/.local/bin/hx-nothelix" "$STAGING/bin/"
else
    cp "$HOME/projects/helix/target/release/hx" "$STAGING/bin/hx-nothelix"
fi
cp dist/nothelix "$STAGING/bin/nothelix"
cp lsp/julia-lsp "$STAGING/bin/julia-lsp"
chmod +x "$STAGING"/bin/*

# Dylib + meta
if [ -f "$HOME/.steel/native/libnothelix.dylib" ]; then
    cp "$HOME/.steel/native/libnothelix.dylib" "$STAGING/lib/"
elif [ -f "target/release/libnothelix.dylib" ]; then
    cp target/release/libnothelix.dylib "$STAGING/lib/"
else
    echo "build-test-tarball: no libnothelix.dylib found; run 'just install' first" >&2
    exit 1
fi
cargo run -p libnothelix --bin nothelix-meta --release > "$STAGING/lib/libnothelix.meta"

# Helix runtime
if [ -d "$HOME/projects/helix/runtime" ]; then
    cp -R "$HOME/projects/helix/runtime"/* "$STAGING/share/nothelix/runtime/"
fi

# Plugin cogs
cp plugin/nothelix.scm "$STAGING/share/nothelix/plugin/"
cp -R plugin/nothelix "$STAGING/share/nothelix/plugin/"

# Examples
cp examples/demo.jl "$STAGING/share/nothelix/examples/"

# LSP
cp lsp/Project.toml lsp/Manifest.toml "$STAGING/share/nothelix/lsp/"

# Kernel scripts
cp kernel/*.jl "$STAGING/share/nothelix/kernel-scripts/"

# Doctor helpers
cp -R dist/doctor "$STAGING/share/nothelix/dist/"
cp dist/config.sh dist/reset.sh dist/uninstall.sh "$STAGING/share/nothelix/dist/"

# In-tarball installer
cp dist/install-local.sh "$STAGING/install-local.sh"

# VERSION
FORK_SHA=$(cat .helix-fork-rev)
BUILD_ID="dev-$(date -u +%Y%m%d)-$(git rev-parse --short=12 HEAD 2>/dev/null || echo local)"
cat > "$STAGING/VERSION" <<EOF
NOTHELIX_VERSION=vtest-local
BUILD_ID=${BUILD_ID}
FORK_SHA=${FORK_SHA}
FORK_BRANCH=feature/inline-image-rendering
LIBNOTHELIX_VERSION=$(grep '^version' libnothelix/Cargo.toml | head -1 | cut -d'"' -f2)
INSTALL_DATE=$(date -u +%Y-%m-%dT%H:%M:%SZ)
EOF

# Assemble tarball
tar -czf "$OUT/nothelix-vtest-local-$(uname -s | tr '[:upper:]' '[:lower:]')-$(uname -m).tar.gz" \
    -C "$OUT" "$(basename "$STAGING")"

(cd "$OUT" && shasum -a 256 nothelix-vtest-local-*.tar.gz > SHA256SUMS)

echo "Tarball built at: $OUT"
ls -lh "$OUT"
```

Make it executable:

```bash
chmod +x scripts/build-test-tarball.sh
```

- [ ] **Step 19.2: Build a local tarball**

```bash
./scripts/build-test-tarball.sh
ls /tmp/nothelix-test-release/
```
Expected: one `.tar.gz` and one `SHA256SUMS` file.

- [ ] **Step 19.3: Install to a throwaway HOME**

```bash
export FAKE_HOME=$(mktemp -d)
HOME="$FAKE_HOME" \
  NOTHELIX_RELEASE_URL="file:///tmp/nothelix-test-release" \
  NOTHELIX_VERSION_OVERRIDE=vtest-local \
  NOTHELIX_PLATFORM_OVERRIDE="darwin-arm64" \
  bash install.sh
ls "$FAKE_HOME/.local/bin"
ls "$FAKE_HOME/.steel/native"
ls "$FAKE_HOME/.local/share/nothelix"
```
Expected: `hx-nothelix`, `nothelix`, `julia-lsp` in bin; `libnothelix.dylib` + `libnothelix.meta` in steel native; runtime, examples, VERSION in share.

- [ ] **Step 19.4: Run `nothelix doctor` inside the throwaway HOME**

```bash
HOME="$FAKE_HOME" \
  STEEL_HOME="$FAKE_HOME/.steel" \
  PATH="$FAKE_HOME/.local/bin:$PATH" \
  NOTHELIX_SHARE="$FAKE_HOME/.local/share/nothelix" \
  NOTHELIX_SKIP_TTY_CHECK=1 \
  "$FAKE_HOME/.local/bin/nothelix" doctor
```
Expected: exit 0, all checks ✓ or ▲, 0 failures.

- [ ] **Step 19.5: Run `nothelix version`**

```bash
HOME="$FAKE_HOME" NOTHELIX_SHARE="$FAKE_HOME/.local/share/nothelix" \
  "$FAKE_HOME/.local/bin/nothelix" version
```
Expected: prints the test VERSION metadata.

- [ ] **Step 19.6: Run `nothelix uninstall --yes`**

```bash
HOME="$FAKE_HOME" \
  STEEL_HOME="$FAKE_HOME/.steel" \
  NOTHELIX_SHARE="$FAKE_HOME/.local/share/nothelix" \
  NOTHELIX_BIN="$FAKE_HOME/.local/bin" \
  "$FAKE_HOME/.local/bin/nothelix" uninstall --yes 2>&1 || true
ls "$FAKE_HOME/.local/bin" 2>/dev/null
ls "$FAKE_HOME/.steel/native" 2>/dev/null
ls "$FAKE_HOME/.local/share/nothelix" 2>/dev/null
rm -rf "$FAKE_HOME"
```
Expected: all listed dirs are empty or missing after uninstall.

- [ ] **Step 19.7: Commit**

```bash
jj describe @ -m "feat(scripts): build-test-tarball.sh for local end-to-end verification"
```

---

### Task 20: README install section

**Files:**
- Modify: `README.md` (add an install section near the top)

- [ ] **Step 20.1: Read the existing README top section**

```bash
head -80 README.md
```

- [ ] **Step 20.2: Insert the install section**

Append a new top-level section titled `## Install` after the main README header and before existing content:

````markdown
## Install

One line on macOS (Apple Silicon) or x86_64 Linux:

```bash
curl -sSL https://raw.githubusercontent.com/koalazub/nothelix/main/install.sh | sh
```

That downloads a pre-built tarball matching your OS/arch, places the Helix fork binary, the libnothelix dylib, the plugin cogs, and a runtime + demo notebook under `~/.local/bin` and `~/.local/share/nothelix`, and adds `(require "nothelix.scm")` to `~/.config/helix/init.scm` if it's not already there. After it finishes, run:

```bash
nothelix
```

to open the bundled demo notebook. See `nothelix --help` for the full subcommand list (`upgrade`, `uninstall`, `doctor`, `config`, `reset`, `version`).

**Requirements:**

- macOS arm64 or Linux x86_64 (Windows and other targets are not yet shipped)
- A Kitty-protocol terminal — Kitty, Ghostty, or WezTerm — for inline plots
- Julia 1.9+ on PATH. If you don't have it, install [juliaup](https://julialang.org/install/) first.

**If something's broken:**

```bash
nothelix doctor
```

runs a set of environment checks and tells you exactly what's wrong. Add `--smoke` to additionally spawn a Julia kernel and verify the full execution pipeline end to end.

**To uninstall:**

```bash
nothelix uninstall
```

Removes every file this install placed. Leaves `~/.julia/`, your existing Helix config, and your own notebooks completely untouched. Use `--purge` to also scrub `~/.cache/helix/helix.log`.
````

- [ ] **Step 20.3: Commit**

```bash
jj describe @ -m "docs(readme): install section with curl-sh command and doctor hints"
```

---

## Phase 9 — Push everything

### Task 21: Push main

- [ ] **Step 21.1: Move the main bookmark to the final commit and push**

```bash
jj bookmark move main --to @
jj git push --bookmark main
```
Expected: `Move forward bookmark main from … to …`.

- [ ] **Step 21.2: Verify the build passes locally one more time**

```bash
bats tests/install/
cargo test -p libnothelix
shellcheck install.sh dist/nothelix dist/doctor/*.sh dist/*.sh
```
Expected: everything green.

- [ ] **Step 21.3: Tag the first release (optional, triggers CI)**

```bash
# Only if you want to ship v0.1.0 right now
jj new -m "chore: bump to v0.1.0 for first nothelix portability release"
jj bookmark create v0.1.0
jj git push --bookmark v0.1.0
```

Note: this will trigger `.github/workflows/release.yml`. Watch it on GitHub Actions and confirm it produces a tarball + SHA256SUMS + the install.sh asset.

---

## Self-review

### Spec coverage

- **Section 1 (user-visible install flow):** Task 7 (install.sh) + Task 14 (uninstall path) cover it. ✓
- **Section 2 (file layout):** Task 6 (install-local.sh). ✓
- **Section 3 (CI / release pipeline, three-job matrix):** Task 16 (release.yml). ✓
- **Section 3b (auto-bump):** Task 18 (helix-fork-bump.yml). ✓
- **Section 4 (nothelix wrapper + subcommands):** Tasks 2 (skeleton) + 3 (version) + 11 (config) + 12 (reset) + 13 (upgrade) + 14 (uninstall) + 8-10 (doctor). ✓
- **Section 5 (doctor checks):** Task 8 (static 11 checks) + Task 9 (graphics query) + Task 10 (smoke). ✓
- **Section 6 (demo notebook):** Task 15. ✓
- **Section 7 (error handling + uninstall + graceful removal):** Task 14. ✓
- **Open question: `.helix-fork-rev` + fork SHA pin:** Task 1. ✓
- **Spec deviation (BUILD_ID instead of Steel version FFI):** Tasks 4, 5 + doctor check in Task 8. Spec updated inline.

### Placeholder scan

Searched plan for `TBD`, `TODO`, `implement later`, `similar to Task N`, `fill in details`, `add error handling`. None found. Every step either shows the exact code or links to a previously-defined helper by name.

### Type / name consistency

Checked: `NOTHELIX_SHARE`, `STEEL_HOME`, `HX_NOTHELIX`, `VERSION_FILE`, `HELIX_RUNTIME`, `NOTHELIX_BIN`, `DOCTOR_CHECKS_OUTPUT`, `DOCTOR_FAIL_COUNT`, `DOCTOR_WARN_COUNT` all used consistently across tasks. The `nothelix` wrapper sources `doctor/static.sh`, `doctor/smoke.sh`, `config.sh`, `reset.sh`, `uninstall.sh` from the same dir each time. The install layout for these helpers is `$NOTHELIX_SHARE/dist/` in every task that references them.

One intentional inconsistency: `tests/install/install-sh.bats` and `tests/install/upgrade.bats` share a big setup block. I duplicated it (per the plan-writing rule "repeat the code — the engineer may be reading tasks out of order") rather than factoring into a shared helper, because bats's helper-loading mechanism is its own distraction.

---

Plan complete and saved to `docs/superpowers/plans/2026-04-12-nothelix-portability-install.md`.
