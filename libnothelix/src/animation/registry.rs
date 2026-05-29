use crate::animation::engine::AnimationEngine;
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

pub struct AnimationRegistry {
    next_id: u64,
    engines: HashMap<u64, AnimationEngine>,
}

impl AnimationRegistry {
    pub fn new() -> Self {
        Self { next_id: 1, engines: HashMap::new() }
    }
    pub fn allocate_id(&mut self) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        id
    }
    pub fn insert(&mut self, id: u64, engine: AnimationEngine) {
        self.engines.insert(id, engine);
    }
    pub fn get_mut(&mut self, id: u64) -> Option<&mut AnimationEngine> {
        self.engines.get_mut(&id)
    }
    pub fn get(&self, id: u64) -> Option<&AnimationEngine> {
        self.engines.get(&id)
    }
    pub fn drop_engine(&mut self, id: u64) -> Option<AnimationEngine> {
        self.engines.remove(&id)
    }
}

impl Default for AnimationRegistry {
    fn default() -> Self { Self::new() }
}

static REGISTRY: OnceLock<Mutex<AnimationRegistry>> = OnceLock::new();

pub fn registry() -> &'static Mutex<AnimationRegistry> {
    REGISTRY.get_or_init(|| Mutex::new(AnimationRegistry::new()))
}

/// Acquire the registry mutex, recovering from poison.
///
/// The registry is just `HashMap<u64, AnimationEngine>` — there's no
/// invariant a paniced thread could have left in an unsafe state. The
/// engines themselves carry per-tick state with no cross-engine
/// invariants. So on poison we proceed with the inner guard rather than
/// propagate the panic across the FFI boundary (which is UB in C).
pub fn lock_registry() -> std::sync::MutexGuard<'static, AnimationRegistry> {
    registry()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}
