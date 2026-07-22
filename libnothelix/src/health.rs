use std::fmt;
use std::path::{Path, PathBuf};

const FORK_INDICATORS: &[&str] = &[
    "add-or-replace-animating-raw-content",
    "DocumentFocusGained",
    "ViewportChanged",
    "TerminalFocusGained",
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HealthIssue {
    pub id: String,
    pub message: String,
    pub fix_hint: String,
}

impl HealthIssue {
    fn new(id: &str, message: impl Into<String>, fix_hint: impl Into<String>) -> Self {
        Self {
            id: id.to_owned(),
            message: message.into(),
            fix_hint: fix_hint.into(),
        }
    }
}

impl fmt::Display for HealthIssue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}\t{}\t{}", self.id, self.message, self.fix_hint)
    }
}

struct Install {
    steel_home: PathBuf,
    nothelix_share: PathBuf,
    hx: PathBuf,
}

impl Install {
    fn at(
        steel_home: impl Into<PathBuf>,
        nothelix_share: impl Into<PathBuf>,
        hx: impl Into<PathBuf>,
    ) -> Self {
        Self {
            steel_home: steel_home.into(),
            nothelix_share: nothelix_share.into(),
            hx: hx.into(),
        }
    }

    fn from_env() -> Self {
        let home = std::env::var("HOME").unwrap_or_default();
        let nothelix_bin =
            env_nonempty("NOTHELIX_BIN").unwrap_or_else(|| format!("{home}/.local/bin"));
        let hx_nothelix = PathBuf::from(&nothelix_bin).join("hx-nothelix");

        Self::at(
            env_nonempty("STEEL_HOME")
                .map_or_else(|| PathBuf::from(&home).join(".steel"), PathBuf::from),
            env_nonempty("NOTHELIX_SHARE").map_or_else(
                || {
                    let xdg = env_nonempty("XDG_DATA_HOME")
                        .unwrap_or_else(|| format!("{home}/.local/share"));
                    PathBuf::from(xdg).join("nothelix")
                },
                PathBuf::from,
            ),
            if hx_nothelix.exists() {
                hx_nothelix
            } else {
                locate_on_path("hx").unwrap_or(hx_nothelix)
            },
        )
    }

    fn issues(&self) -> Vec<HealthIssue> {
        [
            self.dylib_present(),
            self.build_ids_agree(),
            self.plugin_cogs_present(),
            self.ffi_versions_agree(),
            self.fork_symbols_present(),
        ]
        .into_iter()
        .flatten()
        .collect()
    }

    fn dylib_present(&self) -> Option<HealthIssue> {
        let native = self.steel_home.join("native");
        let found = ["libnothelix.dylib", "libnothelix.so"]
            .iter()
            .any(|name| native.join(name).exists());
        (!found).then(|| {
            HealthIssue::new(
                "dylib-missing",
                "libnothelix dylib not found in STEEL_HOME/native/",
                "run 'just install' in the nothelix repo (or 'nothelix upgrade')",
            )
        })
    }

    fn build_ids_agree(&self) -> Option<HealthIssue> {
        let meta = self.steel_home.join("native/libnothelix.meta");
        let version = self.nothelix_share.join("VERSION");
        if !meta.exists() || !version.exists() {
            return None;
        }
        let dylib_id = read_kv(&meta, "BUILD_ID")?;
        let wrapper_id = read_kv(&version, "BUILD_ID")?;
        (dylib_id != wrapper_id).then(|| {
            HealthIssue::new(
                "build-id-mismatch",
                format!("libnothelix and nothelix BUILD_IDs differ ({dylib_id} vs {wrapper_id})"),
                "run 'nothelix upgrade' to rebuild both halves in lockstep",
            )
        })
    }

    fn plugin_installed(&self) -> bool {
        let cogs = self.steel_home.join("cogs");
        ["nothelix.scm", "nothelix"]
            .iter()
            .all(|name| cogs.join(name).exists())
    }

    fn plugin_cogs_present(&self) -> Option<HealthIssue> {
        (!self.plugin_installed()).then(|| {
            HealthIssue::new(
                "cogs-missing",
                "plugin cogs not found in STEEL_HOME/cogs/",
                "run 'just install' to relink the plugin into STEEL_HOME",
            )
        })
    }

    fn ffi_versions_agree(&self) -> Option<HealthIssue> {
        if !self.plugin_installed() {
            return None;
        }
        let declared =
            std::fs::read_to_string(self.steel_home.join("cogs/nothelix/ffi-version.scm"))
                .ok()
                .and_then(|text| scan_expected_ffi_version(&text))
                .unwrap_or(0);
        (declared != crate::NOTHELIX_FFI_VERSION).then(|| {
            HealthIssue::new(
                "ffi-version-mismatch",
                format!(
                    "libnothelix FFI v{}, plugin expects v{declared}",
                    crate::NOTHELIX_FFI_VERSION
                ),
                "run 'just install' to rebuild the dylib against the live-linked plugin",
            )
        })
    }

