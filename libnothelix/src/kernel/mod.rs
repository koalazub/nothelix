mod ipc;
mod process;
mod scripts;

use crate::error::{Error, KernelFault, Result, ffi};
use ipc::{Artifact, KernelDir, Reply};
use nix::sys::signal::Signal;
use serde_json::{Value, json};
use std::fs;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::Duration;
use which::which;

const SET_VAR_POLL_TRIES: u32 = 200;
const SET_VAR_POLL_INTERVAL: Duration = Duration::from_millis(5);

fn refused(reason: &Error) -> String {
    json!({"status": "none", "reason": reason.to_string()}).to_string()
}

fn failed(reason: &Error) -> String {
    json!({"status": "error", "error": reason.to_string()}).to_string()
}

pub fn kernel_adopt(kernel_dir: String) -> String {
    match adopt(&KernelDir::at(&kernel_dir), &kernel_dir) {
        Ok(pid) => json!({"status": "ok", "pid": pid}).to_string(),
        Err(reason) => refused(&reason),
    }
}

fn adopt(dir: &KernelDir, raw_dir: &str) -> Result<u32> {
    let pid = dir
        .recorded_pid()
        .ok_or_else(|| dir.fault(KernelFault::NoPidFile))?;
    if !process::is_live_runner(pid, raw_dir) {
        return Err(dir.fault(KernelFault::ProcessNotAlive { pid }));
    }
    if !dir.holds(Artifact::Ready) {
        return Err(dir.fault(KernelFault::NotReady));
    }
    dir.discard(&Artifact::IN_FLIGHT)?;
    Ok(pid)
}

pub fn kernel_start_macro(
    kernel_dir: String,
    julia_bin: String,
    julia_project: String,
    notebook_path: String,
) -> String {
    match start(
        &KernelDir::at(&kernel_dir),
        julia_bin.trim(),
        julia_project.trim(),
        notebook_path.trim(),
    ) {
        Ok(()) => json!({"status": "ok"}).to_string(),
        Err(reason) => failed(&reason),
    }
}

fn interpreter(dir: &KernelDir, julia_bin: &str) -> Result<PathBuf> {
    if julia_bin.is_empty() {
        return which("julia").map_err(|_| {
            dir.fault(KernelFault::InterpreterMissing {
                name: "julia".to_string(),
            })
        });
    }
    let configured = PathBuf::from(julia_bin);
    if configured.exists() {
        Ok(configured)
    } else {
        Err(Error::absent(configured))
    }
}

fn log_pipes(dir: &KernelDir) -> Result<(Stdio, Stdio)> {
    let path = dir.file(Artifact::Log);
    let log = fs::File::create(&path).map_err(|e| Error::creating(&path, e))?;
    let mirror = log.try_clone().map_err(|e| Error::creating(&path, e))?;
    Ok((Stdio::from(log), Stdio::from(mirror)))
}

fn start(dir: &KernelDir, julia_bin: &str, julia_project: &str, notebook_path: &str) -> Result<()> {
    if let Some(pid) = dir.recorded_pid() {
        process::signal_to(pid, Signal::SIGTERM);
    }
    dir.discard(&Artifact::SESSION)?;
    dir.create()?;

    let runner = scripts::install_runner()?;
    let julia = interpreter(dir, julia_bin)?;
    let (stdout, stderr) = log_pipes(dir)?;

    let mut command = Command::new(&julia);
    if !julia_project.is_empty() {
        command.arg(format!("--project={julia_project}"));
    }
    if !notebook_path.is_empty()
        && let Some(parent) = std::path::Path::new(notebook_path).parent()
        && let Ok(workdir) = fs::canonicalize(parent)
        && workdir.is_dir()
    {
        command.current_dir(workdir);
    }

    let child = command
        .arg(&runner)
        .arg(dir.path())
        .env("GKSwstype", "nul")
        .env("MPLBACKEND", "Agg")
        .stdin(Stdio::null())
        .stdout(stdout)
        .stderr(stderr)
        .spawn()
        .map_err(|e| Error::Subprocess {
            command: julia.display().to_string(),
            detail: e.to_string(),
        })?;

    dir.record_pid(child.id())
}

pub fn kernel_stop(kernel_dir: String) -> String {
    let dir = KernelDir::at(&kernel_dir);
    let Some(pid) = dir.recorded_pid() else {
        return "ok".to_string();
    };
    ffi(stop(&dir, pid).map(|()| "ok".to_string()))
}

fn stop(dir: &KernelDir, pid: u32) -> Result<()> {
    process::signal_to(pid, Signal::SIGTERM);
    dir.discard(&[Artifact::Pid])?;
    dir.discard(&Artifact::SPENT)
}

pub fn kernel_stop_all_processes() -> String {
    process::terminate_orphaned_runners();
    "All kernel processes stopped".to_string()
}

