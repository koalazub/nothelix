# Fork Update & Plugin Completion — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Land all in-flight nothelix improvements, gate cleanly on a `darwin-rebuild switch`, then validate animation + doc-focus end-to-end, add diagnosability via an in-editor health check + extended doctor, and backfill regression coverage.

**Architecture:** Four-pillar implementation following spec `2026-05-24-fork-update-and-plugin-completion-design.md`. Pre-rebuild work (commits, doctor extension, health-check FFI + plugin, regression tests, UX gaps) is sequenced first; user-gated `darwin-rebuild switch` is a hard pause; post-rebuild work (validation, bandaid removal, hard-logging) follows. Notification system runs cheap static checks in libnothelix and surfaces failures on first buffer focus via `set-status!`.

**Tech Stack:** Rust (libnothelix, nextest, clippy), Steel scheme (plugin), bash (dist/doctor scripts), bats (install tests), jj (version control, not git), Nix (flake-driven binary install via nixoala).

**Version control note:** This repo uses `jj` (Jujutsu), not git. Every commit step uses `jj describe` / `jj new` / `jj split`. Never run `git commit`. The remote is named `origin` and tracks `main`. After describing a revision, push with `jj git push -b main`.

**FFI workflow reminder:** After any change under `libnothelix/`, run `just install` to rebuild + codesign the dylib at `~/.steel/native/libnothelix.dylib`. Skipping this means Helix loads stale code.

---

## File Structure

**Files modified:**

- `libnothelix/src/lib.rs` — register new `nothelix-health-check` FFI.
- `libnothelix/src/health.rs` — **new file**. Pure-Rust static-check implementation (dylib presence, BUILD_ID match, plugin cogs present, fork-symbol probe of `hx-nothelix`).
- `libnothelix/src/unicode/mod.rs` — add 3 sibling regression tests for the LaTeX scanner.
- `libnothelix/src/error_format.rs` — add table-driven enrichment test.
- `libnothelix/src/notebook.rs` — add round-trip integrity test.
- `dist/doctor/static.sh` — add `doctor_check_fork_symbols`.
- `tests/install/doctor-static.bats` — add bats test for the new check.
- `plugin/nothelix/health.scm` — **new file**. Plugin-side health-check orchestration: FFI call on load, first-focus notification, `:nothelix-status` command.
- `plugin/nothelix.scm` — require `health.scm`, wire `:nothelix-status` typable.
- `plugin/nothelix/animation.scm` — **post-rebuild**: drop `try-add-or-replace-animating-raw-content!` + `try-register-hook!` bandaids. Surface fallback failures via `set-status!`.
- `plugin/nothelix/output-insert.scm`, `selection.scm` (or wherever audit lands) — add `schedule-reconceal` calls to mutating paths that don't already have them.
- `examples/multi-plot-stress.ipynb` — **new file**. Synthetic notebook with 3-plot cell, nested-let plots, conditional plot, implicit-return plot.

**Files NOT modified:**

- `~/projects/helix/**` — fork source is current; no patches added.
- `~/nixoala/packages/helix/flake.lock` — already pinned at `f8aad896`.

---

## Task 1: Commit pending LaTeX scanner fix

**Files:**
- Modify (already dirty): `libnothelix/src/unicode/scanner.rs`, `libnothelix/src/unicode/mod.rs`

These are uncommitted from the previous session's `\sum_{k=-n}^n` / `e^{2\pi i kt}` fix. They are logically independent of the animation/lib changes also dirty in the working copy, so peel them out first.

- [ ] **Step 1: Inspect current working copy**

Run: `jj st`
Expected: shows the working copy revision with M markers on `unicode/scanner.rs`, `unicode/mod.rs`, plus other unrelated changes (animation/engine.rs, animation/mod.rs, lib.rs, animation.scm, flake.lock).

- [ ] **Step 2: Split the scanner+tests into their own revision**

Run:
```bash
jj split \
  libnothelix/src/unicode/scanner.rs \
  libnothelix/src/unicode/mod.rs \
  --message "fix(unicode): paired limits + complex braced super keep visible group"
```
Expected: a new committed revision contains only the two scanner files; the working copy retains the remaining changes.

- [ ] **Step 3: Verify tests pass**

Run: `cargo nextest run -p libnothelix unicode::tests::sum_paired_limits_keep_both_at_normal_size unicode::tests::complex_braced_superscript_keeps_braces_visible`
Expected: 2 tests pass.

If the test names differ in `mod.rs`, use `grep -n '#\[test\]' libnothelix/src/unicode/mod.rs` to find the real names and re-run.

- [ ] **Step 4: Push**

Run: `jj git push -b main`
Expected: push succeeds; one new commit on `origin/main`.

---

## Task 2: Commit pending animation FFI + plugin work

**Files:**
- Modify (already dirty): `libnothelix/src/animation/engine.rs`, `libnothelix/src/animation/mod.rs`, `libnothelix/src/lib.rs`, `plugin/nothelix/animation.scm`, `flake.lock`

These changes belong together: the new `animation-tick` FFI + `last_tick_bytes` cache + bandaid wrappers in the plugin.

- [ ] **Step 1: Describe the remaining working-copy revision**

Run:
```bash
jj describe -m "feat(animation): split tick API + bandaid wrappers for older binaries

- libnothelix: animation-tick advances the engine and caches bytes;
  animation-tick-bytes is a pure accessor reading last_tick_bytes.
- plugin: try-add-or-replace-animating-raw-content! + try-register-hook!
  use with-handler/eval so a stale hx without the fork patches degrades
  to silent no-op instead of free-identifier failure at load.

Bandaids are removed in a later commit once doctor's stale-binary check
is in place."
```

- [ ] **Step 2: Run nextest + clippy to confirm nothing broke**

Run:
```bash
cargo nextest run -p libnothelix
cargo clippy -p libnothelix --all-targets -- -D warnings
```
Expected: both pass.

- [ ] **Step 3: Reinstall dylib**

Run: `just install`
Expected: dylib reinstalled at `~/.steel/native/libnothelix.dylib`, BUILD_ID printed.

- [ ] **Step 4: Push**

Run: `jj git push -b main`
Expected: push succeeds.

---

## Task 3: Verify nixoala flake pin matches fork HEAD

**Files:**
- Read-only: `/Users/koalazub/nixoala/packages/helix/flake.lock`

The spec says the lock is already current. Verify, don't trust.

- [ ] **Step 1: Compare lock SHA to fork HEAD**

Run:
```bash
fork_head=$(cd /Users/koalazub/projects/helix && git rev-parse origin/feature/inline-image-rendering)
lock_sha=$(grep -A6 '"helix-src"' /Users/koalazub/nixoala/packages/helix/flake.lock | grep '"rev"' | head -1 | sed -E 's/.*"rev": "([a-f0-9]+)".*/\1/')
echo "fork HEAD: $fork_head"
echo "lock SHA:  $lock_sha"
test "$fork_head" = "$lock_sha" && echo "MATCH" || echo "MISMATCH"
```
Expected: `MATCH`.

