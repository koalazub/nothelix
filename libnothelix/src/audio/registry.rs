use std::collections::HashMap;
use std::process::Child;
use std::sync::{Mutex, MutexGuard, OnceLock, PoisonError};

fn store() -> MutexGuard<'static, HashMap<u32, Child>> {
    static STORE: OnceLock<Mutex<HashMap<u32, Child>>> = OnceLock::new();
    STORE
        .get_or_init(|| Mutex::new(HashMap::new()))
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
}

pub fn insert(pid: u32, child: Child) {
    store().insert(pid, child);
}

pub fn stop(pid: u32) -> bool {
    match store().remove(&pid) {
        Some(mut child) => {
            let _ = child.kill();
            let _ = child.wait();
            true
        }
        None => false,
    }
}

pub fn stop_all() {
    for (_, mut child) in store().drain() {
        let _ = child.kill();
        let _ = child.wait();
    }
}

pub fn playing(pid: u32) -> bool {
    let mut guard = store();
    let finished = match guard.get_mut(&pid) {
        Some(child) => matches!(child.try_wait(), Ok(Some(_)) | Err(_)),
        None => return false,
    };
    if finished {
        guard.remove(&pid);
    }
    !finished
}