pub fn kernel_execute_cell_start(
    kernel_dir: String,
    cell_index: isize,
    code: String,
    plot_mode: String,
) -> String {
    let command = json!({
        "type": "execute_cell",
        "cell_index": cell_index,
        "code": code,
        "plot_mode": plot_mode,
    });
    match KernelDir::at(&kernel_dir).send(&command) {
        Ok(()) => json!({"status": "started"}).to_string(),
        Err(reason) => failed(&reason),
    }
}

pub fn kernel_poll_result(kernel_dir: String) -> String {
    match KernelDir::at(&kernel_dir).collect() {
        Ok(Reply::Pending) => json!({"status": "pending"}).to_string(),
        Ok(Reply::Ready(parsed)) => flatten(&parsed),
        Err(reason) => failed(&reason),
    }
}

fn flatten(parsed: &Value) -> String {
    let Some(status) = parsed
        .get("status")
        .and_then(Value::as_str)
        .filter(|status| *status != "error")
    else {
        return json!({
            "status": "error",
            "error": parsed.get("error").and_then(Value::as_str).unwrap_or("Unknown error"),
        })
        .to_string();
    };

    let cell = &parsed["cell"];
    let mut response = json!({
        "status":       status,
        "stdout":       cell["stdout"].as_str().unwrap_or(""),
        "stderr":       cell["stderr"].as_str().unwrap_or(""),
        "output_repr":  cell["output_repr"].as_str().unwrap_or(""),
        "has_error":    cell["has_error"].as_bool().unwrap_or(false),
        "error":        cell.get("error").and_then(Value::as_str).unwrap_or(""),
    });
    for field in [
        "images",
        "plot_data",
        "structured_error",
        "notes",
        "text_plots",
        "audio",
        "widgets",
    ] {
        if let Some(value) = cell.get(field) {
            response[field] = value.clone();
        }
    }
    if let Some(states) = parsed.get("cell_states") {
        response["cell_states"] = states.clone();
    }
    response.to_string()
}

pub fn kernel_interrupt(kernel_dir: String) -> String {
    let dir = KernelDir::at(&kernel_dir);
    ffi(dir
        .recorded_pid()
        .ok_or_else(|| dir.fault(KernelFault::NoPidFile))
        .map(|pid| {
            process::signal_to(pid, Signal::SIGINT);
            "ok".to_string()
        }))
}

fn is_plain_identifier(name: &str) -> bool {
    let mut chars = name.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    if !(first.is_ascii_alphabetic() || first == '_') {
        return false;
    }
    chars.all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '!')
}

fn set_var_command(name: &str, value: &str, cell_index: isize) -> Value {
    json!({
        "type": "set_var",
        "name": name,
        "value": value,
        "cell_index": cell_index,
    })
}

pub fn kernel_set_var(
    kernel_dir: String,
    name: String,
    value: String,
    cell_index: isize,
) -> String {
    if !is_plain_identifier(&name) {
        return json!({
            "status": "error",
            "error": format!("invalid variable name: {name}"),
        })
        .to_string();
    }
    let dir = KernelDir::at(&kernel_dir);
    match exchange(&dir, &set_var_command(&name, &value, cell_index)) {
        Ok(reply) => reply,
        Err(reason) => failed(&reason),
    }
}

fn exchange(dir: &KernelDir, command: &Value) -> Result<String> {
    dir.send(command)?;
    for _ in 0..SET_VAR_POLL_TRIES {
        match dir.collect()? {
            Reply::Ready(parsed) => return Ok(parsed.to_string()),
            Reply::Pending => std::thread::sleep(SET_VAR_POLL_INTERVAL),
        }
    }
    Err(dir.fault(KernelFault::NoReply))
}

fn installed_runner_path() -> Option<PathBuf> {
    let home = std::env::var("HOME").ok()?;
    Some(PathBuf::from(home).join(".local/share/nothelix/kernel/runner.jl"))
}

fn mtime(path: &std::path::Path) -> Option<std::time::SystemTime> {
    fs::metadata(path).and_then(|meta| meta.modified()).ok()
}

fn runner_is_stale(ready: std::time::SystemTime, runner: std::time::SystemTime) -> bool {
    ready < runner
}

fn runner_stale_verdict(
    ready_path: &std::path::Path,
    runner_path: &std::path::Path,
) -> &'static str {
    match (mtime(ready_path), mtime(runner_path)) {
        (Some(ready), Some(runner)) if runner_is_stale(ready, runner) => "yes",
        _ => "no",
    }
}

