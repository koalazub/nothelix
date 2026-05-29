use crate::animation::cache::FrameCache;
use crate::animation::decoder::{AnimatedDecoder, AnimationMetadata};
use crate::animation::renderer::{AnimationRenderer, RenderContext};
use std::time::{Duration, Instant};

pub enum PlaybackState {
    Playing {
        started_at: Instant,
        accumulated_paused: Duration,
    },
    Paused {
        at_offset: Duration,
    },
    Errored {
        reason: String,
    },
    Finished,
}

pub struct TickOutput {
    pub bytes: Vec<u8>,
    pub height: u16,
    pub next_delay_ms: u32,
    pub frame_index: u64,
}

/// Metadata snapshot from the most recent `tick()` call. Used by the Steel
/// FFI accessor functions (`animation_tick_status`, etc.) which read this
/// immediately after calling `animation_tick_bytes`.
///
/// Fields use `isize` because that is Steel's native integer type — both
/// `FromFFIArg` and `Into<FFIValue>` are implemented for `isize`, making it
/// the only integer type usable as both argument and return value in `register_fn`.
#[derive(Clone, Default)]
pub struct TickMetaSnapshot {
    /// 0 = new frame bytes, 1 = no change (same frame), 2 = finished/paused, <0 = error
    pub status: isize,
    pub height: isize,
    pub next_delay_ms: isize,
    pub frame_index: isize,
}

pub struct AnimationEngine {
    pub id: u64,
    decoder: Box<dyn AnimatedDecoder>,
    renderer: Box<dyn AnimationRenderer>,
    #[allow(dead_code)]
    cache: FrameCache,
    metadata: AnimationMetadata,
    state: PlaybackState,
    last_content_id: Option<u64>,
    /// Set on every `tick()` call so Steel-side accessors can read it immediately after.
    pub last_tick_meta: TickMetaSnapshot,
    /// Frame bytes from the most recent `tick()`. Cached so the Steel
    /// API can split "advance the engine" (`animation-tick`) from
    /// "read the bytes the last advance produced"
    /// (`animation-tick-bytes`) without re-advancing — otherwise a
    /// `(animation-tick) (animation-tick-bytes)` pair from Scheme would
    /// step the engine twice and skip every other frame.
    pub last_tick_bytes: Vec<u8>,
}