    fn fork_symbols_present(&self) -> Option<HealthIssue> {
        if !self.hx.exists() {
            return None;
        }
        let bytes = std::fs::read(&self.hx).ok()?;
        let missing: Vec<&str> = FORK_INDICATORS
            .iter()
            .copied()
            .filter(|symbol| !contains_ascii(&bytes, symbol.as_bytes()))
            .collect();
        (!missing.is_empty()).then(|| {
            HealthIssue::new(
                "fork-symbols-missing",
                format!(
                    "hx-nothelix predates fork patches (missing: {})",
                    missing.join(", ")
                ),
                "run 'darwin-rebuild switch' (or rebuild ~/projects/helix and copy to ~/.local/bin/hx-nothelix)",
            )
        })
    }
}

fn scan_expected_ffi_version(text: &str) -> Option<u32> {
    for line in text.lines() {
        let code = line.split_once(';').map_or(line, |(code, _)| code);
        let tokens: Vec<&str> = code
            .split(|c: char| c.is_whitespace() || c == '(' || c == ')')
            .filter(|t| !t.is_empty())
            .collect();
        for window in tokens.windows(3) {
            if window[0] == "define"
                && window[1] == "EXPECTED-FFI-VERSION"
                && let Ok(version) = window[2].parse()
            {
                return Some(version);
            }
        }
    }
    None
}

fn read_kv(path: &Path, key: &str) -> Option<String> {
    let text = std::fs::read_to_string(path).ok()?;
    let prefix = format!("{key}=");
    text.lines()
        .find_map(|line| line.strip_prefix(&prefix))
        .map(|value| value.trim().to_owned())
}

fn contains_ascii(haystack: &[u8], needle: &[u8]) -> bool {
    if needle.is_empty() || needle.len() > haystack.len() {
        return false;
    }
    haystack.windows(needle.len()).any(|w| w == needle)
}

fn env_nonempty(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|s| !s.is_empty())
}

fn locate_on_path(name: &str) -> Option<PathBuf> {
    std::env::var("PATH").ok()?.split(':').find_map(|dir| {
        if dir.is_empty() {
            return None;
        }
        let candidate = PathBuf::from(dir).join(name);
        candidate
            .exists()
            .then(|| std::fs::canonicalize(&candidate).unwrap_or(candidate))
    })
}

fn julia_missing() -> Option<HealthIssue> {
    which::which("julia").err().map(|_| {
        HealthIssue::new(
            "julia-missing",
            "julia not found on PATH — cells cannot run",
            "install Julia (https://julialang.org/install), then restart Helix",
        )
    })
}

fn terminal_multiplexer() -> Option<HealthIssue> {
    let multiplexer = if std::env::var_os("TMUX").is_some() {
        "tmux"
    } else if std::env::var_os("ZELLIJ").is_some() {
        "Zellij"
    } else {
        return None;
    };
    Some(HealthIssue::new(
        "terminal-multiplexer",
        format!("running inside {multiplexer} — inline plots, math, and tables may not render"),
        format!("run Helix directly in a Kitty-protocol terminal, not inside {multiplexer}"),
    ))
}