pub fn kernel_runner_stale(kernel_dir: String) -> String {
    let dir = KernelDir::at(&kernel_dir);
    let Some(runner_path) = installed_runner_path() else {
        return "ERROR: no HOME to locate the installed runner".to_string();
    };
    runner_stale_verdict(&dir.file(Artifact::Ready), &runner_path).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    use std::time::SystemTime;

    #[test]
    fn plain_identifiers_accepted_and_the_rest_rejected() {
        assert!(is_plain_identifier("freq"));
        assert!(is_plain_identifier("_x"));
        assert!(is_plain_identifier("x1_var!"));
        assert!(!is_plain_identifier(""));
        assert!(!is_plain_identifier("1x"));
        assert!(!is_plain_identifier("a b"));
        assert!(!is_plain_identifier("a.b"));
        assert!(!is_plain_identifier("a=1"));
        assert!(!is_plain_identifier("x; rm -rf"));
    }

    #[test]
    fn set_var_command_carries_name_value_and_cell_index() {
        let cmd = set_var_command("freq", "450", 3);
        assert_eq!(cmd["type"], "set_var");
        assert_eq!(cmd["name"], "freq");
        assert_eq!(cmd["value"], "450");
        assert_eq!(cmd["cell_index"], 3);
    }

    #[test]
    fn set_var_rejects_a_non_identifier_before_touching_the_kernel() {
        let out = kernel_set_var(
            "/tmp/nothelix-setvar-nonexistent".to_string(),
            "bad name".to_string(),
            "1".to_string(),
            0,
        );
        assert!(out.contains("\"status\":\"error\""), "{out}");
        assert!(out.contains("invalid variable name"), "{out}");
    }

    #[test]
    fn runner_is_stale_only_when_ready_predates_the_runner() {
        let base = SystemTime::UNIX_EPOCH;
        let later = base + Duration::from_secs(10);
        assert!(runner_is_stale(base, later));
        assert!(!runner_is_stale(later, base));
        assert!(!runner_is_stale(base, base));
    }

    fn write_newer_than(path: &Path, reference: SystemTime) {
        for _ in 0..10_000 {
            fs::write(path, "x").unwrap();
            if mtime(path).unwrap() > reference {
                return;
            }
        }
        panic!("filesystem mtime never advanced past the reference");
    }

    #[test]
    fn runner_stale_verdict_yes_when_ready_predates_the_runner() {
        let dir = std::env::temp_dir().join(format!("nothelix-stale-yes-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let ready = dir.join("ready");
        let runner = dir.join("runner.jl");
        fs::write(&ready, "").unwrap();
        write_newer_than(&runner, mtime(&ready).unwrap());
        assert_eq!(runner_stale_verdict(&ready, &runner), "yes");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn runner_stale_verdict_no_when_ready_is_newer_than_the_runner() {
        let dir = std::env::temp_dir().join(format!("nothelix-stale-no-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let ready = dir.join("ready");
        let runner = dir.join("runner.jl");
        fs::write(&runner, "# runner").unwrap();
        write_newer_than(&ready, mtime(&runner).unwrap());
        assert_eq!(runner_stale_verdict(&ready, &runner), "no");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn runner_stale_verdict_no_when_either_file_is_absent() {
        let dir =
            std::env::temp_dir().join(format!("nothelix-stale-absent-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let ready = dir.join("ready");
        let runner = dir.join("runner.jl");
        fs::write(&runner, "# runner").unwrap();
        assert_eq!(runner_stale_verdict(&ready, &runner), "no");
        let _ = fs::remove_dir_all(&dir);
    }

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

    #[test]
    fn execute_cell_start_writes_plot_mode_into_input_json() {
        let dir = std::env::temp_dir().join("nothelix-execute-plot-mode-test");
        let _ = fs::create_dir_all(&dir);
        let out = kernel_execute_cell_start(
            dir.to_string_lossy().into_owned(),
            3,
            "1+1".to_string(),
            "braille".to_string(),
        );
        let written = fs::read_to_string(dir.join("input.json")).expect("read input.json");
        let _ = fs::remove_dir_all(&dir);
        assert!(out.contains("\"status\":\"started\""), "{out}");
        assert!(written.contains("\"plot_mode\":\"braille\""), "{written}");
        assert!(written.contains("\"cell_index\":3"), "{written}");
    }

    #[test]
    fn interrupt_without_a_pid_file_names_the_kernel_directory() {
        let out = kernel_interrupt("/tmp/nothelix-interrupt-test-nonexistent".to_string());
        assert!(out.starts_with("ERROR:"), "{out}");
        assert!(out.contains("no pid file"), "{out}");
    }

    #[test]
    fn poll_without_a_done_marker_is_pending() {
        let out = kernel_poll_result("/tmp/nothelix-poll-test-nonexistent".to_string());
        assert!(out.contains("\"status\":\"pending\""), "{out}");
    }

    #[test]
    fn flatten_forwards_top_level_cell_states_and_cell_notes() {
        let parsed = json!({
            "status": "ok",
            "cell": {
                "stdout": "hi", "stderr": "", "output_repr": "3",
                "has_error": false,
                "notes": ["note: A below"],
                "text_plots": [{"rows": ["x"], "spans": []}],
            },
            "cell_states": {
                "0": {"state": "fresh", "inputs": []},
                "3": {"state": "out-of-order", "inputs": [{"name": "A", "writer": 5, "rel": "below"}]},
            },
        });
        let flat: Value = serde_json::from_str(&flatten(&parsed)).expect("flat json");
        assert_eq!(flat["cell_states"]["3"]["state"], "out-of-order");
        assert_eq!(flat["notes"][0], "note: A below");
        assert_eq!(flat["text_plots"][0]["rows"][0], "x");
    }
}
