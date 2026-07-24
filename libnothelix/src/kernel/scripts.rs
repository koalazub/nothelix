use crate::error::{Error, Result};
use std::fs;
use std::path::{Path, PathBuf};

include!(concat!(env!("OUT_DIR"), "/kernel_sources.rs"));

pub(super) fn install_runner() -> Result<PathBuf> {
    let home = std::env::var("HOME").map_or_else(|_| PathBuf::from("/tmp"), PathBuf::from);
    install_runner_into(&home)
}

fn install_runner_into(home: &Path) -> Result<PathBuf> {
    let dir = home.join(".local/share/nothelix/kernel");
    fs::create_dir_all(&dir).map_err(|e| Error::creating(&dir, e))?;
    for &(name, source) in SOURCES {
        let path = dir.join(name);
        fs::write(&path, source).map_err(|e| Error::writing(path, e))?;
    }
    Ok(dir.join("runner.jl"))
}

#[cfg(test)]
mod tests {
    use super::{SOURCES, install_runner_into};
    use std::process::Stdio;
    use std::time::{Duration, Instant};

    #[test]
    fn embeds_every_non_test_kernel_source_on_disk() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../kernel");
        let mut on_disk: Vec<String> = std::fs::read_dir(dir)
            .expect("kernel/ directory")
            .filter_map(|entry| entry.ok())
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .filter(|name| name.ends_with(".jl") && !name.ends_with("_test.jl"))
            .collect();
        on_disk.sort();
        let mut embedded: Vec<String> = SOURCES.iter().map(|&(name, _)| name.to_string()).collect();
        embedded.sort();
        assert_eq!(on_disk, embedded);
    }

    #[test]
    fn install_writes_every_embedded_source() {
        let scratch =
            std::env::temp_dir().join(format!("nothelix-install-gate-{}", std::process::id()));
        std::fs::remove_dir_all(&scratch).ok();
        let runner = install_runner_into(&scratch).expect("install_runner_into");
        assert!(runner.ends_with(".local/share/nothelix/kernel/runner.jl"));
        for &(name, source) in SOURCES {
            let written = scratch.join(".local/share/nothelix/kernel").join(name);
            let on_disk = std::fs::read_to_string(&written)
                .unwrap_or_else(|e| panic!("{name} was not written by install_runner: {e}"));
            assert_eq!(on_disk, source, "{name} content drifted from the embed");
        }
        std::fs::remove_dir_all(&scratch).ok();
    }

    #[test]
    fn installed_kernel_boots_to_ready() {
        let scratch =
            std::env::temp_dir().join(format!("nothelix-boot-gate-{}", std::process::id()));
        std::fs::remove_dir_all(&scratch).ok();
        let runner = install_runner_into(&scratch).expect("install_runner_into");
        let kernel_dir = scratch.join("kernel-dir");
        std::fs::create_dir_all(&kernel_dir).expect("kernel dir");

        let mut child = std::process::Command::new("julia")
            .arg("--startup-file=no")
            .arg(&runner)
            .arg(&kernel_dir)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("julia is required for the kernel boot gate");

        let ready = kernel_dir.join("ready");
        let deadline = Instant::now() + Duration::from_secs(120);
        while !ready.exists() && Instant::now() < deadline {
            if let Some(status) = child.try_wait().expect("try_wait") {
                let log =
                    std::fs::read_to_string(kernel_dir.join("kernel.log")).unwrap_or_default();
                let tail: String = log
                    .lines()
                    .rev()
                    .take(8)
                    .collect::<Vec<_>>()
                    .into_iter()
                    .rev()
                    .collect::<Vec<_>>()
                    .join("\n");
                panic!("runner exited with {status} before ready; log tail:\n{tail}");
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        let booted = ready.exists();
        let phase_cleaned = !kernel_dir.join("phase").exists();
        child.kill().ok();
        child.wait().ok();
        std::fs::remove_dir_all(&scratch).ok();
        assert!(booted, "installed kernel did not reach ready within 120s");
        assert!(phase_cleaned, "phase file must be removed before ready");
    }
}
