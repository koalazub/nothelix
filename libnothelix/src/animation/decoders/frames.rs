use crate::animation::decoder::{
    AnimatedDecoder, AnimationMetadata, DecodedFrame, DecoderError, fit_dimensions_to_u16,
};
use std::sync::Arc;
use std::time::Duration;

const MIN_FRAME_DELAY_MS: u64 = 10;

#[cfg(test)]
pub(super) fn frame_at_ms(decoder: &mut dyn AnimatedDecoder, ms: u64) -> DecodedFrame {
    decoder
        .frame_at(Duration::from_millis(ms))
        .expect("decoding the frame succeeds")
        .expect("a frame is presented at this offset")
}

#[derive(Clone, Copy)]
pub(super) enum Looping {
    Always,
    WhenMultiFrame,
}

pub(super) struct FrameSequence {
    frames: Vec<DecodedFrame>,
    span: Duration,
    width: u16,
    height: u16,
}

impl FrameSequence {
    pub(super) fn new(width: u16, height: u16) -> Self {
        Self {
            frames: Vec::new(),
            span: Duration::ZERO,
            width,
            height,
        }
    }

    pub(super) fn is_empty(&self) -> bool {
        self.frames.is_empty()
    }

    pub(super) fn absorb(&mut self, frames: ::image::Frames<'_>) -> Result<(), DecoderError> {
        for frame in frames {
            let frame = frame?;
            let buffer = frame.buffer();
            let (width, height) = fit_dimensions_to_u16(buffer.dimensions())?;
            let rgba = buffer.as_raw().as_slice();
            let (numerator, denominator) = frame.delay().numer_denom_ms();
            let delay_ms = (numerator as u64) / (denominator as u64).max(1);
            self.push(rgba, width, height, delay_ms.max(MIN_FRAME_DELAY_MS));
        }
        Ok(())
    }

    pub(super) fn push_still_from(
        &mut self,
        decoder: impl ::image::ImageDecoder,
    ) -> Result<(), DecoderError> {
        let (width, height) = fit_dimensions_to_u16(decoder.dimensions())?;
        let rgba = ::image::DynamicImage::from_decoder(decoder)?
            .to_rgba8()
            .into_raw();
        self.push(&rgba, width, height, 0);
        Ok(())
    }

    fn push(&mut self, rgba: &[u8], width: u16, height: u16, delay_ms: u64) {
        let rgba: Arc<[u8]> = Arc::from(rgba);
        let content_id = hash_bytes(&rgba);
        self.width = width;
        self.height = height;
        self.frames.push(DecodedFrame {
            rgba,
            width,
            height,
            frame_index: self.frames.len() as u64,
            presentation_offset: self.span,
            content_id,
        });
        self.span += Duration::from_millis(delay_ms);
    }

    pub(super) fn into_source(self, looping: Looping) -> Box<dyn AnimatedDecoder> {
        let frame_count = self.frames.len() as u64;
        let total_duration = match looping {
            Looping::Always => self.span,
            Looping::WhenMultiFrame if frame_count > 1 => self.span,
            Looping::WhenMultiFrame => Duration::ZERO,
        };
        let native_fps = if total_duration.as_millis() == 0 {
            0.0
        } else {
            (frame_count as f32 * 1000.0) / total_duration.as_millis() as f32
        };
        Box::new(FrameListSource {
            frames: self.frames,
            metadata: AnimationMetadata {
                width: self.width,
                height: self.height,
                frame_count,
                native_fps,
                total_duration,
            },
        })
    }
}

struct FrameListSource {
    frames: Vec<DecodedFrame>,
    metadata: AnimationMetadata,
}

impl FrameListSource {
    fn presented_at(&self, elapsed: Duration) -> Option<DecodedFrame> {
        if self.frames.is_empty() {
            return None;
        }
        let span_ms = self.metadata.total_duration.as_millis() as u64;
        let offset = if span_ms == 0 {
            Duration::ZERO
        } else {
            Duration::from_millis((elapsed.as_millis() as u64) % span_ms)
        };
        let index = self
            .frames
            .partition_point(|frame| frame.presentation_offset <= offset)
            .saturating_sub(1);
        Some(self.frames[index].clone())
    }
}

impl AnimatedDecoder for FrameListSource {
    fn metadata(&self) -> AnimationMetadata {
        self.metadata.clone()
    }

    fn frame_at(&mut self, elapsed: Duration) -> Result<Option<DecodedFrame>, DecoderError> {
        Ok(self.presented_at(elapsed))
    }
}

fn hash_bytes(bytes: &[u8]) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    bytes.hash(&mut hasher);
    hasher.finish()
}