pub fn nothelix_health_check_tsv() -> String {
    Install::from_env()
        .issues()
        .into_iter()
        .chain(julia_missing())
        .chain(terminal_multiplexer())
        .map(|issue| issue.to_string())
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn healthy_layout(td: &TempDir) -> (PathBuf, PathBuf, PathBuf) {
        let steel_home = td.path().join("steel");
        let share = td.path().join("share");
        let bin_dir = td.path().join("bin");
        fs::create_dir_all(steel_home.join("native")).unwrap();
        fs::create_dir_all(steel_home.join("cogs/nothelix")).unwrap();
        fs::create_dir_all(&share).unwrap();
        fs::create_dir_all(&bin_dir).unwrap();
        fs::write(steel_home.join("native/libnothelix.dylib"), b"\x00").unwrap();
        fs::write(
            steel_home.join("native/libnothelix.meta"),
            "BUILD_ID=abc123\n",
        )
        .unwrap();
        fs::write(steel_home.join("cogs/nothelix.scm"), b";; entry\n").unwrap();
        fs::write(
            steel_home.join("cogs/nothelix/ffi-version.scm"),
            format!(
                "(define EXPECTED-FFI-VERSION {})\n",
                crate::NOTHELIX_FFI_VERSION
            ),
        )
        .unwrap();
        fs::write(share.join("VERSION"), "BUILD_ID=abc123\n").unwrap();
        let hx = bin_dir.join("hx-nothelix");
        fs::write(
            &hx,
            "add-or-replace-animating-raw-content \
             DocumentFocusGained \
             ViewportChanged \
             TerminalFocusGained",
        )
        .unwrap();
        (steel_home, share, hx)
    }

    #[test]
    fn healthy_install_reports_no_issues() {
        let td = TempDir::new().unwrap();
        let (steel, share, hx) = healthy_layout(&td);
        assert!(Install::at(&steel, &share, &hx).issues().is_empty());
    }

    #[test]
    fn missing_dylib_is_flagged() {
        let td = TempDir::new().unwrap();
        let (steel, share, hx) = healthy_layout(&td);
        fs::remove_file(steel.join("native/libnothelix.dylib")).unwrap();
        let issues = Install::at(&steel, &share, &hx).issues();
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].id, "dylib-missing");
        assert!(issues[0].fix_hint.contains("just install"));
    }

    #[test]
    fn missing_dylib_accepts_so_fallback() {
        let td = TempDir::new().unwrap();
        let (steel, share, hx) = healthy_layout(&td);
        fs::remove_file(steel.join("native/libnothelix.dylib")).unwrap();
        fs::write(steel.join("native/libnothelix.so"), b"\x00").unwrap();
        let issues = Install::at(&steel, &share, &hx).issues();
        assert!(issues.iter().all(|i| i.id != "dylib-missing"));
    }

    #[test]
    fn build_id_mismatch_is_flagged() {
        let td = TempDir::new().unwrap();
        let (steel, share, hx) = healthy_layout(&td);
        fs::write(share.join("VERSION"), "BUILD_ID=zzz999\n").unwrap();
        let issues = Install::at(&steel, &share, &hx).issues();
        assert!(issues.iter().any(|i| i.id == "build-id-mismatch"));
        let issue = issues.iter().find(|i| i.id == "build-id-mismatch").unwrap();
        assert!(issue.message.contains("abc123"));
        assert!(issue.message.contains("zzz999"));
    }

    #[test]
    fn build_id_mismatch_silent_when_files_missing() {
        let td = TempDir::new().unwrap();
        let (steel, share, hx) = healthy_layout(&td);
        fs::remove_file(steel.join("native/libnothelix.meta")).unwrap();
        let issues = Install::at(&steel, &share, &hx).issues();
        assert!(issues.iter().all(|i| i.id != "build-id-mismatch"));
    }

    #[test]
    fn missing_cogs_is_flagged() {
        let td = TempDir::new().unwrap();
        let (steel, share, hx) = healthy_layout(&td);
        fs::remove_file(steel.join("cogs/nothelix.scm")).unwrap();
        let issues = Install::at(&steel, &share, &hx).issues();
        assert!(issues.iter().any(|i| i.id == "cogs-missing"));
    }

    #[test]
    fn missing_cogs_dir_is_flagged() {
        let td = TempDir::new().unwrap();
        let (steel, share, hx) = healthy_layout(&td);
        fs::remove_dir_all(steel.join("cogs/nothelix")).unwrap();
        let issues = Install::at(&steel, &share, &hx).issues();
        assert!(issues.iter().any(|i| i.id == "cogs-missing"));
    }

    #[test]
    fn ffi_version_mismatch_is_flagged() {
        let td = TempDir::new().unwrap();
        let (steel, share, hx) = healthy_layout(&td);
        fs::write(
            steel.join("cogs/nothelix/ffi-version.scm"),
            "(define EXPECTED-FFI-VERSION 999)\n",
        )
        .unwrap();
        let issues = Install::at(&steel, &share, &hx).issues();
        let issue = issues
            .iter()
            .find(|i| i.id == "ffi-version-mismatch")
            .expect("expected ffi-version-mismatch");
        assert_eq!(
            issue.message,
            format!(
                "libnothelix FFI v{}, plugin expects v999",
                crate::NOTHELIX_FFI_VERSION
            )
        );
        assert_eq!(
            issue.fix_hint,
            "run 'just install' to rebuild the dylib against the live-linked plugin"
        );
    }

    #[test]
    fn plugin_without_version_declaration_is_flagged_as_v0() {
        let td = TempDir::new().unwrap();
        let (steel, share, hx) = healthy_layout(&td);
        fs::remove_file(steel.join("cogs/nothelix/ffi-version.scm")).unwrap();
        let issues = Install::at(&steel, &share, &hx).issues();
        let issue = issues
            .iter()
            .find(|i| i.id == "ffi-version-mismatch")
            .expect("expected ffi-version-mismatch");
        assert!(issue.message.contains("plugin expects v0"));
    }

    #[test]
    fn ffi_version_in_comment_does_not_mask_declaration() {
        let td = TempDir::new().unwrap();
        let (steel, share, hx) = healthy_layout(&td);
        fs::write(
            steel.join("cogs/nothelix/ffi-version.scm"),
            format!(
                ";; EXPECTED-FFI-VERSION 42 must match lib.rs\n\
                 (define EXPECTED-FFI-VERSION {})\n\
                 (when (not (equal? got EXPECTED-FFI-VERSION)) (error \"boom\"))\n",
                crate::NOTHELIX_FFI_VERSION
            ),
        )
        .unwrap();
        let issues = Install::at(&steel, &share, &hx).issues();
        assert!(issues.iter().all(|i| i.id != "ffi-version-mismatch"));
    }

    #[test]
    fn ffi_version_silent_when_plugin_missing() {
        let td = TempDir::new().unwrap();
        let (steel, share, hx) = healthy_layout(&td);
        fs::remove_file(steel.join("cogs/nothelix.scm")).unwrap();
        let issues = Install::at(&steel, &share, &hx).issues();
        assert!(issues.iter().all(|i| i.id != "ffi-version-mismatch"));
        assert!(issues.iter().any(|i| i.id == "cogs-missing"));
    }

    #[test]
    fn missing_fork_symbols_is_flagged() {
        let td = TempDir::new().unwrap();
        let (steel, share, hx) = healthy_layout(&td);
        fs::write(&hx, "stub binary contents only — none of the fork symbols").unwrap();
        let issues = Install::at(&steel, &share, &hx).issues();
        let issue = issues
            .iter()
            .find(|i| i.id == "fork-symbols-missing")
            .expect("expected fork-symbols-missing");
        assert!(
            issue
                .message
                .contains("add-or-replace-animating-raw-content"),
            "expected specific symbol in message: {}",
            issue.message
        );
        assert!(issue.fix_hint.contains("darwin-rebuild"));
    }

    #[test]
    fn partial_fork_symbols_lists_only_missing_ones() {
        let td = TempDir::new().unwrap();
        let (steel, share, hx) = healthy_layout(&td);
        fs::write(&hx, "add-or-replace-animating-raw-content only — no events").unwrap();
        let issues = Install::at(&steel, &share, &hx).issues();
        let issue = issues
            .iter()
            .find(|i| i.id == "fork-symbols-missing")
            .expect("expected fork-symbols-missing");
        assert!(issue.message.contains("DocumentFocusGained"));
        assert!(issue.message.contains("ViewportChanged"));
        assert!(issue.message.contains("TerminalFocusGained"));
        assert!(
            !issue
                .message
                .contains("add-or-replace-animating-raw-content")
        );
    }

    #[test]
    fn lto_friendly_binary_with_only_indicator_strings_passes() {
        let td = TempDir::new().unwrap();
        let (steel, share, hx) = healthy_layout(&td);
        fs::write(
            &hx,
            "this binary has: add-or-replace-animating-raw-content \
             plus event types DocumentFocusGained ViewportChanged \
             TerminalFocusGained \
             — the kebab-case strings are inlined and absent",
        )
        .unwrap();
        let issues = Install::at(&steel, &share, &hx).issues();
        assert!(
            issues.iter().all(|i| i.id != "fork-symbols-missing"),
            "LTO-shaped fresh binary must not be flagged: {issues:#?}"
        );
    }

    #[test]
    fn missing_hx_binary_skips_symbol_check() {
        let td = TempDir::new().unwrap();
        let (steel, share, hx) = healthy_layout(&td);
        fs::remove_file(&hx).unwrap();
        let issues = Install::at(&steel, &share, &hx).issues();
        assert!(issues.iter().all(|i| i.id != "fork-symbols-missing"));
    }

    #[test]
    fn display_emits_tab_separated_row() {
        let issue = HealthIssue::new("the-id", "the message", "the fix");
        assert_eq!(issue.to_string(), "the-id\tthe message\tthe fix");
    }

    #[test]
    fn tsv_format_round_trips_one_issue() {
        let td = TempDir::new().unwrap();
        let (steel, share, hx) = healthy_layout(&td);
        fs::remove_file(steel.join("native/libnothelix.dylib")).unwrap();
        let issues = Install::at(&steel, &share, &hx).issues();
        let tsv = issues
            .iter()
            .map(ToString::to_string)
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

        let prev_path = std::env::var("PATH").ok();
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
        unsafe {
            std::env::set_var("PATH", "/var/empty");
        }
        let found = locate_on_path("definitely-not-a-real-binary-name-xyz");
        match prev_path {
            Some(p) => unsafe { std::env::set_var("PATH", p) },
            None => unsafe { std::env::remove_var("PATH") },
        }
        assert!(found.is_none());
    }

    #[test]
    fn ffi_wrapper_returns_string() {
        let out = nothelix_health_check_tsv();
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
