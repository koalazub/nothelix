use nix::sys::signal::{self, Signal};
use nix::unistd::Pid;
use std::process::Command;

pub(super) fn signal_to(pid: u32, sig: Signal) {
    if signal::kill(Pid::from_raw(pid as i32), sig).is_err() {
        let _ = Command::new("kill")
            .args([&format!("-{}", sig.as_str()), &pid.to_string()])
            .output();
    }
}

pub(super) fn is_live_runner(pid: u32, kernel_dir: &str) -> bool {
    if signal::kill(Pid::from_raw(pid as i32), None).is_err() {
        return false;
    }
    let Ok(output) = Command::new("ps")
        .args(["-p", &pid.to_string(), "-o", "command="])
        .output()
    else {
        return false;
    };
    if !output.status.success() {
        return false;
    }
    let command = String::from_utf8_lossy(&output.stdout);
    command.contains("runner.jl") && command.contains(kernel_dir)
}

pub(super) fn terminate_orphaned_runners() {
    let _ = Command::new("pkill")
        .args(["-SIGTERM", "-f", "runner.jl"])
        .output();
    let _ = Command::new("sh")
        .args(["-c", "rm -rf /tmp/helix-kernel-*"])
        .output();
}
