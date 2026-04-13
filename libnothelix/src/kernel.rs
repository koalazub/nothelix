//! Kernel lifecycle management.
//!
//! Manages Julia kernel processes via file-based IPC:
//!   - `input.json`      — command written by Rust
//!   - `output.json`     — result written by Julia
//!   - `output.json.done`— sentinel file signalling completion
//!   - `pid`             — PID of the running Julia process
//!
//! Signals are sent via `nix` for proper POSIX semantics (SIGTERM / SIGINT).

use std::{fs, path::Path, process::Command};

use nix::{
    sys::signal::{self, Signal},
    unistd::Pid,
};
use serde_json::{json, Value};
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

// ─── IPC helpers ──────────────────────────────────────────────────────────────

pub fn write_kernel_command(kernel_dir: &str, cmd: &Value) -> Result<(), String> {
    let input_file = Path::new(kernel_dir).join("input.json");
    // Remove any stale done marker before writing a new command.
    let _ = fs::remove_file(Path::new(kernel_dir).join("output.json.done"));
    fs::write(&input_file, cmd.to_string()).map_err(|e| format!("Cannot write input.json: {e}"))
}

// ─── FFI-facing functions ─────────────────────────────────────────────────────

pub fn kernel_start_macro(kernel_dir: String) -> String {
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

    let julia = match which("julia") {
        Ok(p) => p,
        Err(_) => {
            return json!({"status": "error", "error": "julia not found in PATH"}).to_string()
        }
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
    match Command::new(&julia)
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
    for f in &["input.json", "output.json", "output.json.done", "ready"] {
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
    let done_file = Path::new(&kernel_dir).join("output.json.done");
    let output_file = Path::new(&kernel_dir).join("output.json");

    if !done_file.exists() {
        return json!({"status": "pending"}).to_string();
    }

    let _ = fs::remove_file(&done_file);

    let content = match fs::read_to_string(&output_file) {
        Ok(c) => c,
        Err(e) => return json!({"status": "error", "error": e.to_string()}).to_string(),
    };

    let parsed: Value = match serde_json::from_str(&content) {
        Ok(v) => v,
        Err(e) => return json!({"status": "error", "error": e.to_string()}).to_string(),
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
