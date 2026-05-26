//! Static health checks for the nothelix plugin runtime.
//!
//! Mirrors a cheap subset of `nothelix doctor`'s static checks so the
//! plugin can self-diagnose at load time and surface missing components
//! to the user in-editor (via `set-status!`) instead of degrading
//! silently.
//!
//! Design choices:
//! - Pure Rust, no shell-out, no kernel spawn — runs in microseconds.
//! - All paths derived from environment variables (`STEEL_HOME`,
//!   `NOTHELIX_SHARE`, `NOTHELIX_BIN`, `HOME`) with the same defaults
//!   the shell wrapper at `dist/nothelix` uses, so a healthy install
//!   reports clean and a broken one reports the same issue the shell
//!   doctor would.
//! - Returns TSV (one issue per line, fields tab-separated) so Steel
//!   can parse it with the existing `string-split` utility — no JSON
//!   parser required at startup.
//!
//! Checks implemented:
//! 1. `libnothelix.{dylib,so}` exists in `$STEEL_HOME/native/`.
//! 2. `BUILD_ID` in `libnothelix.meta` matches the one in
//!    `$NOTHELIX_SHARE/VERSION` (catches the case where the dylib was
//!    rebuilt but the wrapper script wasn't, or vice versa).
//! 3. `$STEEL_HOME/cogs/nothelix.scm` + `$STEEL_HOME/cogs/nothelix/`
//!    exist (the symlinks `just install` puts down).
//! 4. The installed `hx-nothelix` (or `hx` fallback) contains the four
//!    fork-only Steel symbols — animation FFI + focus + viewport — so
//!    the user gets a hard pointer at "your binary predates the fork
//!    patches" rather than silent no-ops.

use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HealthIssue {
    pub id: String,
    pub message: String,
    pub fix_hint: String,
}

/// Run all health checks against the supplied resolved paths.
/// Exposed (not just the env-driven wrapper) so tests can drive it with
/// `tempfile::TempDir` and don't have to mutate the process environment.
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
            message: "libnothelix dylib not found in STEEL_HOME/native/".into(),
            fix_hint: "run 'just install' in the nothelix repo (or 'nothelix upgrade')".into(),
        });
    }
}

fn check_build_id(steel_home: &Path, nothelix_share: &Path, issues: &mut Vec<HealthIssue>) {
    let meta = steel_home.join("native/libnothelix.meta");
    let version = nothelix_share.join("VERSION");
    // If either file is missing, dylib-missing or VERSION-missing covers it;
    // mismatch only makes sense when both files exist.
    if !meta.exists() || !version.exists() {
        return;
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
            fix_hint: "run 'nothelix upgrade' to rebuild both halves in lockstep".into(),
        });
    }
}

fn check_plugin_cogs(steel_home: &Path, issues: &mut Vec<HealthIssue>) {
    let entry = steel_home.join("cogs/nothelix.scm");
    let dir = steel_home.join("cogs/nothelix");
    if !entry.exists() || !dir.exists() {
        issues.push(HealthIssue {
            id: "cogs-missing".into(),
            message: "plugin cogs not found in STEEL_HOME/cogs/".into(),
            fix_hint: "run 'just install' to relink the plugin into STEEL_HOME".into(),
        });
    }
}

fn check_fork_symbols(hx_nothelix: &Path, issues: &mut Vec<HealthIssue>) {
    if !hx_nothelix.exists() {
        // No binary at all is a different failure mode; covered by the
        // plugin's own require chain if it gets that far. We'd produce
        // misleading "missing symbols" output here for a perfectly
        // healthy upstream-helix install that just hasn't been pointed
        // at the fork yet.
        return;
    }
    let Ok(bytes) = std::fs::read(hx_nothelix) else {
        return;
    };
    const FORK_SYMBOLS: &[&str] = &[
        "add-or-replace-animating-raw-content",
        "document-focus-gained",
        "document-focus-lost",
        "viewport-changed",
    ];
    let missing: Vec<&&str> = FORK_SYMBOLS
        .iter()
        .filter(|sym| !contains_ascii(&bytes, sym.as_bytes()))
        .collect();
    if !missing.is_empty() {
        let names: Vec<&str> = missing.iter().map(|s| **s).collect();
        issues.push(HealthIssue {
            id: "fork-symbols-missing".into(),
            message: format!(
                "hx-nothelix predates fork patches (missing: {})",
                names.join(", ")
            ),
            fix_hint: "run 'darwin-rebuild switch' (or rebuild ~/projects/helix and copy to ~/.local/bin/hx-nothelix)".into(),
        });
    }
}