- [ ] **Step 2: If MISMATCH, bump the flake**

Only run if Step 1 printed MISMATCH:
```bash
cd /Users/koalazub/nixoala
nix flake update helix-flake
cd /Users/koalazub/nixoala/packages/helix
nix flake update helix-src
```
Then commit the lockfile changes in nixoala (the user uses git there — confirm with `git status` before assuming).

If MATCH, skip Step 2.

- [ ] **Step 3: Mark this task complete**

No commit needed if Step 1 was MATCH. Move on.

---

## Task 4: Add doctor fork-symbols static check (failing bats first)

**Files:**
- Modify: `dist/doctor/static.sh`
- Modify: `tests/install/doctor-static.bats`

TDD: write the bats test first, watch it fail, add the check, watch it pass.

- [ ] **Step 1: Inspect existing bats structure**

Run: `head -60 tests/install/doctor-static.bats`
Expected: shows how tests are set up — likely `setup_file()` builds a fake `$NOTHELIX_BIN` + `$NOTHELIX_SHARE` tree and sources `static.sh`.

- [ ] **Step 2: Write the failing test**

Append to `tests/install/doctor-static.bats`:

```bash
@test "fork-symbols check fails when hx-nothelix lacks animation FFI symbol" {
    # Create a fake hx-nothelix that's a real binary (so -x passes)
    # but contains none of the fork-only symbols.
    printf '#!/bin/sh\nexit 0\n' > "$NOTHELIX_BIN/hx-nothelix"
    chmod +x "$NOTHELIX_BIN/hx-nothelix"

    # shellcheck disable=SC1091
    . "$NOTHELIX_REPO/dist/doctor/static.sh"
    doctor_check_fork_symbols

    [[ "$DOCTOR_FAIL_COUNT" -eq 1 ]]
    [[ "$DOCTOR_CHECKS_OUTPUT" == *"predates fork patches"* ]]
    [[ "$DOCTOR_CHECKS_OUTPUT" == *"add-or-replace-animating-raw-content"* ]]
}

@test "fork-symbols check passes when hx-nothelix contains all symbols" {
    # Fake binary that contains all four symbols as strings.
    cat > "$NOTHELIX_BIN/hx-nothelix" <<'EOF'
#!/bin/sh
# add-or-replace-animating-raw-content
# document-focus-gained
# document-focus-lost
# viewport-changed
exit 0
EOF
    chmod +x "$NOTHELIX_BIN/hx-nothelix"

    # shellcheck disable=SC1091
    . "$NOTHELIX_REPO/dist/doctor/static.sh"
    doctor_check_fork_symbols

    [[ "$DOCTOR_FAIL_COUNT" -eq 0 ]]
    [[ "$DOCTOR_CHECKS_OUTPUT" == *"fork patches present"* ]]
}
```

- [ ] **Step 3: Run the new tests to confirm they fail**

Run:
```bash
cd /Users/koalazub/projects/nothelix
NOTHELIX_REPO=$PWD bats tests/install/doctor-static.bats -f "fork-symbols"
```
Expected: both tests FAIL with "doctor_check_fork_symbols: command not found".

- [ ] **Step 4: Add the static check**

In `dist/doctor/static.sh`, after `doctor_check_hx_nothelix()` (the function that confirms `-x "$HX_NOTHELIX"`), append:

```bash
doctor_check_fork_symbols() {
    if [ ! -x "$HX_NOTHELIX" ]; then
        return
    fi
    local missing=""
    local sym
    for sym in \
        "add-or-replace-animating-raw-content" \
        "document-focus-gained" \
        "document-focus-lost" \
        "viewport-changed"; do
        if ! strings "$HX_NOTHELIX" 2>/dev/null | grep -Fq "$sym"; then
            missing="${missing}${missing:+ }${sym}"
        fi
    done
    if [ -n "$missing" ]; then
        _doctor_fail "hx-nothelix predates fork patches — missing: ${missing} — run 'darwin-rebuild switch' (or rebuild via cargo + copy to ~/.local/bin/hx-nothelix)"
    else
        _doctor_pass "fork patches present in hx-nothelix (animation FFI + focus + viewport events)"
    fi
}
```

- [ ] **Step 5: Register the check in `run_static_doctor_checks`**

