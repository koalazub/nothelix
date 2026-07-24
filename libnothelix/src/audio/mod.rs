mod player;
mod registry;
mod wav;
mod waveform;

pub use waveform::audio_waveform;

use crate::error::FFI_ERROR_PREFIX;
use player::resolved;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};

fn refused(why: &str) -> String {
    format!("{FFI_ERROR_PREFIX}{why}")
}

fn spawn_detached(binary: &Path, args: Vec<String>) -> std::io::Result<Child> {
    Command::new(binary)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
}

pub fn audio_play(path: String) -> String {
    let Some((player, binary)) = resolved() else {
        return refused("no audio player found");
    };
    match spawn_detached(binary, player.args(&path)) {
        Ok(child) => {
            let pid = child.id();
            registry::insert(pid, child);
            pid.to_string()
        }
        Err(source) => refused(&format!("{}: {source}", player.binary())),
    }
}

fn scrub_path_for(path: &str) -> PathBuf {
    match Path::new(path).parent() {
        Some(dir) => dir.join("scrub.wav"),
        None => PathBuf::from("scrub.wav"),
    }
}

pub fn audio_play_from(path: String, offset_ms: isize) -> String {
    let offset = offset_ms.max(0) as u64;
    let bytes = match std::fs::read(&path) {
        Ok(bytes) => bytes,
        Err(source) => return refused(&format!("cannot read {path}: {source}")),
    };
    let sliced = match wav::slice_pcm16(&bytes, offset) {
        Ok(sliced) => sliced,
        Err(error) => return refused(&error.message()),
    };
    let scrub = scrub_path_for(&path);
    if let Err(source) = std::fs::write(&scrub, &sliced) {
        return refused(&format!("cannot write {}: {source}", scrub.display()));
    }
    let Some((player, binary)) = resolved() else {
        return refused("no audio player found");
    };
    let scrub = scrub.to_string_lossy().into_owned();
    match spawn_detached(binary, player.args(&scrub)) {
        Ok(child) => {
            let pid = child.id();
            registry::insert_at(pid, child, offset);
            pid.to_string()
        }
        Err(source) => refused(&format!("{}: {source}", player.binary())),
    }
}

pub fn audio_position(pid: String) -> String {
    match pid.trim().parse::<u32>() {
        Ok(pid) => match registry::elapsed_ms(pid) {
            Some(ms) => ms.to_string(),
            None => refused("unknown pid"),
        },
        Err(_) => refused("unknown pid"),
    }
}

pub fn audio_info(path: String) -> String {
    let bytes = match std::fs::read(&path) {
        Ok(bytes) => bytes,
        Err(source) => return refused(&format!("cannot read {path}: {source}")),
    };
    match wav::parse_pcm16(&bytes) {
        Ok(wav) => format!("{}\t{}", wav.rate, wav.channels),
        Err(error) => refused(&error.message()),
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