fn read_kv(path: &Path, key: &str) -> Option<String> {
    let text = std::fs::read_to_string(path).ok()?;
    let prefix = format!("{key}=");
    for line in text.lines() {
        if let Some(v) = line.strip_prefix(&prefix) {
            return Some(v.trim().to_string());
        }
    }
    None
}

// Naive substring scan; sufficient for the symbol-probe use case and
// avoids pulling in memchr just for this.
fn contains_ascii(haystack: &[u8], needle: &[u8]) -> bool {
    if needle.is_empty() || needle.len() > haystack.len() {
        return false;
    }
    haystack.windows(needle.len()).any(|w| w == needle)
}

/// Resolve the environment-driven defaults the shell wrapper uses.
/// Returns (steel_home, nothelix_share, hx_path).
///
/// `hx_path` resolution: prefer `$NOTHELIX_BIN/hx-nothelix` (the
/// tarball-install layout where the fork binary is shipped under its
/// own name), then fall back to whatever `hx` is first on `$PATH` (the
/// nixoala/home-manager layout where the fork hx is the system hx).
/// If neither exists, return the hx-nothelix path so the
/// fork-symbols check can fail-silent against a non-existent path
/// — that's the correct behaviour for an upstream-only install.
fn resolve_paths() -> (PathBuf, PathBuf, PathBuf) {
    let home = std::env::var("HOME").unwrap_or_default();

    let steel_home = std::env::var("STEEL_HOME")
        .ok()
        .filter(|s| !s.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(&home).join(".steel"));

    let nothelix_share = std::env::var("NOTHELIX_SHARE")
        .ok()
        .filter(|s| !s.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            let xdg = std::env::var("XDG_DATA_HOME")
                .ok()
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| format!("{home}/.local/share"));
            PathBuf::from(xdg).join("nothelix")
        });

    let nothelix_bin = std::env::var("NOTHELIX_BIN")
        .ok()
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| format!("{home}/.local/bin"));
    let hx_nothelix = PathBuf::from(&nothelix_bin).join("hx-nothelix");

    let hx_path = if hx_nothelix.exists() {
        hx_nothelix
    } else if let Some(p) = locate_on_path("hx") {
        p
    } else {
        // No hx anywhere; keep the canonical path so the message is
        // clear (and the check skips per check_fork_symbols' guard).
        hx_nothelix
    };

    (steel_home, nothelix_share, hx_path)
}

/// First `name` found by walking `$PATH`. Symlinks are resolved so the
/// symbol probe runs against the real binary, not a wrapper that
/// re-execs.
fn locate_on_path(name: &str) -> Option<PathBuf> {
    let path = std::env::var("PATH").ok()?;
    for dir in path.split(':') {
        if dir.is_empty() {
            continue;
        }
        let candidate = PathBuf::from(dir).join(name);
        if candidate.exists() {
            return std::fs::canonicalize(&candidate).ok().or(Some(candidate));
        }
    }
    None
}