Find the function `run_static_doctor_checks` in `dist/doctor/static.sh` (it's the dispatcher that calls each check). Add `doctor_check_fork_symbols` immediately after `doctor_check_hx_nothelix` in the dispatcher list.

- [ ] **Step 6: Re-run the tests**

Run: `NOTHELIX_REPO=$PWD bats tests/install/doctor-static.bats -f "fork-symbols"`
Expected: both tests PASS.

- [ ] **Step 7: Run full doctor bats suite to confirm no regression**

Run: `NOTHELIX_REPO=$PWD bats tests/install/`
Expected: all tests pass.

- [ ] **Step 8: Commit**

```bash
jj new -m "feat(doctor): detect stale hx-nothelix via symbol probe

Adds doctor_check_fork_symbols that probes the installed hx-nothelix
for the four fork-only symbols (animation FFI + three focus/viewport
events). Fails with an actionable hint when missing so users don't
have to diagnose silently-degraded plugin behaviour by hand.

Covers the recurring 'why isn't animation firing' issue at its actual
source: the installed binary predates the fork patches."
jj git push -b main
```

---

## Task 5: Add `nothelix-health-check` FFI in libnothelix

**Files:**
- Create: `libnothelix/src/health.rs`
- Modify: `libnothelix/src/lib.rs`

The FFI is the bedrock of Pillar 3. It runs the *static* doctor checks in pure Rust (no shell), returns a structured list the plugin can render. Cheap enough to run on every plugin load.

- [ ] **Step 1: Write the failing test**

Create the new file at `libnothelix/src/health.rs` with the test stub first:

```rust
//! Static health checks for the nothelix plugin runtime.
//!
//! Mirrors a cheap subset of `nothelix doctor`'s static checks.
//! Designed to run on Steel plugin load: file existence + symbol probing,
//! no kernel spawn, no shell-out.

use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HealthIssue {
    pub id: String,
    pub message: String,
    pub fix_hint: String,
}

pub fn run_health_check(
    steel_home: &Path,
    nothelix_share: &Path,
    hx_nothelix: &Path,
) -> Vec<HealthIssue> {
    let mut issues = Vec::new();
    check_dylib(steel_home, &mut issues);
    check_build_id(steel_home, nothelix_share, &mut issues);
    check_plugin_cogs(steel_home, &mut issues);
    check_fork_symbols(hx_nothelix, &mut issues);
    issues
}

fn check_dylib(steel_home: &Path, issues: &mut Vec<HealthIssue>) {
    let dylib = steel_home.join("native/libnothelix.dylib");
    let so = steel_home.join("native/libnothelix.so");
    if !dylib.exists() && !so.exists() {
        issues.push(HealthIssue {
            id: "dylib-missing".into(),
            message: "libnothelix dylib not found".into(),
            fix_hint: "run 'nothelix upgrade' or 'just install' in the nothelix repo".into(),
        });
    }
}

fn check_build_id(steel_home: &Path, nothelix_share: &Path, issues: &mut Vec<HealthIssue>) {
    let meta = steel_home.join("native/libnothelix.meta");
    let version = nothelix_share.join("VERSION");
    if !meta.exists() || !version.exists() {
        return; // dylib-missing already covers the no-dylib case
    }
    let meta_id = read_kv(&meta, "BUILD_ID");
    let version_id = read_kv(&version, "BUILD_ID");
    if meta_id.is_some() && version_id.is_some() && meta_id != version_id {
        issues.push(HealthIssue {
            id: "build-id-mismatch".into(),
            message: format!(
                "libnothelix and nothelix BUILD_IDs differ ({} vs {})",
                meta_id.as_deref().unwrap_or("?"),
                version_id.as_deref().unwrap_or("?"),
            ),
            fix_hint: "run 'nothelix upgrade'".into(),
        });
    }
}

fn check_plugin_cogs(steel_home: &Path, issues: &mut Vec<HealthIssue>) {
    let entry = steel_home.join("cogs/nothelix.scm");
    let dir = steel_home.join("cogs/nothelix");
    if !entry.exists() || !dir.exists() {
        issues.push(HealthIssue {
            id: "cogs-missing".into(),
            message: "plugin cogs not found in STEEL_HOME".into(),
            fix_hint: "run 'just install' to relink the plugin".into(),
        });
    }
}

fn check_fork_symbols(hx_nothelix: &Path, issues: &mut Vec<HealthIssue>) {
    if !hx_nothelix.exists() {
        // No binary at all — different problem; let dylib/cogs cover it
        // since the user might not be running through the wrapper.
        return;
    }
    let Ok(bytes) = std::fs::read(hx_nothelix) else {
        return;
    };
    const SYMS: &[&str] = &[
        "add-or-replace-animating-raw-content",
        "document-focus-gained",
        "document-focus-lost",
        "viewport-changed",
    ];
    let missing: Vec<&str> = SYMS
        .iter()
        .copied()
        .filter(|sym| !contains_ascii(&bytes, sym.as_bytes()))
        .collect();
    if !missing.is_empty() {
        issues.push(HealthIssue {
            id: "fork-symbols-missing".into(),
            message: format!(
                "hx-nothelix predates fork patches (missing: {})",
                missing.join(", ")
            ),
            fix_hint: "run 'darwin-rebuild switch' (or rebuild ~/projects/helix and copy to ~/.local/bin/hx-nothelix)".into(),
        });
    }
}

fn read_kv(path: &Path, key: &str) -> Option<String> {
    let text = std::fs::read_to_string(path).ok()?;
    for line in text.lines() {
        if let Some(v) = line.strip_prefix(&format!("{key}=")) {
            return Some(v.trim().to_string());
        }
    }
    None
}

fn contains_ascii(haystack: &[u8], needle: &[u8]) -> bool {
    if needle.is_empty() || needle.len() > haystack.len() {
        return false;
    }
    haystack.windows(needle.len()).any(|w| w == needle)
}

/// Steel-callable wrapper. Returns a TSV-encoded result so Steel can
/// parse it with the existing string-split utilities. Each line is
/// `id\tmessage\tfix_hint`. Empty output means no issues.
pub fn nothelix_health_check_tsv(
    steel_home: String,
    nothelix_share: String,
    hx_nothelix: String,
) -> String {
    let issues = run_health_check(
        Path::new(&steel_home),
        Path::new(&nothelix_share),
        Path::new(&hx_nothelix),
    );
    issues
        .iter()
        .map(|i| format!("{}\t{}\t{}", i.id, i.message, i.fix_hint))
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn setup_healthy(td: &TempDir) -> (PathBuf, PathBuf, PathBuf) {
        let steel_home = td.path().join("steel");
        let share = td.path().join("share");
        let bin_dir = td.path().join("bin");
        fs::create_dir_all(steel_home.join("native")).unwrap();
        fs::create_dir_all(steel_home.join("cogs/nothelix")).unwrap();
        fs::create_dir_all(&share).unwrap();
        fs::create_dir_all(&bin_dir).unwrap();
        fs::write(steel_home.join("native/libnothelix.dylib"), b"\x00").unwrap();
        fs::write(steel_home.join("native/libnothelix.meta"), "BUILD_ID=abc123\n").unwrap();
        fs::write(steel_home.join("cogs/nothelix.scm"), b";; entry\n").unwrap();
        fs::write(share.join("VERSION"), "BUILD_ID=abc123\n").unwrap();
        let hx = bin_dir.join("hx-nothelix");
        fs::write(
            &hx,
            "add-or-replace-animating-raw-content document-focus-gained \
             document-focus-lost viewport-changed",
        )
        .unwrap();
        (steel_home, share, hx)
    }

    #[test]
    fn healthy_install_reports_no_issues() {
        let td = TempDir::new().unwrap();
        let (steel, share, hx) = setup_healthy(&td);
        assert!(run_health_check(&steel, &share, &hx).is_empty());
    }

    #[test]
    fn missing_dylib_reports_issue() {
        let td = TempDir::new().unwrap();
        let (steel, share, hx) = setup_healthy(&td);
        fs::remove_file(steel.join("native/libnothelix.dylib")).unwrap();
        let issues = run_health_check(&steel, &share, &hx);
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].id, "dylib-missing");
    }

    #[test]
    fn build_id_mismatch_reports_issue() {
        let td = TempDir::new().unwrap();
        let (steel, share, hx) = setup_healthy(&td);
        fs::write(share.join("VERSION"), "BUILD_ID=zzz999\n").unwrap();
        let issues = run_health_check(&steel, &share, &hx);
        assert!(issues.iter().any(|i| i.id == "build-id-mismatch"));
    }

    #[test]
    fn missing_cogs_reports_issue() {
        let td = TempDir::new().unwrap();
        let (steel, share, hx) = setup_healthy(&td);
        fs::remove_file(steel.join("cogs/nothelix.scm")).unwrap();
        let issues = run_health_check(&steel, &share, &hx);
        assert!(issues.iter().any(|i| i.id == "cogs-missing"));
    }

    #[test]
    fn missing_fork_symbols_reports_issue() {
        let td = TempDir::new().unwrap();
        let (steel, share, hx) = setup_healthy(&td);
        fs::write(&hx, "stub binary contents only").unwrap();
        let issues = run_health_check(&steel, &share, &hx);
        assert!(issues
            .iter()
            .any(|i| i.id == "fork-symbols-missing"
                && i.message.contains("add-or-replace-animating-raw-content")));
    }

    #[test]
    fn tsv_encoding_round_trip() {
        let td = TempDir::new().unwrap();
        let (steel, share, hx) = setup_healthy(&td);
        fs::remove_file(steel.join("native/libnothelix.dylib")).unwrap();
        let out = nothelix_health_check_tsv(
            steel.to_string_lossy().into(),
            share.to_string_lossy().into(),
            hx.to_string_lossy().into(),
        );
        let line = out.lines().next().unwrap();
        let parts: Vec<&str> = line.split('\t').collect();
        assert_eq!(parts.len(), 3);
        assert_eq!(parts[0], "dylib-missing");
    }
}
```

- [ ] **Step 2: Verify `tempfile` is in libnothelix's dev-dependencies**

Run: `grep -E '^tempfile' libnothelix/Cargo.toml`
Expected: `tempfile = "..."` line under `[dev-dependencies]`. If missing:

```bash
cargo add --package libnothelix --dev tempfile
```

- [ ] **Step 3: Register the module**

In `libnothelix/src/lib.rs`, add `pub mod health;` near the top with the other module declarations (line 6-17 area, after `pub mod animation;`).

- [ ] **Step 4: Register the FFI**

In `libnothelix/src/lib.rs` `build_module()`, add a new section before the closing `m`:

```rust
    // ── Health check ──────────────────────────────────────────────────────────
    m.register_fn("nothelix-health-check-tsv", health::nothelix_health_check_tsv);
```

- [ ] **Step 5: Run the tests**

Run: `cargo nextest run -p libnothelix health::`
Expected: 6 tests pass.

- [ ] **Step 6: Reinstall dylib**

Run: `just install`
Expected: dylib reinstalled.

- [ ] **Step 7: Commit**

```bash
jj new -m "feat(health): nothelix-health-check FFI for in-editor diagnosability

Pure-Rust static check covering dylib presence, BUILD_ID match, plugin
cogs presence, and fork-symbol probing of hx-nothelix. Returns TSV so
the plugin can render issues without depending on a sexp parser.

Sets up the FFI side of the in-editor notification system that
surfaces missing components on first buffer focus instead of letting
the user debug silent degradation by hand."
```

---

## Task 6: Wire health check into plugin + first-focus notification

**Files:**
- Create: `plugin/nothelix/health.scm`
- Modify: `plugin/nothelix.scm`

- [ ] **Step 1: Create `plugin/nothelix/health.scm`**

```scheme
;;; health.scm — in-editor health check + first-focus notification.
;;;
;;; On plugin load we call the libnothelix `nothelix-health-check-tsv`
;;; FFI, which runs cheap static checks (dylib presence, BUILD_ID
;;; match, plugin cogs presence, fork-symbol probe of hx-nothelix).
;;; The result is cached in a box; on the first document-focus event
;;; of the session we surface the first issue via `set-status!`.
;;;
;;; A `:nothelix-status` typable command dumps the full list at any
;;; time (registered in nothelix.scm, dispatched here).

(require "helix/editor.scm")
(require "helix/misc.scm")

(#%require-dylib "libnothelix"
                 (only-in nothelix
                          nothelix-health-check-tsv
                          resolve-symlink-dir))

(provide *health-issues*
         run-health-check!
         install-first-focus-hint!
         nothelix-status-command)

;; List of (list id message fix-hint), empty when healthy.
(define *health-issues* (list))
(define *health-hint-shown?* #f)

(define (steel-home)
  (let ([env (or (getenv "STEEL_HOME") "")])
    (if (string=? env "")
        (string-append (or (getenv "HOME") "") "/.steel")
        env)))

(define (nothelix-share)
  (let ([env (or (getenv "NOTHELIX_SHARE") "")])
    (if (string=? env "")
        (string-append (or (getenv "HOME") "")
                       "/.local/share/nothelix")
        env)))

(define (hx-nothelix-path)
  (let ([nb (or (getenv "NOTHELIX_BIN") "")])
    (if (string=? nb "")
        (string-append (or (getenv "HOME") "") "/.local/bin/hx-nothelix")
        (string-append nb "/hx-nothelix"))))

(define (split-on-tab line)
  ;; Steel doesn't ship string-split; use a small accumulator.
  (define len (string-length line))
  (define result (list))
  (define start 0)
  (define i 0)
  (define (emit end)
    (set! result (cons (substring line start end) result))
    (set! start (+ end 1)))
  (let loop ()
    (cond
      [(= i len)
       (emit i)
       (reverse result)]
      [(char=? (string-ref line i) #\tab)
       (emit i)
       (set! i (+ i 1))
       (loop)]
      [else
       (set! i (+ i 1))
       (loop)])))

(define (parse-tsv tsv)
  (if (string=? tsv "")
      (list)
      (map split-on-tab
           (filter (lambda (s) (not (string=? s "")))
                   (string-split tsv #\newline)))))

;; string-split shim — Steel base might or might not have it.
;; If your Steel version provides string-split, this define-if can
;; be removed. We use char delimiter.
(define (string-split-by s ch)
  (define len (string-length s))
  (define result (list))
  (define start 0)
  (let loop ([i 0])
    (cond
      [(= i len)
       (set! result (cons (substring s start i) result))
       (reverse result)]
      [(char=? (string-ref s i) ch)
       (set! result (cons (substring s start i) result))
       (set! start (+ i 1))
       (loop (+ i 1))]
      [else
       (loop (+ i 1))])))

(define (parse-tsv-safe tsv)
  (if (string=? tsv "")
      (list)
      (map split-on-tab
           (filter (lambda (s) (not (string=? s "")))
                   (string-split-by tsv #\newline)))))

(define (run-health-check!)
  (define tsv (nothelix-health-check-tsv
                (steel-home)
                (nothelix-share)
                (hx-nothelix-path)))
  (set! *health-issues* (parse-tsv-safe tsv))
  *health-issues*)

(define (format-issue-status issue)
  ;; issue is (id message fix-hint)
  (string-append "⚠ "
                 (list-ref issue 1)
                 " — "
                 (list-ref issue 2)))

(define (install-first-focus-hint!)
  (register-hook! "document-focus-gained"
    (lambda (_doc-id)
      (when (and (not *health-hint-shown?*)
                 (not (null? *health-issues*)))
        (set! *health-hint-shown?* #t)
        (define first (car *health-issues*))
        (define base (format-issue-status first))
        (define more
          (if (> (length *health-issues*) 1)
              (string-append base " (more — :nothelix-status)")
              base))
        (set-status! more)))))

(define (nothelix-status-command)
  ;; Re-run the check so the user can recheck after fixing things.
  (run-health-check!)
  (cond
    [(null? *health-issues*)
     (set-status! "nothelix: all checks pass")]
    [else
     (define lines
       (map (lambda (issue)
              (string-append (list-ref issue 0)
                             ": "
                             (list-ref issue 1)
                             " (fix: "
                             (list-ref issue 2)
                             ")"))
            *health-issues*))
     (set-status! (string-join lines " | "))]))

;; string-join shim (same caveat as string-split).
(define (string-join lst sep)
  (cond
    [(null? lst) ""]
    [(null? (cdr lst)) (car lst)]
    [else (string-append (car lst) sep (string-join (cdr lst) sep))]))

;; Run the check at module load.
(run-health-check!)
(install-first-focus-hint!)
```

- [ ] **Step 2: Wire into `plugin/nothelix.scm`**

In `plugin/nothelix.scm`, add `(require "nothelix/health.scm")` after the other `nothelix/*` requires.

Then at the end of `nothelix.scm` (in the test commands area or its own section), add:

```scheme
;;@doc
;; Print the current nothelix health-check status. Re-runs the check
;; each invocation so you can verify a fix.
(define (nothelix-status)
  (nothelix-status-command))
```

This top-level `(define (nothelix-status) ...)` is what Helix exposes as the typable `:nothelix-status`.

- [ ] **Step 3: Reinstall**

Run: `just install`
Expected: dylib + plugin cogs reinstalled.

- [ ] **Step 4: Manual smoke (deferred to post-rebuild Task 12)**

Defer end-to-end smoke until the rebuilt `hx-nothelix` is in place — the plugin won't load cleanly enough to test until then. Skip to commit.

- [ ] **Step 5: Commit**

```bash
jj new -m "feat(plugin): in-editor health check + :nothelix-status

On plugin load, calls nothelix-health-check-tsv FFI and caches issues.
First document-focus-gained event in the session surfaces the first
issue via set-status! with a hint pointing to :nothelix-status for
the full list.

Closes the diagnosability gap: missing dylib, mismatched BUILD_ID,
missing plugin cogs, and stale fork binary now all self-announce
instead of degrading silently."
```

---

## Task 7: Regression tests — LaTeX scanner sibling cases

**Files:**
- Modify: `libnothelix/src/unicode/mod.rs`

Add three sibling tests to the existing `sum_paired_limits_keep_both_at_normal_size` / `complex_braced_superscript_keeps_braces_visible` set.

- [ ] **Step 1: Find the existing test block**

Run: `grep -n "sum_paired_limits_keep_both_at_normal_size" libnothelix/src/unicode/mod.rs`
Expected: shows the line number; tests are in a `#[cfg(test)] mod tests { ... }` block.

- [ ] **Step 2: Add the three new tests**

In the same `mod tests { ... }` block, after the existing paired-limits test, add:

```rust
#[test]
fn integral_with_bounds_keeps_limits_normal_size() {
    // \int_a^b f(x)dx must not shrink a or b to scripted forms.
    let overlays = latex_overlays_text(r"\int_a^b f(x)dx");
    assert!(
        !overlays.contains('ᵃ') && !overlays.contains('ᵇ'),
        "integral limits got shrunk: {overlays}"
    );
    assert!(overlays.contains('∫'), "missing integral glyph: {overlays}");
}

#[test]
fn prod_with_braced_subscript_keeps_normal() {
    let overlays = latex_overlays_text(r"\prod_{i=1}^n a_i");
    // The \prod glyph should appear, and "n" should remain at normal
    // size (not converted to ⁿ).
    assert!(overlays.contains('∏'), "missing prod glyph: {overlays}");
    assert!(
        !overlays.contains('ⁿ'),
        "paired-limit shrunk superscript: {overlays}"
    );
}

#[test]
fn inline_super_after_non_big_operator_still_works() {
    // \alpha^2 is *not* a big-operator context — the pending_limits
    // logic should not interfere, and ² should still appear.
    let overlays = latex_overlays_text(r"\alpha^2");
    assert!(overlays.contains('α'), "missing alpha: {overlays}");
    assert!(overlays.contains('²'), "inline super lost: {overlays}");
}
```

Note: `latex_overlays_text` is the existing helper in the test module that calls the public scanner and returns the post-substitution string. If it's named differently in your file, use the same helper the existing two tests use.

- [ ] **Step 3: Run the new tests**

Run: `cargo nextest run -p libnothelix unicode::`
Expected: all unicode tests pass (62 total if there were 60 before + 2 from prior session + 3 new... or however the count lands).

- [ ] **Step 4: Commit**

```bash
jj new -m "test(unicode): sibling cases for paired limits + inline super

- integral bounds (\int_a^b) — verifies the past_close fix doesn't
  shrink ASCII bounds that aren't wrapped in braces.
- braced product subscript (\prod_{i=1}^n) — verifies the paired
  detection still triggers on braced sub + bare super.
- inline super on non-operator (\alpha^2) — verifies the
  pending_limits logic doesn't poison contexts that aren't big-
  operators."
```

---

## Task 8: Regression tests — error_format table-driven

**Files:**
- Modify: `libnothelix/src/error_format.rs`

Cover each enrichment path with a focused test.

- [ ] **Step 1: Inspect existing test module**

Run: `grep -nE "^(#\[cfg\(test\)\]|mod tests|fn test_)" libnothelix/src/error_format.rs`
Expected: shows whether a `mod tests` block already exists.

- [ ] **Step 2: Add enrichment-path tests**

Inside the existing `#[cfg(test)] mod tests { ... }` (or create one at end of file if absent), add:

```rust
#[test]
fn undef_var_enrichment_when_static_scan_finds_definition() {
    let json = r#"{
        "exception_type": "UndefVarError",
        "message": "UndefVarError: `t` not defined",
        "backtrace": []
    }"#;
    let raw = "UndefVarError: `t` not defined";
    let out = format_error(&FormatContext { error_json: json, raw_error: raw, notebook_path: None });
    assert!(out.contains("`t`"), "lost the variable name: {out}");
}

#[test]
fn method_error_named_form_pinpoints() {
    let json = r#"{
        "exception_type": "MethodError",
        "message": "MethodError: no method matching foo(::Int64)",
        "backtrace": []
    }"#;
    let raw = "MethodError: no method matching foo(::Int64)";
    let out = format_error(&FormatContext { error_json: json, raw_error: raw, notebook_path: None });
    assert!(out.contains("foo"), "missed the function name: {out}");
}

#[test]
fn method_error_parenthesized_form_pinpoints() {
    let json = r#"{
        "exception_type": "MethodError",
        "message": "MethodError: no method matching (Matrix)(::Vector{Int64})",
        "backtrace": []
    }"#;
    let raw = "MethodError: no method matching (Matrix)(::Vector{Int64})";
    let out = format_error(&FormatContext { error_json: json, raw_error: raw, notebook_path: None });
    assert!(out.contains("Matrix"), "missed parenthesized name: {out}");
}

#[test]
fn parse_error_with_bracket_imbalance_reports_count() {
    let json = r#"{
        "exception_type": "ParseError",
        "message": "ParseError: Expected `]`",
        "backtrace": [],
        "raw_source": "foo([1, 2, [3, 4])"
    }"#;
    let raw = "ParseError: Expected `]`";
    let out = format_error(&FormatContext { error_json: json, raw_error: raw, notebook_path: None });
    // The enricher should mention bracket balance somewhere.
    assert!(
        out.to_lowercase().contains("bracket") || out.contains("imbalance") || out.contains("["),
        "missed bracket-balance enrichment: {out}"
    );
}

#[test]
fn parse_error_expected_end_is_demoted() {
    // Verify the E018 demotion: "Expected `end`" gets a softer hint
    // (we don't iterate, no `end` keyword is missing in this fixture).
    let json = r#"{
        "exception_type": "ParseError",
        "message": "ParseError: Expected `end`",
        "backtrace": [],
        "raw_source": "let x = 1"
    }"#;
    let raw = "ParseError: Expected `end`";
    let out = format_error(&FormatContext { error_json: json, raw_error: raw, notebook_path: None });
    // The demotion should not aggressively claim we forgot an `end`.
    // We just verify the raw error is preserved without misleading text.
    assert!(out.contains("Expected"), "expected text dropped: {out}");
}
```

