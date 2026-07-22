use crate::animation::decoder::{DecoderError, lookup_decoder};
use crate::animation::engine::{AnimationEngine, PauseOutcome};
use crate::animation::renderer::{TerminalCaps, select_renderer};
use std::collections::HashMap;
use std::sync::{Mutex, MutexGuard, OnceLock, PoisonError};
use std::time::Instant;

#[derive(Debug, thiserror::Error)]
pub enum RegistrationError {
    #[error("no animation decoder registered for MIME `{mime}`")]
    UnknownMime { mime: String },
    #[error(transparent)]
    Decode(#[from] DecoderError),
}

impl RegistrationError {
    pub fn code(&self) -> i32 {
        match self {
            Self::UnknownMime { .. } => -2,
            Self::Decode(_) => -3,
        }
    }
}

pub struct AnimationRegistry {
    next_id: u64,
    engines: HashMap<u64, AnimationEngine>,
}

impl AnimationRegistry {
    pub fn new() -> Self {
        Self {
            next_id: 1,
            engines: HashMap::new(),
        }
    }

    pub fn install(&mut self, build: impl FnOnce(u64) -> AnimationEngine) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        self.engines.insert(id, build(id));
        id
    }

    pub fn get(&self, id: u64) -> Option<&AnimationEngine> {
        self.engines.get(&id)
    }

    pub fn get_mut(&mut self, id: u64) -> Option<&mut AnimationEngine> {
        self.engines.get_mut(&id)
    }

    pub fn drop_engine(&mut self, id: u64) -> Option<AnimationEngine> {
        self.engines.remove(&id)
    }
}

impl Default for AnimationRegistry {
    fn default() -> Self {
        Self::new()
    }
}

pub fn lock_registry() -> MutexGuard<'static, AnimationRegistry> {
    static REGISTRY: OnceLock<Mutex<AnimationRegistry>> = OnceLock::new();
    REGISTRY
        .get_or_init(|| Mutex::new(AnimationRegistry::new()))
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
}

pub fn register_engine(mime: &str, bytes: &[u8]) -> Result<u64, RegistrationError> {
    let decode = lookup_decoder(mime).ok_or_else(|| RegistrationError::UnknownMime {
        mime: mime.to_string(),
    })?;
    let decoder = decode(bytes)?;
    let renderer = select_renderer(&TerminalCaps::HOST_DEFAULT);
    Ok(lock_registry().install(|id| AnimationEngine::new(id, decoder, renderer)))
}

pub fn retire_engine(engine_id: u64) -> Vec<u8> {
    match lock_registry().drop_engine(engine_id) {
        Some(mut engine) => engine.teardown(),
        None => Vec::new(),
    }
}

pub fn set_engine_paused(engine_id: u64, paused: bool) -> PauseOutcome {
    match lock_registry().get_mut(engine_id) {
        Some(engine) => {
            engine.set_paused(paused, Instant::now());
            PauseOutcome::Applied
        }
        None => PauseOutcome::UnknownEngine,
    }
}