/// Steel-callable wrapper. Resolves paths from environment + defaults,
/// runs the static checks, and returns a TSV blob the plugin parses.
/// Empty string means "all checks pass".
///
/// TSV format: one line per issue, three tab-separated columns:
///   `<id>\t<message>\t<fix_hint>`
pub fn nothelix_health_check_tsv() -> String {
    let (steel_home, nothelix_share, hx_nothelix) = resolve_paths();
    let issues = run_health_check(&steel_home, &nothelix_share, &hx_nothelix);
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

    // Build a tempdir containing a fully-healthy install layout.
    // Returns (steel_home, nothelix_share, hx_nothelix).
    fn healthy_layout(td: &TempDir) -> (PathBuf, PathBuf, PathBuf) {
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
            // All four fork-only symbols embedded as plain strings.
            "add-or-replace-animating-raw-content \
             document-focus-gained \
             document-focus-lost \
             viewport-changed",
        )
        .unwrap();
        (steel_home, share, hx)
    }

    #[test]
    fn healthy_install_reports_no_issues() {
        let td = TempDir::new().unwrap();
        let (steel, share, hx) = healthy_layout(&td);
        assert!(run_health_check(&steel, &share, &hx).is_empty());
    }

    #[test]
    fn missing_dylib_is_flagged() {
        let td = TempDir::new().unwrap();
        let (steel, share, hx) = healthy_layout(&td);
        fs::remove_file(steel.join("native/libnothelix.dylib")).unwrap();
        let issues = run_health_check(&steel, &share, &hx);
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].id, "dylib-missing");
        assert!(issues[0].fix_hint.contains("just install"));
    }

    #[test]
    fn missing_dylib_accepts_so_fallback() {
        // Linux installs land .so, macOS lands .dylib. Either should satisfy.
        let td = TempDir::new().unwrap();
        let (steel, share, hx) = healthy_layout(&td);
        fs::remove_file(steel.join("native/libnothelix.dylib")).unwrap();
        fs::write(steel.join("native/libnothelix.so"), b"\x00").unwrap();
        let issues = run_health_check(&steel, &share, &hx);
        assert!(issues.iter().all(|i| i.id != "dylib-missing"));
    }

    #[test]
    fn build_id_mismatch_is_flagged() {
        let td = TempDir::new().unwrap();
        let (steel, share, hx) = healthy_layout(&td);
        fs::write(share.join("VERSION"), "BUILD_ID=zzz999\n").unwrap();
        let issues = run_health_check(&steel, &share, &hx);
        assert!(issues.iter().any(|i| i.id == "build-id-mismatch"));
        let issue = issues.iter().find(|i| i.id == "build-id-mismatch").unwrap();
        assert!(issue.message.contains("abc123"));
        assert!(issue.message.contains("zzz999"));
    }

    #[test]
    fn build_id_mismatch_silent_when_files_missing() {
        // If meta.toml or VERSION is missing, dylib-missing reports it
        // first; we don't fabricate a mismatch report. This keeps the
        // surfaced message specific and actionable.
        let td = TempDir::new().unwrap();
        let (steel, share, hx) = healthy_layout(&td);
        fs::remove_file(steel.join("native/libnothelix.meta")).unwrap();
        let issues = run_health_check(&steel, &share, &hx);
        assert!(issues.iter().all(|i| i.id != "build-id-mismatch"));
    }

    #[test]
    fn missing_cogs_is_flagged() {
        let td = TempDir::new().unwrap();
        let (steel, share, hx) = healthy_layout(&td);
        fs::remove_file(steel.join("cogs/nothelix.scm")).unwrap();
        let issues = run_health_check(&steel, &share, &hx);
        assert!(issues.iter().any(|i| i.id == "cogs-missing"));
    }

    #[test]
    fn missing_cogs_dir_is_flagged() {
        let td = TempDir::new().unwrap();
        let (steel, share, hx) = healthy_layout(&td);
        fs::remove_dir_all(steel.join("cogs/nothelix")).unwrap();
        let issues = run_health_check(&steel, &share, &hx);
        assert!(issues.iter().any(|i| i.id == "cogs-missing"));
    }

    #[test]
    fn missing_fork_symbols_is_flagged() {
        let td = TempDir::new().unwrap();
        let (steel, share, hx) = healthy_layout(&td);
        fs::write(&hx, "stub binary contents only — none of the fork symbols").unwrap();
        let issues = run_health_check(&steel, &share, &hx);
        let issue = issues
            .iter()
            .find(|i| i.id == "fork-symbols-missing")
            .expect("expected fork-symbols-missing");
        // The message should name at least one specific missing symbol.
        assert!(
            issue.message.contains("add-or-replace-animating-raw-content"),
            "expected specific symbol in message: {}",
            issue.message
        );
        assert!(issue.fix_hint.contains("darwin-rebuild"));
    }

    #[test]
    fn partial_fork_symbols_lists_only_missing_ones() {
        // Simulate the user's current install: has the animation FFI +
        // focus-lost but missing focus-gained + viewport-changed.
        let td = TempDir::new().unwrap();
        let (steel, share, hx) = healthy_layout(&td);
        fs::write(
            &hx,
            "add-or-replace-animating-raw-content document-focus-lost only",
        )
        .unwrap();
        let issues = run_health_check(&steel, &share, &hx);
        let issue = issues
            .iter()
            .find(|i| i.id == "fork-symbols-missing")
            .expect("expected fork-symbols-missing");
        assert!(issue.message.contains("document-focus-gained"));
        assert!(issue.message.contains("viewport-changed"));
        assert!(!issue.message.contains("add-or-replace-animating-raw-content"));
        assert!(!issue.message.contains("document-focus-lost"));
    }

    #[test]
    fn missing_hx_binary_skips_symbol_check() {
        // An upstream-helix install (no hx-nothelix at all) shouldn't
        // produce a misleading "missing symbols" message.
        let td = TempDir::new().unwrap();
        let (steel, share, hx) = healthy_layout(&td);
        fs::remove_file(&hx).unwrap();
        let issues = run_health_check(&steel, &share, &hx);
        assert!(issues.iter().all(|i| i.id != "fork-symbols-missing"));
    }

    #[test]
    fn tsv_format_round_trips_one_issue() {
        // Drive an unhealthy install through the public TSV wrapper and
        // confirm the format Steel parses (one line, three \t-separated
        // fields, fields free of literal tabs).
        let td = TempDir::new().unwrap();
        let (steel, share, hx) = healthy_layout(&td);
        fs::remove_file(steel.join("native/libnothelix.dylib")).unwrap();
        let issues = run_health_check(&steel, &share, &hx);
        let tsv = issues
            .iter()
            .map(|i| format!("{}\t{}\t{}", i.id, i.message, i.fix_hint))
            .collect::<Vec<_>>()
            .join("\n");
        let line = tsv.lines().next().expect("expected at least one line");
        let parts: Vec<&str> = line.split('\t').collect();
        assert_eq!(parts.len(), 3);
        assert_eq!(parts[0], "dylib-missing");
        assert!(parts[2].contains("just install"));
    }

    #[test]
    fn locate_on_path_finds_existing_binary_via_path() {
        // Drop a binary into a tempdir, point PATH at the dir, and
        // confirm locate_on_path returns the canonicalised path.
        let td = TempDir::new().unwrap();
        let bin_dir = td.path().join("bin");
        fs::create_dir_all(&bin_dir).unwrap();
        let bin_path = bin_dir.join("hx-test-marker");
        fs::write(&bin_path, "#!/bin/sh\nexit 0\n").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = fs::metadata(&bin_path).unwrap().permissions();
            perms.set_mode(0o755);
            fs::set_permissions(&bin_path, perms).unwrap();
        }

        // Save + restore PATH so we don't pollute the test process for
        // sibling tests running in the same binary.
        let prev_path = std::env::var("PATH").ok();
        // SAFETY: tests run single-threaded inside this `cfg(test)`
        // module by default; nextest spawns a fresh process per test
        // group, so env mutation is local.
        unsafe {
            std::env::set_var("PATH", bin_dir.to_string_lossy().as_ref());
        }
        let found = locate_on_path("hx-test-marker");
        match prev_path {
            Some(p) => unsafe { std::env::set_var("PATH", p) },
            None => unsafe { std::env::remove_var("PATH") },
        }

        let found = found.expect("expected locate_on_path to find the marker binary");
        assert!(found.ends_with("hx-test-marker"));
        assert!(found.exists());
    }

    #[test]
    fn locate_on_path_returns_none_for_nonexistent() {
        let prev_path = std::env::var("PATH").ok();
        unsafe { std::env::set_var("PATH", "/var/empty"); }
        let found = locate_on_path("definitely-not-a-real-binary-name-xyz");
        match prev_path {
            Some(p) => unsafe { std::env::set_var("PATH", p) },
            None => unsafe { std::env::remove_var("PATH") },
        }
        assert!(found.is_none());
    }

    #[test]
    fn ffi_wrapper_returns_string() {
        // The FFI wrapper exists and doesn't panic when called with the
        // ambient environment (whatever happens to be set in the test
        // shell). We just verify it's callable and returns a String.
        let out = nothelix_health_check_tsv();
        // Either empty (healthy) or a TSV blob. Empty is fine.
        if !out.is_empty() {
            for line in out.lines() {
                assert_eq!(
                    line.matches('\t').count(),
                    2,
                    "TSV line must have exactly 2 tabs: {line:?}"
                );
            }
        }
    }
}