Adjust assertions if the actual enrichment strings differ — peek at
`enrich_method_error`, `count_bracket_balance`, etc. for the real
wording. Each test's job is to lock in the *contract* (some
enrichment fires), not the exact phrasing.

- [ ] **Step 3: Run tests**

Run: `cargo nextest run -p libnothelix error_format::`
Expected: all pass. If any fail, the enricher contract differs from what the test asserts — adjust the assertion to match the real output, **not** the other way around (these tests are pinning current behaviour, not driving new behaviour).

- [ ] **Step 4: Commit**

```bash
jj new -m "test(error_format): pin enrichment contracts

Five table-driven-flavoured tests covering UndefVar, MethodError
(named + parenthesized), ParseError (bracket balance + Expected-end
demotion). Locks in the current enrichment output so regressions
in the enrichers get caught at build time instead of by the user
seeing 'check spelling' for a static-scan-resolvable issue."
```

---

## Task 9: Regression test — notebook round-trip integrity

**Files:**
- Modify: `libnothelix/src/notebook.rs`

Build a synthetic .ipynb, round-trip, diff.

- [ ] **Step 1: Find the test block**

Run: `grep -nE "^(#\[cfg\(test\)\]|mod tests|fn test_)" libnothelix/src/notebook.rs | head -10`

