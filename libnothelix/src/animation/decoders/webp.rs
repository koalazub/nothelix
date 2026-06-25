use crate::animation::decoder::{
    AnimatedDecoder, AnimationMetadata, DecodedFrame, DecoderEntry, DecoderError,
};
use std::sync::Arc;
use std::time::Duration;

pub struct WebPSource {
    frames: Vec<DecodedFrame>,
    metadata: AnimationMetadata,
}

impl WebPSource {
    pub fn open(bytes: &[u8]) -> Result<Box<dyn AnimatedDecoder>, DecoderError> {
        use ::image::AnimationDecoder;
        use ::image::ImageDecoder;
        use ::image::codecs::webp::WebPDecoder;

        // Static WebP files don't surface through `into_frames` (the AnimationDecoder
        // path yields zero frames), so we inspect the file twice: once for animated
        // frames, and if none surface, fall back to a single-frame static decode.
        let cursor1 = std::io::BufReader::new(std::io::Cursor::new(bytes));
        let dec_anim =
            WebPDecoder::new(cursor1).map_err(|e| DecoderError::Malformed(e.to_string()))?;
        let dims = dec_anim.dimensions();
        let frames_iter = dec_anim.into_frames();

        let mut frames = Vec::new();
        let mut acc = Duration::ZERO;
        let (mut width, mut height) =
            crate::animation::decoder::fit_dimensions_to_u16(dims.0, dims.1)?;
        for (idx, f) in frames_iter.enumerate() {
            let f = f.map_err(|e| DecoderError::Malformed(e.to_string()))?;
            let buf = f.buffer();
            (width, height) =
                crate::animation::decoder::fit_dimensions_to_u16(buf.width(), buf.height())?;
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

        // Static WebP fallback: re-open and decode the single full image.
        if frames.is_empty() {
            let cursor2 = std::io::BufReader::new(std::io::Cursor::new(bytes));
            let dec_static =
                WebPDecoder::new(cursor2).map_err(|e| DecoderError::Malformed(e.to_string()))?;
            let (w, h) = dec_static.dimensions();
            let mut buf = vec![0u8; (w as usize) * (h as usize) * 4];
            dec_static
                .read_image(&mut buf)
                .map_err(|e| DecoderError::Malformed(e.to_string()))?;
            let rgba: Arc<[u8]> = Arc::from(buf.as_slice());
            let content_id = hash_bytes(&rgba);
            (width, height) = crate::animation::decoder::fit_dimensions_to_u16(w, h)?;
            frames.push(DecodedFrame {
                rgba,
                width,
                height,
                frame_index: 0,
                presentation_offset: Duration::ZERO,
                content_id,
            });
            acc = Duration::ZERO;
        }

        let frame_count = frames.len() as u64;
        let total = if frame_count <= 1 {
            Duration::ZERO
        } else {
            acc
        };
        let native_fps = if total.as_millis() == 0 {
            0.0
        } else {
            (frame_count as f32 * 1000.0) / total.as_millis() as f32
        };
        Ok(Box::new(WebPSource {
            frames,
            metadata: AnimationMetadata {
                width,
                height,
                frame_count: Some(frame_count),
                native_fps,
                total_duration: Some(total),
                loops_natively: frame_count > 1,
            },
        }))
    }
}

inventory::submit! {
    DecoderEntry { mime: "image/webp", factory: |b| WebPSource::open(b) }
}

impl AnimatedDecoder for WebPSource {
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
            Duration::from_millis((elapsed.as_millis() as u64) % (total.as_millis() as u64).max(1))
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

    fn build_static_webp() -> Vec<u8> {
        use ::image::codecs::webp::WebPEncoder;
        use ::image::{ColorType, ImageEncoder};
        // 4x4 solid red, lossless WebP. WebPEncoder in image 0.25 only supports
        // static WebP — animated WebP encoding is not exposed through the public
        // API. The behavioral guarantees we can verify on static WebP are
        // dimension fidelity, single-frame, and stable content_id.
        let rgba = [255u8, 0, 0, 255].repeat(16);
        let mut buf = Vec::new();
        WebPEncoder::new_lossless(&mut buf)
            .write_image(&rgba, 4, 4, ColorType::Rgba8.into())
            .unwrap();
        buf
    }

    #[test]
    fn decoder_yields_one_frame_with_correct_dimensions() {
        let bytes = build_static_webp();
        let dec = WebPSource::open(&bytes).expect("decode static webp");
        let meta = dec.metadata();
        assert_eq!(meta.frame_count, Some(1));
        assert_eq!(meta.width, 4);
        assert_eq!(meta.height, 4);
    }

    #[test]
    fn frame_at_zero_returns_index_zero() {
        let bytes = build_static_webp();
        let mut dec = WebPSource::open(&bytes).unwrap();
        let f = dec.frame_at(Duration::from_millis(0)).unwrap().unwrap();
        assert_eq!(f.frame_index, 0);
        assert_eq!(f.width, 4);
        assert_eq!(f.height, 4);
    }

    #[test]
    fn content_id_stable_across_calls() {
        let bytes = build_static_webp();
        let mut dec = WebPSource::open(&bytes).unwrap();
        let a = dec
            .frame_at(Duration::from_millis(0))
            .unwrap()
            .unwrap()
            .content_id;
        let b = dec
            .frame_at(Duration::from_millis(50))
            .unwrap()
            .unwrap()
            .content_id;
        assert_eq!(a, b);
    }

    #[test]
    fn malformed_bytes_return_error() {
        let bad = b"not a webp at all";
        assert!(WebPSource::open(bad).is_err());
    }
}
