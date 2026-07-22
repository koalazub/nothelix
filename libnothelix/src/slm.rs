#![allow(clippy::needless_pass_by_value)]

use crate::error::{Error, Result};
use std::fs;
use std::io::{ErrorKind, Read as _, Write as _};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

const CELL_SEP: char = '\u{1e}';
const COMMAND_LINE_TOOLS: &str = "/Library/Developer/CommandLineTools";
const HELPER_TIMEOUT: Duration = Duration::from_secs(120);
const HELPER_POLL_INTERVAL: Duration = Duration::from_millis(50);
const HELPER_SOURCE: &str = include_str!("../../tools/nothelix-slm/main.swift");

fn nothelix_data_dir() -> PathBuf {
    std::env::var("HOME")
        .map_or_else(|_| PathBuf::from("/tmp"), PathBuf::from)
        .join(".local/share/nothelix")
}

fn path_safe(key: &str) -> String {
    key.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

fn djb2_hash(s: &str) -> i64 {
    let mut h: i64 = 5381;
    for c in s.chars() {
        h = (h * 33 + i64::from(c as u32)) % 2_147_483_647;
    }
    h
}

fn split_cells(blob: &str) -> Vec<&str> {
    blob.split(CELL_SEP).collect()
}

fn is_blank_cell(cell: &str) -> bool {
    cell.trim().is_empty()
}

struct SummaryCache {
    root: PathBuf,
}

impl SummaryCache {
    fn under(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    fn entry(&self, workspace: &str, hash: &str) -> PathBuf {
        self.root.join(path_safe(workspace)).join(path_safe(hash))
    }

    fn label(&self, workspace: &str, hash: &str) -> Result<Option<String>> {
        let path = self.entry(workspace, hash);
        match fs::read_to_string(&path) {
            Ok(label) => Ok(Some(label)),
            Err(e) if e.kind() == ErrorKind::NotFound => Ok(None),
            Err(e) => Err(Error::reading(path, e)),
        }
    }

    fn store(&self, workspace: &str, hash: &str, label: &str) -> Result<()> {
        let path = self.entry(workspace, hash);
        let parent = path.parent().ok_or_else(|| Error::orphan(&path))?;
        fs::create_dir_all(parent).map_err(|e| Error::creating(parent, e))?;
        fs::write(&path, label).map_err(|e| Error::writing(path, e))
    }

    fn uncached<'a>(&self, workspace: &str, cells: &[&'a str]) -> Vec<&'a str> {
        cells
            .iter()
            .copied()
            .filter(|cell| !self.entry(workspace, &djb2_hash(cell).to_string()).exists())
            .collect()
    }

    fn seed_blanks(&self, workspace: &str, blanks: &[&str]) -> Result<()> {
        for cell in blanks {
            self.store(workspace, &djb2_hash(cell).to_string(), "")?;
        }
        Ok(())
    }

    fn assign(&self, workspace: &str, sent: &[&str], stdout: &str) -> Result<()> {
        for (cell, label) in sent.iter().zip(stdout.lines()) {
            self.store(workspace, &djb2_hash(cell).to_string(), label)?;
        }
        Ok(())
    }
}

fn sdk_name(path: &Path) -> Option<&str> {
    let name = path.file_name()?.to_str()?;
    if name.starts_with("MacOSX") && name.ends_with(".sdk") {
        Some(name)
    } else {
        None
    }
}

fn newest_sdk_in(dir: &Path) -> Option<PathBuf> {
    fs::read_dir(dir)
        .ok()?
        .filter_map(std::result::Result::ok)
        .map(|entry| entry.path())
        .filter(|path| sdk_name(path).is_some())
        .max_by(|a, b| sdk_name(a).cmp(&sdk_name(b)))
}

fn swiftc(source: &Path, out: &Path, sdk: Option<&Path>) -> Result<()> {
    let mut command = Command::new("swiftc");
    if let Some(sdk) = sdk {
        command
            .env("DEVELOPER_DIR", COMMAND_LINE_TOOLS)
            .arg("-sdk")
            .arg(sdk);
    }
    let status = command
        .arg(source)
        .arg("-o")
        .arg(out)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map_err(|e| Error::Subprocess {
            command: "swiftc".to_string(),
            detail: e.to_string(),
        })?;
    if !status.success() {
        return Err(Error::Subprocess {
            command: "swiftc".to_string(),
            detail: format!("exited with {status}"),
        });
    }
    if out.exists() {
        Ok(())
    } else {
        Err(Error::Subprocess {
            command: "swiftc".to_string(),
            detail: format!("produced no binary at {}", out.display()),
        })
    }
}