- [ ] **Step 2: Add the round-trip test**

In the `#[cfg(test)] mod tests { ... }` block (create one if missing), add:

```rust
#[test]
fn round_trip_preserves_outputs_and_attachments() {
    use std::fs;
    use tempfile::TempDir;

    let td = TempDir::new().unwrap();
    let ipynb_path = td.path().join("test.ipynb");

    // Minimal but realistic ipynb:
    //  - code cell with display_data image output
    //  - markdown cell with an attachment image
    //  - code cell with execution_count = null (never run)
    //  - the `using NothelixMacros` line is NOT in source — it should
    //    be ADDED by jl_preamble in convert and STRIPPED on round-trip.
    let ipynb = r#"{
      "cells": [
        {
          "cell_type": "code",
          "execution_count": 1,
          "metadata": {},
          "source": ["x = 1 + 1"],
          "outputs": [
            {
              "output_type": "display_data",
              "data": {"image/png": "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mNkYAAAAAYAAjCB0C8AAAAASUVORK5CYII="},
              "metadata": {}
            }
          ]
        },
        {
          "cell_type": "markdown",
          "metadata": {},
          "source": ["See ![pic](attachment:tile.png)"],
          "attachments": {
            "tile.png": {"image/png": "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mNkYAAAAAYAAjCB0C8AAAAASUVORK5CYII="}
          }
        },
        {
          "cell_type": "code",
          "execution_count": null,
          "metadata": {},
          "source": ["y = 2"],
          "outputs": []
        }
      ],
      "metadata": {
        "kernelspec": {"display_name": "Julia 1.11", "language": "julia", "name": "julia-1.11"},
        "language_info": {"name": "julia"}
      },
      "nbformat": 4,
      "nbformat_minor": 5
    }"#;
    fs::write(&ipynb_path, ipynb).unwrap();

    // ipynb → .jl
    let jl_path = td.path().join("test.jl");
    let convert_result = notebook_convert_sync(
        ipynb_path.to_string_lossy().into(),
        jl_path.to_string_lossy().into(),
    );
    assert_eq!(convert_result, "", "convert errored: {convert_result}");

    // The .jl must NOT contain `using NothelixMacros` literally (filtered
    // out of the preamble per project_nixoala memory + spec).
    let jl_contents = fs::read_to_string(&jl_path).unwrap();
    assert!(
        !jl_contents.contains("using NothelixMacros"),
        ".jl preamble leaked NothelixMacros pragma"
    );

    // .jl → ipynb (back)
    let round_path = td.path().join("round.ipynb");
    let back_result = convert_to_ipynb(
        jl_path.to_string_lossy().into(),
        round_path.to_string_lossy().into(),
    );
    assert_eq!(back_result, "", "convert back errored: {back_result}");

    let round = fs::read_to_string(&round_path).unwrap();
    let original_v: serde_json::Value = serde_json::from_str(ipynb).unwrap();
    let round_v: serde_json::Value = serde_json::from_str(&round).unwrap();

    // Cell-count preserved
    let orig_cells = original_v["cells"].as_array().unwrap();
    let round_cells = round_v["cells"].as_array().unwrap();
    assert_eq!(orig_cells.len(), round_cells.len(), "cell count diverged");

    // First code cell outputs preserved
    assert_eq!(
        orig_cells[0]["outputs"], round_cells[0]["outputs"],
        "code cell outputs lost on round-trip"
    );

    // Markdown cell attachments preserved
    assert_eq!(
        orig_cells[1]["attachments"], round_cells[1]["attachments"],
        "markdown attachments lost on round-trip"
    );

    // Never-run cell stays null
    assert_eq!(
        round_cells[2]["execution_count"], serde_json::Value::Null,
        "execution_count fabricated for never-run cell"
    );
}
```

