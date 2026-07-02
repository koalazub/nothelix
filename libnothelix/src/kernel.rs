// Steel's `register_fn` marshals values from the Steel VM and requires
// the registered fn's signature to take owned types (`String`, `RVec<u8>`),
// not borrows. The clippy lint is technically correct that some args
// aren't consumed internally, but the owned type is load-bearing for the
// FFI dispatcher.
#![allow(clippy::needless_pass_by_value)]

//! Kernel lifecycle management.
//!
//! Manages Julia kernel processes via file-based IPC:
//!   - `input.json`       — command written by Rust
//!   - `output.msgpack`   — result written by Julia (preferred, when MsgPack.jl is available)
//!   - `output.done`      — sentinel file signalling msgpack completion
//!   - `output.json`      — result written by Julia (fallback)
//!   - `output.json.done` — sentinel file signalling JSON completion
//!   - `pid`              — PID of the running Julia process
//!
//! Signals are sent via `nix` for proper POSIX semantics (SIGTERM / SIGINT).

use std::{fs, path::Path, process::Command};

use nix::{
    sys::signal::{self, Signal},
    unistd::Pid,
};
use serde_json::{Value, json};
use which::which;

// ─── Embedded kernel scripts ──────────────────────────────────────────────────
// Extracted to ~/.local/share/nothelix/kernel/ on first kernel start.

static RUNNER_JL: &str = include_str!("../../kernel/runner.jl");
static CELL_REGISTRY_JL: &str = include_str!("../../kernel/cell_registry.jl");
static AST_ANALYSIS_JL: &str = include_str!("../../kernel/ast_analysis.jl");
static OUTPUT_CAPTURE_JL: &str = include_str!("../../kernel/output_capture.jl");
static CELL_MACROS_JL: &str = include_str!("../../kernel/cell_macros.jl");

// ─── Helpers ──────────────────────────────────────────────────────────────────

fn home_dir() -> std::path::PathBuf {
    std::env::var("HOME")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| std::path::PathBuf::from("/tmp"))
}

fn kernel_scripts_dir() -> std::path::PathBuf {
    home_dir().join(".local/share/nothelix/kernel")
}

/// Extract embedded kernel scripts to disk (idempotent — always overwrites
/// so that upgrades propagate automatically).
fn ensure_kernel_scripts() -> Result<std::path::PathBuf, String> {
    let dir = kernel_scripts_dir();
    fs::create_dir_all(&dir).map_err(|e| format!("Cannot create kernel scripts dir: {e}"))?;

    let files = &[
        ("runner.jl", RUNNER_JL),
        ("cell_registry.jl", CELL_REGISTRY_JL),
        ("ast_analysis.jl", AST_ANALYSIS_JL),
        ("output_capture.jl", OUTPUT_CAPTURE_JL),
        ("cell_macros.jl", CELL_MACROS_JL),
    ];

    for (name, content) in files {
        let path = dir.join(name);
        fs::write(&path, content).map_err(|e| format!("Cannot write {name}: {e}"))?;
    }

    Ok(dir)
}

/// Read the PID stored in `<kernel_dir>/pid`, if present.
fn read_pid(kernel_dir: &Path) -> Option<u32> {
    fs::read_to_string(kernel_dir.join("pid"))
        .ok()
        .and_then(|s| s.trim().parse::<u32>().ok())
}

/// Send SIGTERM to a process via `nix`, falling back to `kill(1)` on error.
fn sigterm_pid(pid: u32) {
    let nix_pid = Pid::from_raw(pid as i32);
    if signal::kill(nix_pid, Signal::SIGTERM).is_err() {
        // Graceful degradation: try the system kill command.
        let _ = Command::new("kill")
            .args(["-SIGTERM", &pid.to_string()])
            .output();
    }
}

/// Send SIGINT to a process via `nix`.
fn sigint_pid(pid: u32) {
    let nix_pid = Pid::from_raw(pid as i32);
    if signal::kill(nix_pid, Signal::SIGINT).is_err() {
        let _ = Command::new("kill")
            .args(["-SIGINT", &pid.to_string()])
            .output();
    }
}

