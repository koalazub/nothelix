use std::collections::HashMap;
use std::process::Child;
use std::sync::{Mutex, MutexGuard, OnceLock, PoisonError};
use std::time::Instant;

struct Entry {
    child: Child,
    started: Instant,
    offset_ms: u64,
}

fn store() -> MutexGuard<'static, HashMap<u32, Entry>> {
    static STORE: OnceLock<Mutex<HashMap<u32, Entry>>> = OnceLock::new();
    STORE
        .get_or_init(|| Mutex::new(HashMap::new()))
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
}

pub fn insert(pid: u32, child: Child) {
    insert_at(pid, child, 0);
}

pub fn insert_at(pid: u32, child: Child, offset_ms: u64) {
    store().insert(
        pid,
        Entry {
            child,
            started: Instant::now(),
            offset_ms,
        },
    );
}

pub fn stop(pid: u32) -> bool {
    match store().remove(&pid) {
        Some(mut entry) => {
            let _ = entry.child.kill();
            let _ = entry.child.wait();
            true
        }
        None => false,
    }
}

pub fn stop_all() {
    for (_, mut entry) in store().drain() {
        let _ = entry.child.kill();
        let _ = entry.child.wait();
    }
}

pub fn playing(pid: u32) -> bool {
    let mut guard = store();
    let finished = match guard.get_mut(&pid) {
        Some(entry) => matches!(entry.child.try_wait(), Ok(Some(_)) | Err(_)),
        None => return false,
    };
    if finished {
        guard.remove(&pid);
    }
    !finished
}

pub fn elapsed_ms(pid: u32) -> Option<u64> {
    let guard = store();
    let entry = guard.get(&pid)?;
    Some(entry.offset_ms + entry.started.elapsed().as_millis() as u64)
}