fn compile_helper(out: &Path) -> Result<()> {
    let dir = out.parent().ok_or_else(|| Error::orphan(out))?;
    fs::create_dir_all(dir).map_err(|e| Error::creating(dir, e))?;
    let staged = tempfile::Builder::new()
        .suffix(".swift")
        .tempfile()
        .map_err(|e| Error::creating("staged nothelix-slm source", e))?;
    fs::write(staged.path(), HELPER_SOURCE).map_err(|e| Error::writing(staged.path(), e))?;

    let Err(without_sdk) = swiftc(staged.path(), out, None) else {
        return Ok(());
    };
    let Some(sdk) = newest_sdk_in(&Path::new(COMMAND_LINE_TOOLS).join("SDKs")) else {
        return Err(without_sdk);
    };
    swiftc(staged.path(), out, Some(&sdk)).map_err(|with_sdk| Error::Subprocess {
        command: "swiftc".to_string(),
        detail: format!("{without_sdk}; with -sdk {}: {with_sdk}", sdk.display()),
    })
}

struct Helper {
    path: PathBuf,
}

impl Helper {
    fn installed() -> Result<Self> {
        let path = nothelix_data_dir().join("bin/nothelix-slm");
        if !path.exists() {
            compile_helper(&path)?;
        }
        Ok(Self { path })
    }

    fn answers_probe(&self) -> Result<()> {
        let status = Command::new(&self.path)
            .arg("--probe")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map_err(|e| self.fault(e.to_string()))?;
        if status.success() {
            Ok(())
        } else {
            Err(self.fault(format!("--probe exited with {status}")))
        }
    }

    fn fault(&self, detail: String) -> Error {
        Error::Subprocess {
            command: self.path.display().to_string(),
            detail,
        }
    }
}

fn summarize(command: &Path, stdin_blob: &str, timeout: Duration) -> Result<String> {
    let fault = |detail: String| Error::Subprocess {
        command: command.display().to_string(),
        detail,
    };
    let mut child = Command::new(command)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| fault(e.to_string()))?;
    let mut stdin = child
        .stdin
        .take()
        .ok_or_else(|| fault("no stdin pipe".to_string()))?;
    stdin
        .write_all(stdin_blob.as_bytes())
        .map_err(|e| fault(format!("cannot feed stdin: {e}")))?;
    drop(stdin);
    let (status, stdout) = wait_with_timeout(child, timeout, &fault)?;
    if status.success() {
        Ok(stdout)
    } else {
        Err(fault(format!("exited with {status}")))
    }
}

fn wait_with_timeout(
    mut child: Child,
    timeout: Duration,
    fault: &impl Fn(String) -> Error,
) -> Result<(ExitStatus, String)> {
    let mut stdout = child
        .stdout
        .take()
        .ok_or_else(|| fault("no stdout pipe".to_string()))?;
    let reader = std::thread::spawn(move || {
        let mut buf = String::new();
        stdout.read_to_string(&mut buf).map(|_| buf)
    });
    let deadline = Instant::now() + timeout;
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                let out = reader
                    .join()
                    .map_err(|_| fault("stdout reader panicked".to_string()))?
                    .map_err(|e| fault(format!("cannot read stdout: {e}")))?;
                return Ok((status, out));
            }
            Ok(None) if Instant::now() >= deadline => {
                let _ = child.kill();
                let _ = child.wait();
                let _ = reader.join();
                return Err(fault(format!("no reply within {}s", timeout.as_secs_f32())));
            }
            Ok(None) => std::thread::sleep(HELPER_POLL_INTERVAL),
            Err(e) => return Err(fault(format!("cannot await exit: {e}"))),
        }
    }
}

fn refresh(cache: &SummaryCache, workspace: &str, cells_blob: &str) -> Result<()> {
    let helper = Helper::installed()?;
    let cells = split_cells(cells_blob);
    let misses = cache.uncached(workspace, &cells);
    if misses.is_empty() {
        return Ok(());
    }
    let (blanks, sent): (Vec<&str>, Vec<&str>) =
        misses.iter().copied().partition(|c| is_blank_cell(c));
    cache.seed_blanks(workspace, &blanks)?;
    if sent.is_empty() {
        return Ok(());
    }
    let stdout = summarize(
        &helper.path,
        &sent.join(&CELL_SEP.to_string()),
        HELPER_TIMEOUT,
    )?;
    cache.assign(workspace, &sent, &stdout)
}

static SLM_AVAILABLE: OnceLock<Mutex<Option<bool>>> = OnceLock::new();
static REFRESH_IN_FLIGHT: AtomicBool = AtomicBool::new(false);