/// #true when the process is alive and its command line names our runner.jl
/// with this kernel dir — guards against PID recycling handing us a stranger.
fn is_live_runner(pid: u32, kernel_dir: &str) -> bool {
    if signal::kill(Pid::from_raw(pid as i32), None).is_err() {
        return false;
    }
    let output = match Command::new("ps")
        .args(["-p", &pid.to_string(), "-o", "command="])
        .output()
    {
        Ok(o) if o.status.success() => o,
        _ => return false,
    };
    let command = String::from_utf8_lossy(&output.stdout);
    command.contains("runner.jl") && command.contains(kernel_dir)
}

/// Adopt a kernel left running by a previous editor session. Succeeds when
/// `<kernel_dir>/pid` names a live runner.jl for this dir and the ready
/// marker exists; stale in-flight IPC files are cleared so the next command
/// starts clean. Returns `{"status":"ok"}` or `{"status":"none"}`.
pub fn kernel_adopt(kernel_dir: String) -> String {
    let kdir = Path::new(&kernel_dir);
    let Some(pid) = read_pid(kdir) else {
        return json!({"status": "none", "reason": "no pid file"}).to_string();
    };
    if !is_live_runner(pid, &kernel_dir) {
        return json!({"status": "none", "reason": "process not alive"}).to_string();
    }
    if !kdir.join("ready").exists() {
        return json!({"status": "none", "reason": "kernel not ready"}).to_string();
    }
    for f in &[
        "input.json",
        "output.json",
        "output.json.done",
        "output.msgpack",
        "output.done",
    ] {
        let _ = fs::remove_file(kdir.join(f));
    }
    json!({"status": "ok", "pid": pid}).to_string()
}

// ─── IPC helpers ──────────────────────────────────────────────────────────────

pub fn write_kernel_command(kernel_dir: &str, cmd: &Value) -> Result<(), String> {
    let input_file = Path::new(kernel_dir).join("input.json");
    // Remove any stale done markers before writing a new command.
    let _ = fs::remove_file(Path::new(kernel_dir).join("output.json.done"));
    let _ = fs::remove_file(Path::new(kernel_dir).join("output.done"));
    fs::write(&input_file, cmd.to_string()).map_err(|e| format!("Cannot write input.json: {e}"))
}

// ─── FFI-facing functions ─────────────────────────────────────────────────────

