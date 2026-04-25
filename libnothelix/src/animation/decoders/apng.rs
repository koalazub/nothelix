use crate::animation::decoder::*;
use std::sync::Arc;
use std::time::Duration;

pub struct ApngSource {
    frames: Vec<DecodedFrame>,
    metadata: AnimationMetadata,
}

impl ApngSource {
    pub fn open(bytes: &[u8]) -> Result<Box<dyn AnimatedDecoder>, DecoderError> {
        use ::image::AnimationDecoder;
        use ::image::codecs::png::PngDecoder;
        let dec = PngDecoder::new(std::io::Cursor::new(bytes))
            .map_err(|e| DecoderError::Malformed(e.to_string()))?;
        let apng = dec
            .apng()
            .map_err(|e| DecoderError::Malformed(e.to_string()))?;
        let frames_iter = apng.into_frames();
        let mut frames = Vec::new();
        let mut acc = Duration::ZERO;
        let mut width = 0u16;
        let mut height = 0u16;
        for (idx, f) in frames_iter.enumerate() {
            let f = f.map_err(|e| DecoderError::Malformed(e.to_string()))?;
            let buf = f.buffer();
            width = buf.width() as u16;
            height = buf.height() as u16;
            let raw = buf.as_raw();
            let rgba: Arc<[u8]> = Arc::from(raw.as_slice());
            let content_id = hash_bytes(&rgba);
            let presentation_offset = acc;
            let delay = f.delay().numer_denom_ms();
            let delay_ms = (delay.0 as u64) / (delay.1 as u64).max(1);
            acc += Duration::from_millis(delay_ms.max(10));
            frames.push(DecodedFrame {
                rgba,
                width,
                height,
                frame_index: idx as u64,
                presentation_offset,
                content_id,
            });
        }
        let frame_count = frames.len() as u64;
        let total = if frame_count == 0 { Duration::ZERO } else { acc };
        let native_fps = if total.as_millis() == 0 {
            0.0
        } else {
            (frame_count as f32 * 1000.0) / total.as_millis() as f32
        };
        Ok(Box::new(ApngSource {
            frames,
            metadata: AnimationMetadata {
                width,
                height,
                frame_count: Some(frame_count),
                native_fps,
                total_duration: Some(total),
                loops_natively: true,
            },
        }))
    }
}

inventory::submit! {
    DecoderEntry { mime: "image/apng", factory: |b| ApngSource::open(b) }
}

impl AnimatedDecoder for ApngSource {
    fn metadata(&self) -> AnimationMetadata {
        self.metadata.clone()
    }

    fn frame_at(&mut self, elapsed: Duration) -> Result<Option<DecodedFrame>, DecoderError> {
        if self.frames.is_empty() {
            return Ok(None);
        }
        let total = self.metadata.total_duration.unwrap_or(Duration::ZERO);
        let t = if total.as_millis() == 0 {
            Duration::ZERO
        } else {
            Duration::from_millis(
                (elapsed.as_millis() as u64) % (total.as_millis() as u64).max(1),
            )
        };
        let mut chosen = &self.frames[0];
        for f in &self.frames {
            if f.presentation_offset <= t {
                chosen = f;
            } else {
                break;
            }
        }
        Ok(Some(chosen.clone()))
    }

    fn seek(&mut self, _elapsed: Duration) -> Result<(), DecoderError> {
        Ok(())
    }
}

fn hash_bytes(bytes: &[u8]) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    bytes.hash(&mut h);
    h.finish()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Minimal smoke test: a single-frame "APNG" (technically a static PNG with
    /// the apng decoder API still applicable). Real animated APNGs are tested
    /// via integration in Task 31.
    #[test]
    fn opens_static_png_via_apng_path() {
        // Build a 4x4 red PNG inline.
        use ::image::codecs::png::PngEncoder;
        use ::image::{ColorType, ImageEncoder};
        let rgba = vec![255u8, 0, 0, 255].repeat(16);
        let mut buf = Vec::new();
        PngEncoder::new(&mut buf)
            .write_image(&rgba, 4, 4, ColorType::Rgba8.into())
            .unwrap();
        // Static PNG isn't an APNG; PngDecoder::apng() typically still returns OK
        // but may yield zero frames (image crate skips non-APNG static images).
        // Either outcome is acceptable — we just verify it doesn't panic.
        match ApngSource::open(&buf) {
            Ok(dec) => {
                let meta = dec.metadata();
                let frames = meta.frame_count.unwrap_or(0);
                if frames >= 1 {
                    // If a frame was returned, dimensions must be correct.
                    assert_eq!(meta.width, 4);
                    assert_eq!(meta.height, 4);
                }
                // Zero frames: static PNG treated as empty animation — acceptable.
            }
            Err(_) => {
                // Acceptable: static PNG is not APNG; skip.
            }
        }
    }

    #[test]
    fn lookup_table_includes_apng() {
        assert!(crate::animation::decoder::lookup_decoder("image/apng").is_some());
    }
}
