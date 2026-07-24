mod player;
mod registry;

use crate::error::FFI_ERROR_PREFIX;
use player::resolved;
use std::process::{Command, Stdio};

fn refused(why: &str) -> String {
    format!("{FFI_ERROR_PREFIX}{why}")
}

pub fn audio_play(path: String) -> String {
    let Some((player, binary)) = resolved() else {
        return refused("no audio player found");
    };
    match Command::new(binary)
        .args(player.args(&path))
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
    {
        Ok(child) => {
            let pid = child.id();
            registry::insert(pid, child);
            pid.to_string()
        }
        Err(source) => refused(&format!("{}: {source}", player.binary())),
    }
}

pub fn audio_stop(pid: String) -> String {
    match pid.trim().parse::<u32>() {
        Ok(pid) if registry::stop(pid) => "stopped".to_string(),
        _ => refused("unknown pid"),
    }
}

pub fn audio_stop_all() -> String {
    registry::stop_all();
    "stopped".to_string()
}

pub fn audio_playing(pid: String) -> String {
    match pid.trim().parse::<u32>() {
        Ok(pid) => registry::playing(pid).to_string(),
        Err(_) => "false".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stopping_a_pid_that_was_never_spawned_is_an_error() {
        assert_eq!(audio_stop("999999999".to_string()), "ERROR: unknown pid");
    }

    #[test]
    fn stopping_a_non_numeric_handle_is_an_error() {
        assert_eq!(audio_stop("not-a-pid".to_string()), "ERROR: unknown pid");
    }

    #[test]
    fn an_unknown_pid_is_not_playing() {
        assert_eq!(audio_playing("999999999".to_string()), "false");
    }

    #[test]
    fn a_non_numeric_handle_is_not_playing() {
        assert_eq!(audio_playing("garbage".to_string()), "false");
    }

    #[test]
    fn stop_all_reports_stopped_even_with_nothing_playing() {
        assert_eq!(audio_stop_all(), "stopped");
    }
}