/// Start a Julia kernel in `kernel_dir`. `julia_bin` overrides the PATH julia
/// when non-empty (a project may pin a specific interpreter); `julia_project`
/// is passed as `--project=<…>` when non-empty (a project may pin its env).
/// Both arrive ONLY after the project directory has been explicitly trusted
/// (see the trust allowlist) — opening an untrusted repo passes "" for both.
pub fn kernel_start_macro(
    kernel_dir: String,
    julia_bin: String,
    julia_project: String,
    notebook_path: String,
) -> String {
    let kdir = Path::new(&kernel_dir);

    // Kill any pre-existing process cleanly via SIGTERM.
    if let Some(pid) = read_pid(kdir) {
        sigterm_pid(pid);
    }

    // Remove all stale IPC files so the new process starts clean.
    // Without this, a leftover `ready` file tricks the Scheme side into
    // thinking the new kernel is up before it actually is.
    for f in &[
        "pid",
        "ready",
        "input.json",
        "output.json",
        "output.json.done",
        "output.msgpack",
        "output.done",
    ] {
        let _ = fs::remove_file(kdir.join(f));
    }

    if let Err(e) = fs::create_dir_all(kdir) {
        return json!({"status": "error", "error": format!("Cannot create kernel dir: {e}")})
            .to_string();
    }

    let scripts_dir = match ensure_kernel_scripts() {
        Ok(d) => d,
        Err(e) => return json!({"status": "error", "error": e}).to_string(),
    };

    let julia = if julia_bin.trim().is_empty() {
        match which("julia") {
            Ok(p) => p,
            Err(_) => {
                return json!({"status": "error", "error": "julia not found in PATH"}).to_string();
            }
        }
    } else {
        let p = std::path::PathBuf::from(julia_bin.trim());
        if !p.exists() {
            return json!({"status": "error",
                "error": format!("configured julia-bin does not exist: {}", p.display())})
            .to_string();
        }
        p
    };

    let runner = scripts_dir.join("runner.jl");

    // Redirect stdout/stderr to kernel.log so startup errors are diagnosable.
    let log_file = fs::File::create(kdir.join("kernel.log"))
        .map_err(|e| format!("Cannot create kernel.log: {e}"));
    let (stdout_cfg, stderr_cfg) = match log_file {
        Ok(f) => {
            let f2 = f.try_clone().expect("failed to clone log file handle");
            (std::process::Stdio::from(f), std::process::Stdio::from(f2))
        }
        Err(_) => (std::process::Stdio::null(), std::process::Stdio::null()),
    };

    // Don't set JULIA_LOAD_PATH — the kernel uses the user's default
    // env (where Pkg, LinearAlgebra, etc. live). NothelixMacros is only
    // needed by the LSP, and the kernel has its own CellMacros module
    // that defines @cell/@markdown for runtime execution.
    let mut cmd = Command::new(&julia);
    // `--project` must precede the script argument.
    let project = julia_project.trim();
    if !project.is_empty() {
        cmd.arg(format!("--project={project}"));
    }

    // Run the kernel in the notebook's own directory so relative paths in
    // cells (load_strain("data.hdf5"), include("helpers.jl"), readdir(), …)
    // resolve next to the notebook, exactly like Jupyter and Marimo. IPC is
    // unaffected: kernel_dir is an absolute scratch path. Scratch/unsaved
    // notebooks pass an empty path and keep the inherited working directory.
    let nb = notebook_path.trim();
    if !nb.is_empty()
        && let Some(dir) = Path::new(nb).parent()
    {
        let dir = fs::canonicalize(dir).unwrap_or_else(|_| dir.to_path_buf());
        if dir.is_dir() {
            cmd.current_dir(dir);
        }
    }

    match cmd
        .arg(&runner)
        .arg(&kernel_dir)
        // Headless graphics backends: prevent GR/Plots.jl from opening GUI windows
        .env("GKSwstype", "nul")
        .env("MPLBACKEND", "Agg")
        .stdin(std::process::Stdio::null())
        .stdout(stdout_cfg)
        .stderr(stderr_cfg)
        .spawn()
    {
        Ok(child) => {
            let _ = fs::write(kdir.join("pid"), child.id().to_string());
            json!({"status": "ok"}).to_string()
        }
        Err(e) => {
            json!({"status": "error", "error": format!("Failed to start Julia: {e}")}).to_string()
        }
    }
}

pub fn kernel_stop(kernel_dir: String) -> String {
    let kdir = Path::new(&kernel_dir);

    if let Some(pid) = read_pid(kdir) {
        sigterm_pid(pid);
        let _ = fs::remove_file(kdir.join("pid"));
    } else {
        return "ok".to_string();
    }

    // Clean up IPC files.
    for f in &[
        "input.json",
        "output.json",
        "output.json.done",
        "output.msgpack",
        "output.done",
        "ready",
    ] {
        let _ = fs::remove_file(kdir.join(f));
    }

    "ok".to_string()
}

pub fn kernel_stop_all_processes() -> String {
    // SIGTERM all Julia processes running runner.jl.
    let _ = Command::new("pkill")
        .args(["-SIGTERM", "-f", "runner.jl"])
        .output();
    // Remove stale kernel directories.
    let _ = Command::new("sh")
        .args(["-c", "rm -rf /tmp/helix-kernel-*"])
        .output();
    "All kernel processes stopped".to_string()
}

