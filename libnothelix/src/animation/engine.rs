use crate::animation::decoder::{AnimatedDecoder, AnimationMetadata};
use crate::animation::renderer::AnimationRenderer;
use crate::error::Error;
use std::time::{Duration, Instant};

const CELL_PIXEL_HEIGHT: u16 = 16;

pub enum PlaybackState {
    Playing {
        started_at: Instant,
        accumulated_paused: Duration,
    },
    Paused {
        at_offset: Duration,
    },
    Errored {
        cause: Error,
    },
    Finished,
}

#[derive(Clone, Copy, Default, PartialEq, Eq, Debug)]
pub enum TickStatus {
    #[default]
    NewFrame,
    NoChange,
    Stopped,
    Faulted,
    UnknownEngine,
}

impl TickStatus {
    pub fn code(self) -> i32 {
        match self {
            Self::NewFrame => 0,
            Self::NoChange => 1,
            Self::Stopped => 2,
            Self::Faulted => -1,
            Self::UnknownEngine => -2,
        }
    }
}

#[derive(Clone, Copy)]
pub enum PauseOutcome {
    Applied,
    UnknownEngine,
}

impl PauseOutcome {
    pub fn code(self) -> i32 {
        match self {
            Self::Applied => 0,
            Self::UnknownEngine => -1,
        }
    }
}

pub struct TickOutput<'a> {
    pub bytes: &'a [u8],
    pub height: u16,
    pub next_delay_ms: u32,
}

#[derive(Clone, Copy, Default)]
pub struct TickMetaSnapshot {
    pub status: TickStatus,
    pub height: u16,
    pub next_delay_ms: u32,
    pub frame_index: u64,
}

pub struct AnimationEngine {
    pub id: u64,
    decoder: Box<dyn AnimatedDecoder>,
    renderer: Box<dyn AnimationRenderer>,
    metadata: AnimationMetadata,
    state: PlaybackState,
    last_content_id: Option<u64>,
    pub last_tick_meta: TickMetaSnapshot,
    pub last_tick_bytes: Vec<u8>,
}

impl AnimationEngine {
    pub fn new(
        id: u64,
        decoder: Box<dyn AnimatedDecoder>,
        renderer: Box<dyn AnimationRenderer>,
    ) -> Self {
        let metadata = decoder.metadata();
        Self {
            id,
            decoder,
            renderer,
            metadata,
            state: PlaybackState::Playing {
                started_at: Instant::now(),
                accumulated_paused: Duration::ZERO,
            },
            last_content_id: None,
            last_tick_meta: TickMetaSnapshot::default(),
            last_tick_bytes: Vec::new(),
        }
    }

    pub fn set_paused(&mut self, paused: bool, now: Instant) {
        match (&self.state, paused) {
            (
                PlaybackState::Playing {
                    started_at,
                    accumulated_paused,
                },
                true,
            ) => {
                let at_offset = now
                    .saturating_duration_since(*started_at)
                    .saturating_sub(*accumulated_paused);
                self.state = PlaybackState::Paused { at_offset };
            }
            (PlaybackState::Paused { at_offset }, false) => {
                self.state = PlaybackState::Playing {
                    started_at: now - *at_offset,
                    accumulated_paused: Duration::ZERO,
                };
            }
            _ => {}
        }
    }

    pub fn tick(&mut self, now: Instant) -> Option<TickOutput<'_>> {
        let Some(elapsed) = self.playing_offset(now) else {
            self.halt(TickStatus::Stopped);
            return None;
        };
        let frame = match self.decoder.frame_at(elapsed) {
            Ok(Some(frame)) => frame,
            Ok(None) => {
                self.state = PlaybackState::Finished;
                self.halt(TickStatus::Stopped);
                return None;
            }
            Err(cause) => {
                self.state = PlaybackState::Errored {
                    cause: cause.into(),
                };
                self.halt(TickStatus::Faulted);
                return None;
            }
        };
        let bytes = if Some(frame.content_id) == self.last_content_id {
            Vec::new()
        } else {
            match self.renderer.transmit_frame(&frame, self.id) {
                Ok(bytes) => bytes,
                Err(cause) => {
                    self.state = PlaybackState::Errored { cause };
                    self.halt(TickStatus::Faulted);
                    return None;
                }
            }
        };
        self.last_content_id = Some(frame.content_id);
        let height = frame.height.div_ceil(CELL_PIXEL_HEIGHT);
        let next_delay_ms = self.metadata.tick_interval_ms();
        self.last_tick_meta = TickMetaSnapshot {
            status: if bytes.is_empty() {
                TickStatus::NoChange
            } else {
                TickStatus::NewFrame
            },
            height,
            next_delay_ms,
            frame_index: frame.frame_index,
        };
        self.last_tick_bytes = bytes;
        Some(TickOutput {
            bytes: &self.last_tick_bytes,
            height,
            next_delay_ms,
        })
    }

    pub fn teardown(&mut self) -> Vec<u8> {
        self.renderer.teardown(self.id)
    }

    fn playing_offset(&self, now: Instant) -> Option<Duration> {
        match &self.state {
            PlaybackState::Playing {
                started_at,
                accumulated_paused,
            } => Some(
                now.saturating_duration_since(*started_at)
                    .saturating_sub(*accumulated_paused),
            ),
            _ => None,
        }
    }

    fn halt(&mut self, status: TickStatus) {
        self.last_tick_meta = TickMetaSnapshot {
            status,
            ..TickMetaSnapshot::default()
        };
        self.last_tick_bytes.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::animation::decoders::gif;
    use crate::animation::decoders::gif_fixture::tiny_gif_bytes;
    use crate::animation::renderers::static_fallback::StaticFallbackRenderer;

    fn gif_engine() -> AnimationEngine {
        let decoder = gif::open(&tiny_gif_bytes()).unwrap();
        AnimationEngine::new(1, decoder, StaticFallbackRenderer::boxed())
    }

    #[test]
    fn tick_returns_frame_when_playing() {
        let mut engine = gif_engine();
        let output = engine
            .tick(Instant::now())
            .expect("first tick emits a frame");
        assert!(!output.bytes.is_empty());
    }

    #[test]
    fn paused_tick_returns_none() {
        let mut engine = gif_engine();
        let now = Instant::now();
        engine.set_paused(true, now);
        assert!(engine.tick(now + Duration::from_millis(50)).is_none());
    }

    #[test]
    fn resume_restores_playing() {
        let mut engine = gif_engine();
        let now = Instant::now();
        engine.set_paused(true, now);
        engine.set_paused(false, now + Duration::from_millis(20));
        assert!(engine.tick(now + Duration::from_millis(30)).is_some());
    }
}