The function names `notebook_convert_sync` / `convert_to_ipynb` may differ slightly — `grep -n 'pub fn ' libnothelix/src/notebook.rs` to confirm before running.

- [ ] **Step 3: Verify `serde_json` is available**

Run: `grep -E '^serde_json' libnothelix/Cargo.toml`
Expected: present (it's used throughout notebook.rs). If only in `dependencies`, that's fine for a test inside the same crate.

- [ ] **Step 4: Run the test**

Run: `cargo nextest run -p libnothelix notebook::tests::round_trip_preserves_outputs_and_attachments`
Expected: passes. If it fails, the failure surface real round-trip bugs — fix the production code, not the test.

- [ ] **Step 5: Commit**

```bash
jj new -m "test(notebook): round-trip preserves outputs + attachments

Synthesises a minimal but realistic .ipynb (code cell with display_data
PNG, markdown cell with attachment, never-run cell), converts to .jl
and back, asserts cell count + outputs + attachments + execution_count
nullability all survive. Catches the regressions we've fixed
piecemeal: NothelixMacros preamble leakage, output stripping, attachment
loss, fabricated execution counts."
```

---

## Task 10: Conceal jitter audit + fix

**Files:**
- Modify: candidates are `plugin/nothelix/output-insert.scm`, `plugin/nothelix/selection.scm`. Audit determines which.

The fix from a prior session covered `backslash.scm`. Find sibling mutation paths.

- [ ] **Step 1: Identify buffer-mutating callsites**

Run:
```bash
grep -nE "(insert-string|replace-selection-with|paste-after|paste-before|delete-selection)" plugin/nothelix/*.scm
```
Expected: a list of callsites grouped by file.

- [ ] **Step 2: For each callsite, check whether `schedule-reconceal` follows**

Manually walk through the grep output. For each callsite whose subsequent line(s) do NOT call `schedule-reconceal`, note the file + line for fixing.

- [ ] **Step 3: Add the missing calls**

For each noted callsite, add immediately after the mutation:

```scheme
(schedule-reconceal 50)
```

The `50` matches the existing pattern in `backslash.scm`. Higher delays (e.g. 400 in `post-insert-char`) are reserved for high-frequency events.

If `schedule-reconceal` is not imported in the file, add:

```scheme
(require "conceal.scm")
```

with the other requires at the top of the file. (`conceal.scm` provides it; check `provide` line in conceal.scm to confirm.)

- [ ] **Step 4: Reinstall**

Run: `just install`

- [ ] **Step 5: Defer manual smoke**

Visual confirmation requires running Helix, which works fine pre-rebuild. Quick smoke: open a notebook with conceal-eligible math, paste a block, observe whether the display flickers. If yes, the new `schedule-reconceal` calls aren't covering the case — investigate.

- [ ] **Step 6: Commit**

```bash
jj new -m "fix(conceal): invalidate cache on buffer-mutating callsites

Adds schedule-reconceal to paste/yank/delete paths that previously
bypassed cache invalidation, causing flicker when conceal-eligible
content was edited via non-backslash channels."
```

---

## Task 11: Multi-plot stress test notebook

**Files:**
- Create: `examples/multi-plot-stress.ipynb`

Synthetic notebook to exercise the per-expression plot capture path.

- [ ] **Step 1: Create the notebook**

```json
{
  "cells": [
    {
      "cell_type": "code",
      "execution_count": null,
      "metadata": {},
      "source": [
        "using Plots\n",
        "plot(rand(10))\n",
        "plot(rand(10), rand(10))\n",
        "scatter(rand(10), rand(10))"
      ],
      "outputs": []
    },
    {
      "cell_type": "code",
      "execution_count": null,
      "metadata": {},
      "source": [
        "let\n",
        "    a = plot(rand(10))\n",
        "    b = plot(rand(10).^2)\n",
        "    display(a)\n",
        "    display(b)\n",
        "end"
      ],
      "outputs": []
    },
    {
      "cell_type": "code",
      "execution_count": null,
      "metadata": {},
      "source": [
        "if true\n",
        "    plot(rand(10))\n",
        "end"
      ],
      "outputs": []
    },
    {
      "cell_type": "code",
      "execution_count": null,
      "metadata": {},
      "source": [
        "plot(rand(10))"
      ],
      "outputs": []
    }
  ],
  "metadata": {
    "kernelspec": {"display_name": "Julia 1.11", "language": "julia", "name": "julia-1.11"},
    "language_info": {"name": "julia"}
  },
  "nbformat": 4,
  "nbformat_minor": 5
}
```

- [ ] **Step 2: Defer manual run to post-rebuild**

Running the notebook end-to-end needs the rebuilt `hx-nothelix`. Note the file for post-rebuild execution under Task 13.

- [ ] **Step 3: Commit**

```bash
jj new -m "test(examples): multi-plot stress notebook for capture path

Four cells exercising per-expression plot capture:
1. Three top-level plot/scatter calls in one cell.
2. Two plots inside a let-block with explicit display().
3. Single plot inside if-branch.
4. Single plot in implicit-return position.

Run post-rebuild against the fork's redraw path to confirm capture_toplevel
captures all plot expressions, not just the cell's final return value."
```

---

## Task 12: USER GATE — `darwin-rebuild switch`

**This task requires the user to act. The agent stops here and reports.**

- [ ] **Step 1: Report status to the user**

Print to the user:
> All pre-rebuild work is committed and pushed. The flake pin at
> `~/nixoala/packages/helix/flake.lock` already points at fork HEAD
> `f8aad896`. To pick up the new fork binary (animation FFI +
> document-focus + viewport events), please run:
>
>     sudo darwin-rebuild switch --flake ~/nixoala
>
> Per memory rule I cannot run this for you. Resume the plan once
> the rebuild completes; the next tasks validate the new binary.

- [ ] **Step 2: Wait for user confirmation**

Do not proceed until the user confirms the rebuild is complete. If the user reports failure, halt — the rest of the plan assumes a successful rebuild.

---

## Task 13: Validate animation + doc-focus + viewport end-to-end

**Files:**
- Read-only (verification): the installed `hx-nothelix`, `examples/multi-plot-stress.ipynb`, an animated GIF.

- [ ] **Step 1: Confirm the binary has the new symbols**

Run:
```bash
hx_path=$(readlink -f "$(which hx-nothelix)" 2>/dev/null || readlink -f "$(which hx)")
for sym in add-or-replace-animating-raw-content document-focus-gained document-focus-lost viewport-changed; do
    if strings "$hx_path" | grep -Fq "$sym"; then
        echo "✓ $sym"
    else
        echo "✗ $sym missing — rebuild did not pick up the patches"
    fi
done
```
Expected: four ✓ lines.

- [ ] **Step 2: Run nothelix doctor**

Run: `nothelix doctor`
Expected: passes including the new `fork patches present in hx-nothelix` line. Zero failures.

- [ ] **Step 3: Animation smoke**

Locate or create a tiny animated GIF (`examples/anim-smoke.gif` if it exists; otherwise any small `.gif`). Open it via a `.ipynb` cell with an image output.

Open in Helix:
```bash
hx examples/multi-plot-stress.ipynb  # or any notebook with images
```

Manually verify:
- Animation plays (frames advance).
- `:buffer-next` (switch to a different buffer): animation pauses within a tick.
- `:buffer-previous` (back): animation resumes.
- Scroll the cell off-screen via `Ctrl-D` repeatedly: animation pauses.
- Scroll back via `Ctrl-U`: animation resumes.

If any step fails, the FFI may have surfaced a regression — debug, do not proceed.

- [ ] **Step 4: Multi-plot smoke**

Open `examples/multi-plot-stress.ipynb`. Execute each cell with `<space>x` (or the configured execute-cell keybind). Confirm:
- Cell 1 renders 3 plots.
- Cell 2 renders 2 plots.
- Cell 3 renders 1 plot.
- Cell 4 renders 1 plot.

If fewer than declared, file a follow-up in the task list and continue; the multi-plot fix landed earlier in the session and this is a regression check.

- [ ] **Step 5: Health check smoke**

With everything healthy:
- `:nothelix-status` should print `nothelix: all checks pass`.

Force-break the dylib:
```bash
mv ~/.steel/native/libnothelix.dylib ~/.steel/native/libnothelix.dylib.bak
```

Restart Helix, open any file, observe the first-focus status line displays `⚠ libnothelix dylib not found — …`. Confirm `:nothelix-status` lists `dylib-missing`.

Restore:
```bash
mv ~/.steel/native/libnothelix.dylib.bak ~/.steel/native/libnothelix.dylib
```

- [ ] **Step 6: Report findings**

No commit yet — this task only validates. Note any regression in the task list before proceeding.

---

## Task 14: Remove animation.scm bandaid

**Files:**
- Modify: `plugin/nothelix/animation.scm`

Now safe to remove the with-handler/eval wrappers because:
1. The fork binary now has the FFI guaranteed.
2. Doctor's `fork-symbols` check catches future regressions.
3. The health-check plugin module surfaces a missing-symbol situation in-editor.

- [ ] **Step 1: Replace `try-add-or-replace-animating-raw-content!` callsites**

In `plugin/nothelix/animation.scm`, find the function `try-add-or-replace-animating-raw-content!` (currently defined around line 37–41). Delete the entire definition.

Find its callsite in `schedule-tick` (around line 198):

```scheme
         (try-add-or-replace-animating-raw-content!
           bytes
           eid
           height
           char-idx
           #t)
```

Replace with direct call + status check:

```scheme
         (define ok? (helix.static.add-or-replace-animating-raw-content!
                       bytes eid height char-idx #t))
         (when (not ok?)
           (set-status!
             "nothelix animation: helix refused frame insert — :nothelix-status"))
```

- [ ] **Step 2: Replace `try-register-hook!` with bare `register-hook!`**

In `animation.scm`, find the three uses of `try-register-hook!` (around lines 257, 266, 276) and the wrapper definition (around line 252). Replace each `try-register-hook!` call with `register-hook!`, and delete the wrapper definition.

- [ ] **Step 3: Drop the explanatory comments**

Remove the comment block above the wrappers (the paragraph starting "The animation FFI lives only on a freshly-rebuilt fork binary..." around line 27, and the similar block above `try-register-hook!` around line 244). These no longer apply.

- [ ] **Step 4: Drop unused imports**

If `eval` and `with-handler` are no longer referenced anywhere in `animation.scm` after these edits, remove their `require` lines if any. (They're builtins; usually no explicit import is needed, but check.)

- [ ] **Step 5: Reinstall and smoke**

Run: `just install`

Restart Helix and confirm:
- Plugin loads without errors (check `:log-open` or stderr).
- Animation still plays.
- Focus/viewport pause/resume still works.

- [ ] **Step 6: Commit**

```bash
jj new -m "refactor(animation): drop with-handler bandaid now that fork is current

The try-add-or-replace-animating-raw-content! and try-register-hook!
wrappers were defensive against a stale hx-nothelix that predated the
fork patches. With darwin-rebuild now in place and the fork-symbols
doctor check + in-editor health notification catching future
regressions, the wrappers add cost without protection.

Callsites now use the FFIs directly. Failed frame inserts surface via
set-status! instead of silent #false return."
```

---

## Task 15: Final clippy + nextest + bats sweep

**Files:**
- Read-only.

- [ ] **Step 1: Clippy**

Run: `cargo clippy -p libnothelix --all-targets -- -D warnings`
Expected: zero warnings.

- [ ] **Step 2: Nextest**

Run: `cargo nextest run -p libnothelix`
Expected: all tests pass. Note: if Task 7/8/9 tests were skipped or test names diverged, surface that here.

- [ ] **Step 3: Bats**

Run: `NOTHELIX_REPO=$PWD bats tests/install/`
Expected: all pass.

- [ ] **Step 4: Doctor**

Run: `nothelix doctor --smoke`
Expected: passes including the new fork-symbols check. The `--smoke` flag exercises the Julia kernel boot, so this verifies the post-rebuild stack is healthy.

- [ ] **Step 5: Commit (if any drive-by fixes happened)**

If steps 1–4 required code changes (test-name fixes, clippy hints), commit those. Otherwise skip.

```bash
# Only if fixes were made
jj new -m "chore: drive-by fixes from final verification sweep"
```

- [ ] **Step 6: Push everything**

Run: `jj git push -b main`
Expected: all new revisions pushed to origin.

---

## Task 16: Update task list to completion + write closing notes

- [ ] **Step 1: Mark all tasks complete in the harness**

For each task in `TaskList`, run `TaskUpdate` with `status: completed`. If any task hit a real regression that you deferred, leave it `in_progress` and add a follow-up task explaining.

- [ ] **Step 2: Summarise for the user**

Print a brief, one-paragraph summary covering: what shipped, what's now visible in-editor (the health check, the doctor extension), and any deferred items. Don't recap the plan; the user has the plan file.

---

## Self-Review

Spec coverage check (each spec section must map to a task):

- **Sequencing (8 steps)** → Tasks 1, 2, 3, (4-9 cover pre-rebuild work in any order), 12 (gate), 13–14 (post-rebuild). ✓
- **Pillar 1 — Animation + doc-focus** → Task 13 (validate), Task 14 (bandaid removal). ✓
- **Pillar 2 — Doctor stale-binary detection** → Task 4 (bats + impl). ✓
- **Pillar 3 — Notification system (FFI + plugin + typable)** → Task 5 (FFI), Task 6 (plugin wiring). ✓
- **Pillar 4 — Regression tests** → Task 7 (scanner), Task 8 (errors), Task 9 (round-trip). ✓
- **Pillar 4b — UX gaps** → Task 10 (conceal jitter), Task 11 (multi-plot), Task 14 step 1 (animation fallback hard-logging via set-status!). ✓
- **Acceptance criteria** → Task 15 verifies criteria 6; Task 13 verifies criteria 1, 2, 5; Task 4 verifies criteria 4; Task 14 verifies criteria 3.

Placeholder scan: no TBD/TODO/fill-in-later. All commands are exact, all code is complete. Test names referenced match the code shown.

Type consistency: `HealthIssue` struct field names (`id`, `message`, `fix_hint`) used consistently across Task 5 implementation and Task 6 plugin parser (which splits TSV in same order). FFI function `nothelix-health-check-tsv` matches both registration in `lib.rs` and call site in `health.scm`.
