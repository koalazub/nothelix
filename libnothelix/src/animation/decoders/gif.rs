use crate::animation::decoder::*;
use std::sync::Arc;
use std::time::Duration;

pub struct GifSource {
    frames: Vec<DecodedFrame>,
    metadata: AnimationMetadata,
}

impl GifSource {
    pub fn open(bytes: &[u8]) -> Result<Box<dyn AnimatedDecoder>, DecoderError> {
        use ::image::{AnimationDecoder, codecs::gif::GifDecoder};
        let dec = GifDecoder::new(std::io::Cursor::new(bytes))
            .map_err(|e| DecoderError::Malformed(e.to_string()))?;
        let frames_iter = dec.into_frames();
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
        Ok(Box::new(GifSource {
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
    DecoderEntry { mime: "image/gif", factory: |b| GifSource::open(b) }
}

impl AnimatedDecoder for GifSource {
    fn metadata(&self) -> AnimationMetadata { self.metadata.clone() }

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

    fn seek(&mut self, _elapsed: Duration) -> Result<(), DecoderError> { Ok(()) }
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
    use crate::animation::decoders::gif_fixture::tiny_gif_bytes;

    #[test]
    fn metadata_reports_four_frames() {
        let bytes = tiny_gif_bytes();
        let dec = GifSource::open(&bytes).expect("decode tiny gif");
        let meta = dec.metadata();
        assert_eq!(meta.frame_count, Some(4));
        assert_eq!(meta.width, 32);
        assert_eq!(meta.height, 32);
    }

    #[test]
    fn frame_at_returns_index_one_at_150ms() {
        let bytes = tiny_gif_bytes();
        let mut dec = GifSource::open(&bytes).unwrap();
        let f = dec.frame_at(Duration::from_millis(150)).unwrap().unwrap();
        assert_eq!(f.frame_index, 1);
    }

    #[test]
    fn frame_at_loops_after_total_duration() {
        let bytes = tiny_gif_bytes();
        let mut dec = GifSource::open(&bytes).unwrap();
        let f = dec.frame_at(Duration::from_millis(450)).unwrap().unwrap();
        // 450 % 400 = 50 -> frame 0
        assert_eq!(f.frame_index, 0);
    }

    #[test]
    fn content_ids_are_distinct_per_frame() {
        let bytes = tiny_gif_bytes();
        let mut dec = GifSource::open(&bytes).unwrap();
        let mut ids = std::collections::HashSet::new();
        for ms in [0, 100, 200, 300] {
            let f = dec.frame_at(Duration::from_millis(ms)).unwrap().unwrap();
            ids.insert(f.content_id);
        }
        assert_eq!(ids.len(), 4);
    }

    #[test]
    fn malformed_bytes_return_error() {
        assert!(GifSource::open(b"not a gif").is_err());
    }
}