impl AnimationEngine {
    pub fn new(
        id: u64,
        decoder: Box<dyn AnimatedDecoder>,
        renderer: Box<dyn AnimationRenderer>,
        cache_budget_bytes: usize,
    ) -> Self {
        let metadata = decoder.metadata();
        Self {
            id,
            decoder,
            renderer,
            cache: FrameCache::new(cache_budget_bytes),
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

    pub fn metadata(&self) -> &AnimationMetadata {
        &self.metadata
    }
    pub fn state(&self) -> &PlaybackState {
        &self.state
    }

    pub fn pause(&mut self, now: Instant) {
        if let PlaybackState::Playing {
            started_at,
            accumulated_paused,
        } = &self.state
        {
            let elapsed = now
                .saturating_duration_since(*started_at)
                .saturating_sub(*accumulated_paused);
            self.state = PlaybackState::Paused { at_offset: elapsed };
        }
    }

    pub fn resume(&mut self, now: Instant) {
        if let PlaybackState::Paused { at_offset } = &self.state {
            let started_at = now - *at_offset;
            self.state = PlaybackState::Playing {
                started_at,
                accumulated_paused: Duration::ZERO,
            };
        }
    }

    pub fn tick(&mut self, now: Instant) -> Option<TickOutput> {
        let elapsed = if let PlaybackState::Playing {
            started_at,
            accumulated_paused,
        } = &self.state
        {
            now.saturating_duration_since(*started_at)
                .saturating_sub(*accumulated_paused)
        } else {
            self.last_tick_meta = TickMetaSnapshot {
                status: 2,
                height: 0,
                next_delay_ms: 0,
                frame_index: 0,
            };
            self.last_tick_bytes.clear();
            return None;
        };
        let frame = match self.decoder.frame_at(elapsed) {
            Ok(Some(f)) => f,
            Ok(None) => {
                self.state = PlaybackState::Finished;
                self.last_tick_meta = TickMetaSnapshot {
                    status: 2,
                    height: 0,
                    next_delay_ms: 0,
                    frame_index: 0,
                };
                self.last_tick_bytes.clear();
                return None;
            }
            Err(e) => {
                self.state = PlaybackState::Errored {
                    reason: e.to_string(),
                };
                self.last_tick_meta = TickMetaSnapshot {
                    status: -1,
                    height: 0,
                    next_delay_ms: 0,
                    frame_index: 0,
                };
                self.last_tick_bytes.clear();
                return None;
            }
        };
        let bytes = if Some(frame.content_id) == self.last_content_id {
            Vec::new()
        } else {
            let ctx = RenderContext {
                engine_id: self.id,
                cell_position: (0, 0),
                previous_content_id: self.last_content_id,
            };
            self.renderer.transmit_frame(&frame, &ctx)
        };
        self.last_content_id = Some(frame.content_id);
        let next_delay_ms = compute_next_delay(&self.metadata);
        let height = ((frame.height as f32) / 16.0).ceil() as u16;
        let status: isize = if bytes.is_empty() { 1 } else { 0 };
        self.last_tick_meta = TickMetaSnapshot {
            status,
            height: height as isize,
            next_delay_ms: next_delay_ms as isize,
            frame_index: frame.frame_index as isize,
        };
        self.last_tick_bytes = bytes.clone();
        Some(TickOutput {
            bytes,
            height,
            next_delay_ms,
            frame_index: frame.frame_index,
        })
    }

    pub fn teardown(&mut self) -> Vec<u8> {
        self.renderer.teardown(self.id)
    }
}

fn compute_next_delay(meta: &AnimationMetadata) -> u32 {
    let fps = if meta.native_fps > 0.0 {
        meta.native_fps
    } else {
        30.0
    };
    ((1000.0 / fps).round() as u32).max(8)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::animation::decoders::gif::GifSource;
    use crate::animation::decoders::gif_fixture::tiny_gif_bytes;
    use crate::animation::renderer::TerminalCaps;
    use crate::animation::renderers::static_fallback::StaticFallbackRenderer;

    #[test]
    fn tick_returns_frame_when_playing() {
        let dec = GifSource::open(&tiny_gif_bytes()).unwrap();
        let r = StaticFallbackRenderer::try_new(&TerminalCaps::default()).unwrap();
        let mut eng = AnimationEngine::new(1, dec, r, 1_000_000);
        let now = Instant::now();
        let out = eng.tick(now).expect("first tick produces a frame");
        assert!(!out.bytes.is_empty());
    }

    #[test]
    fn paused_tick_returns_none() {
        let dec = GifSource::open(&tiny_gif_bytes()).unwrap();
        let r = StaticFallbackRenderer::try_new(&TerminalCaps::default()).unwrap();
        let mut eng = AnimationEngine::new(1, dec, r, 1_000_000);
        let now = Instant::now();
        eng.pause(now);
        assert!(eng.tick(now + Duration::from_millis(50)).is_none());
    }

    #[test]
    fn resume_restores_playing() {
        let dec = GifSource::open(&tiny_gif_bytes()).unwrap();
        let r = StaticFallbackRenderer::try_new(&TerminalCaps::default()).unwrap();
        let mut eng = AnimationEngine::new(1, dec, r, 1_000_000);
        let now = Instant::now();
        eng.pause(now);
        eng.resume(now + Duration::from_millis(20));
        let out = eng.tick(now + Duration::from_millis(30));
        assert!(out.is_some());
    }
}
