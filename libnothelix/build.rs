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