fn try_acquire_in_flight() -> bool {
    REFRESH_IN_FLIGHT
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .is_ok()
}

fn release_in_flight() {
    REFRESH_IN_FLIGHT.store(false, Ordering::SeqCst);
}

struct InFlightGuard;

impl Drop for InFlightGuard {
    fn drop(&mut self) {
        release_in_flight();
    }
}

pub fn slm_available(_workspace: String) -> String {
    let lock = SLM_AVAILABLE.get_or_init(|| Mutex::new(None));
    let mut guard = lock.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    let available =
        *guard.get_or_insert_with(|| Helper::installed().and_then(|h| h.answers_probe()).is_ok());
    if available { "yes".into() } else { "no".into() }
}

pub fn slm_refresh_summaries(workspace: String, cells_blob: String) -> String {
    if !try_acquire_in_flight() {
        return String::new();
    }
    std::thread::spawn(move || {
        let _guard = InFlightGuard;
        let _unreportable = refresh(
            &SummaryCache::under(nothelix_data_dir().join("summaries")),
            &workspace,
            &cells_blob,
        );
    });
    String::new()
}

pub fn slm_summary_for(workspace: String, hash: String) -> String {
    crate::error::ffi(
        SummaryCache::under(nothelix_data_dir().join("summaries"))
            .label(&workspace, &hash)
            .map(Option::unwrap_or_default),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn cache_get(root: &Path, workspace: &str, hash: &str) -> String {
        SummaryCache::under(root)
            .label(workspace, hash)
            .unwrap()
            .unwrap_or_default()
    }

    fn cache_put(root: &Path, workspace: &str, hash: &str, label: &str) {
        SummaryCache::under(root)
            .store(workspace, hash, label)
            .unwrap();
    }

    #[test]
    fn djb2_matches_scheme_reference_value() {
        assert_eq!(djb2_hash("abc"), 193_485_963);
    }

    #[test]
    fn djb2_of_empty_string_is_seed() {
        assert_eq!(djb2_hash(""), 5381);
    }

    #[test]
    fn djb2_hashes_unicode_by_scalar_value_not_byte() {
        let ascii = djb2_hash("a");
        let multibyte = djb2_hash("\u{2200}");
        assert_ne!(ascii, multibyte);
        assert_eq!(multibyte, 5381_i64 * 33 + 0x2200);
    }

    #[test]
    fn cache_roundtrip() {
        let root = tempdir().unwrap();
        assert_eq!(cache_get(root.path(), "ws", "42"), "");
        cache_put(root.path(), "ws", "42", "null space basis");
        assert_eq!(cache_get(root.path(), "ws", "42"), "null space basis");
    }

    #[test]
    fn cache_write_empty_line_still_lands_a_file() {
        let root = tempdir().unwrap();
        cache_put(root.path(), "ws", "1", "");
        assert!(SummaryCache::under(root.path()).entry("ws", "1").exists());
        assert_eq!(cache_get(root.path(), "ws", "1"), "");
    }

    #[test]
    fn distinct_workspaces_isolate() {
        let root = tempdir().unwrap();
        cache_put(root.path(), "wsA", "1", "a-label");
        cache_put(root.path(), "wsB", "1", "b-label");
        assert_eq!(cache_get(root.path(), "wsA", "1"), "a-label");
        assert_eq!(cache_get(root.path(), "wsB", "1"), "b-label");
    }

    #[test]
    fn sanitizes_path_separators_in_workspace() {
        let root = tempdir().unwrap();
        cache_put(root.path(), "/abs/ws/../x", "1", "v");
        assert_eq!(cache_get(root.path(), "/abs/ws/../x", "1"), "v");
    }

    #[test]
    fn split_cells_on_record_separator() {
        let blob = format!("first{CELL_SEP}second{CELL_SEP}third");
        assert_eq!(split_cells(&blob), vec!["first", "second", "third"]);
    }

    #[test]
    fn split_cells_single_entry_has_no_separator() {
        assert_eq!(split_cells("only"), vec!["only"]);
    }

    #[test]
    fn missing_hashes_skips_cells_with_a_cache_file() {
        let root = tempdir().unwrap();
        cache_put(
            root.path(),
            "ws",
            &djb2_hash("cell a").to_string(),
            "cached",
        );
        let cells = ["cell a", "cell b"];
        assert_eq!(
            SummaryCache::under(root.path()).uncached("ws", &cells),
            vec!["cell b"]
        );
    }

    #[test]
    fn missing_hashes_empty_when_all_cached() {
        let root = tempdir().unwrap();
        let cells = ["x", "y"];
        for cell in cells {
            cache_put(root.path(), "ws", &djb2_hash(cell).to_string(), "l");
        }
        assert!(
            SummaryCache::under(root.path())
                .uncached("ws", &cells)
                .is_empty()
        );
    }

    #[test]
    fn newest_sdk_picks_the_unversioned_major_symlink() {
        let dir = tempdir().unwrap();
        for name in [
            "MacOSX15.sdk",
            "MacOSX26.sdk",
            "MacOSX26.5.sdk",
            "MacOSX27.0.sdk",
            "MacOSX27.sdk",
            "MacOSX.sdk",
        ] {
            fs::write(dir.path().join(name), b"").unwrap();
        }
        let picked = newest_sdk_in(dir.path()).unwrap();
        assert_eq!(
            picked.file_name().unwrap().to_str().unwrap(),
            "MacOSX27.sdk"
        );
    }

    #[test]
    fn newest_sdk_ignores_non_sdk_entries() {
        let dir = tempdir().unwrap();
        fs::write(dir.path().join("readme.txt"), b"").unwrap();
        assert!(newest_sdk_in(dir.path()).is_none());
    }

    #[test]
    fn newest_sdk_missing_dir_is_none() {
        let dir = tempdir().unwrap();
        assert!(newest_sdk_in(&dir.path().join("does-not-exist")).is_none());
    }

    #[test]
    fn partition_misses_separates_blank_from_non_blank() {
        let misses = ["print(1)", "", "print(2)", "   "];
        let (blanks, sent): (Vec<&str>, Vec<&str>) =
            misses.iter().copied().partition(|c| is_blank_cell(c));
        assert_eq!(blanks, vec!["", "   "]);
        assert_eq!(sent, vec!["print(1)", "print(2)"]);
    }

    #[test]
    fn empty_cell_in_batch_does_not_shift_neighboring_hashes() {
        let root = tempdir().unwrap();
        let cache = SummaryCache::under(root.path());
        let cells = ["print(1)", "", "print(2)"];
        let misses = cache.uncached("ws", &cells);
        assert_eq!(misses, vec!["print(1)", "", "print(2)"]);

        let (blanks, sent): (Vec<&str>, Vec<&str>) =
            misses.iter().copied().partition(|c| is_blank_cell(c));
        cache.seed_blanks("ws", &blanks).unwrap();
        cache
            .assign("ws", &sent, "prints one\nprints two\n")
            .unwrap();

        assert_eq!(
            cache_get(root.path(), "ws", &djb2_hash("print(1)").to_string()),
            "prints one"
        );
        assert_eq!(
            cache_get(root.path(), "ws", &djb2_hash("print(2)").to_string()),
            "prints two"
        );
        assert_eq!(cache_get(root.path(), "ws", &djb2_hash("").to_string()), "");
    }

    #[test]
    fn seed_blank_cells_writes_empty_label_without_calling_helper() {
        let root = tempdir().unwrap();
        SummaryCache::under(root.path())
            .seed_blanks("ws", &["", "   "])
            .unwrap();
        assert_eq!(cache_get(root.path(), "ws", &djb2_hash("").to_string()), "");
        assert_eq!(
            cache_get(root.path(), "ws", &djb2_hash("   ").to_string()),
            ""
        );
    }

    #[test]
    fn in_flight_guard_blocks_second_acquire_until_released() {
        release_in_flight();
        assert!(try_acquire_in_flight());
        assert!(!try_acquire_in_flight());
        release_in_flight();
        assert!(try_acquire_in_flight());
        release_in_flight();
    }

    #[test]
    fn in_flight_guard_releases_on_drop_even_after_panic() {
        release_in_flight();
        assert!(try_acquire_in_flight());
        let result = std::panic::catch_unwind(|| {
            let _guard = InFlightGuard;
            panic!("simulated worker panic");
        });
        assert!(result.is_err());
        assert!(try_acquire_in_flight());
        release_in_flight();
    }

    #[test]
    fn run_helper_kills_and_reports_the_timeout() {
        let dir = tempdir().unwrap();
        let script = dir.path().join("hang.sh");
        fs::write(&script, "#!/bin/sh\nexec sleep 5\n").unwrap();
        {
            use std::os::unix::fs::PermissionsExt as _;
            let mut perms = fs::metadata(&script).unwrap().permissions();
            perms.set_mode(0o755);
            fs::set_permissions(&script, perms).unwrap();
        }

        let started = Instant::now();
        let failure = summarize(&script, "input", Duration::from_millis(150)).unwrap_err();
        assert!(failure.to_string().contains("no reply within"), "{failure}");
        assert!(
            started.elapsed() < Duration::from_secs(2),
            "timeout should kill the wedged helper well before its 5s sleep completes"
        );
    }
}