pub fn kernel_execute_cell_start(kernel_dir: String, cell_index: isize, code: String) -> String {
    let cmd = json!({"type": "execute_cell", "cell_index": cell_index, "code": code});
    match write_kernel_command(&kernel_dir, &cmd) {
        Ok(_) => json!({"status": "started"}).to_string(),
        Err(e) => json!({"status": "error", "error": e}).to_string(),
    }
}

pub fn kernel_poll_result(kernel_dir: String) -> String {
    let kdir = Path::new(&kernel_dir);
    let msgpack_done = kdir.join("output.done");
    let json_done = kdir.join("output.json.done");

    // Prefer msgpack, fall back to JSON.
    let (done_file, use_msgpack) = if msgpack_done.exists() {
        (msgpack_done, true)
    } else if json_done.exists() {
        (json_done, false)
    } else {
        return json!({"status": "pending"}).to_string();
    };

    let _ = fs::remove_file(&done_file);

    let parsed: Value = if use_msgpack {
        let output_file = kdir.join("output.msgpack");
        let bytes = match fs::read(&output_file) {
            Ok(b) => b,
            Err(e) => return json!({"status": "error", "error": e.to_string()}).to_string(),
        };
        match rmp_serde::from_slice(&bytes) {
            Ok(v) => v,
            Err(e) => return json!({"status": "error", "error": e.to_string()}).to_string(),
        }
    } else {
        let output_file = kdir.join("output.json");
        let content = match fs::read_to_string(&output_file) {
            Ok(c) => c,
            Err(e) => return json!({"status": "error", "error": e.to_string()}).to_string(),
        };
        match serde_json::from_str(&content) {
            Ok(v) => v,
            Err(e) => return json!({"status": "error", "error": e.to_string()}).to_string(),
        }
    };

    let status = parsed["status"].as_str().unwrap_or("error");
    if status == "error" {
        return json!({
            "status": "error",
            "error": parsed["error"].as_str().unwrap_or("Unknown error")
        })
        .to_string();
    }

    // Flatten runner.jl's `{"status": "ok", "cell": {...}}` into the flat
    // structure the Scheme layer expects.
    let cell = &parsed["cell"];
    let mut response = json!({
        "status":       status,
        "stdout":       cell["stdout"].as_str().unwrap_or(""),
        "stderr":       cell["stderr"].as_str().unwrap_or(""),
        "output_repr":  cell["output_repr"].as_str().unwrap_or(""),
        "has_error":    cell["has_error"].as_bool().unwrap_or(false),
        "error":        cell.get("error").and_then(|v| v.as_str()).unwrap_or(""),
    });

    if let Some(images) = cell.get("images") {
        response["images"] = images.clone();
    }

    if let Some(plot_data) = cell.get("plot_data") {
        response["plot_data"] = plot_data.clone();
    }

    if let Some(se) = cell.get("structured_error") {
        response["structured_error"] = se.clone();
    }

    response.to_string()
}

pub fn kernel_interrupt(kernel_dir: String) -> String {
    let kdir = Path::new(&kernel_dir);
    match read_pid(kdir) {
        None => "ERROR: No PID file found".to_string(),
        Some(pid) => {
            sigint_pid(pid);
            "ok".to_string()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn adopt_refuses_missing_dir() {
        let out = kernel_adopt("/tmp/nothelix-adopt-test-nonexistent".to_string());
        assert!(out.contains("\"status\":\"none\""), "{out}");
    }

    #[test]
    fn adopt_refuses_dead_or_foreign_pid() {
        let dir = std::env::temp_dir().join("nothelix-adopt-test-foreign");
        let _ = fs::create_dir_all(&dir);
        let mut child = Command::new("sleep")
            .arg("30")
            .spawn()
            .expect("spawn sleep");
        fs::write(dir.join("pid"), child.id().to_string()).expect("write pid");
        fs::write(dir.join("ready"), "").expect("write ready");
        let out = kernel_adopt(dir.to_string_lossy().into_owned());
        let _ = child.kill();
        let _ = child.wait();
        let _ = fs::remove_dir_all(&dir);
        assert!(
            out.contains("\"status\":\"none\""),
            "sleep is not runner.jl: {out}"
        );
    }
}
