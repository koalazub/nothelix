use crate::animation::decoder::{
    AnimatedDecoder, AnimationMetadata, DecodedFrame, DecoderEntry, DecoderError, frame_at_in,
    hash_bytes,
};
use std::sync::Arc;
use std::time::Duration;

pub struct ApngSource {
    frames: Vec<DecodedFrame>,
    metadata: AnimationMetadata,
}

impl ApngSource {
    pub fn open(bytes: &[u8]) -> Result<Box<dyn AnimatedDecoder>, DecoderError> {
        use ::image::AnimationDecoder;
        use ::image::ImageDecoder;
        use ::image::codecs::png::PngDecoder;

        // First pass: try the animated path. PNGs that are not APNGs surface
        // through `into_frames()` with zero frames; for those we fall back to
        // a static single-frame decode so callers can render the still image.
        let dec_anim = PngDecoder::new(std::io::Cursor::new(bytes))?;
        let dims = dec_anim.dimensions();
        let apng = dec_anim.apng()?;
        let frames_iter = apng.into_frames();

        let mut frames = Vec::new();
        let mut acc = Duration::ZERO;
        let (mut width, mut height) =
            crate::animation::decoder::fit_dimensions_to_u16(dims.0, dims.1)?;
        for (idx, f) in frames_iter.enumerate() {
            let f = f?;
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

        if frames.is_empty() {
            let dec_static = PngDecoder::new(std::io::Cursor::new(bytes))?;
            let (w, h) = dec_static.dimensions();
            // Read into Rgba8 — PNGs may be Rgb8 or Indexed; convert via image's DynamicImage.
            let dyn_img = ::image::DynamicImage::from_decoder(dec_static)?;
            let rgba8 = dyn_img.to_rgba8();
            let raw = rgba8.into_raw();
            let rgba: Arc<[u8]> = Arc::from(raw.as_slice());
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
        Ok(Box::new(ApngSource {
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
    DecoderEntry { mime: "image/apng", factory: |b| ApngSource::open(b) }
}

impl AnimatedDecoder for ApngSource {
    fn metadata(&self) -> AnimationMetadata {
        self.metadata.clone()
    }

    fn frame_at(&mut self, elapsed: Duration) -> Result<Option<DecodedFrame>, DecoderError> {
        Ok(frame_at_in(
            &self.frames,
            self.metadata.total_duration,
            elapsed,
        ))
    }

    fn seek(&mut self, _elapsed: Duration) -> Result<(), DecoderError> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a deterministic 3-frame 4x4 animated APNG using the low-level
    /// `png` crate's animation API. Frames cycle through red → green → blue
    /// at 100ms each, looping infinitely.
    fn build_tiny_apng() -> Vec<u8> {
        use png::{BitDepth, ColorType, Encoder};
        let palette = [
            [255u8, 0, 0, 255], // red
            [0, 255, 0, 255],   // green
            [0, 0, 255, 255],   // blue
        ];
        let mut buf = Vec::new();
        {
            let mut enc = Encoder::new(&mut buf, 4, 4);
            enc.set_color(ColorType::Rgba);
            enc.set_depth(BitDepth::Eight);
            enc.set_animated(palette.len() as u32, 0).unwrap(); // 0 = infinite loop
            enc.set_frame_delay(100, 1000).unwrap(); // 100/1000 s = 100 ms
            let mut writer = enc.write_header().unwrap();
            for color in &palette {
                let frame: Vec<u8> = color.iter().copied().cycle().take(4 * 4 * 4).collect();
                writer.write_image_data(&frame).unwrap();
            }
            writer.finish().unwrap();
        }
        buf
    }

    #[test]
    fn metadata_reports_three_frames() {
        let bytes = build_tiny_apng();
        let dec = ApngSource::open(&bytes).expect("decode tiny apng");
        let meta = dec.metadata();
        assert_eq!(meta.frame_count, Some(3));
        assert_eq!(meta.width, 4);
        assert_eq!(meta.height, 4);
    }

    #[test]
    fn frame_at_timing_picks_correct_frame() {
        let bytes = build_tiny_apng();
        let mut dec = ApngSource::open(&bytes).unwrap();
        // Frames at 0..100, 100..200, 200..300 → query midpoints
        let f0 = dec.frame_at(Duration::from_millis(50)).unwrap().unwrap();
        let f1 = dec.frame_at(Duration::from_millis(150)).unwrap().unwrap();
        let f2 = dec.frame_at(Duration::from_millis(250)).unwrap().unwrap();
        assert_eq!(f0.frame_index, 0);
        assert_eq!(f1.frame_index, 1);
        assert_eq!(f2.frame_index, 2);
    }

    #[test]
    fn frame_at_loops_modulo_total() {
        let bytes = build_tiny_apng();
        let mut dec = ApngSource::open(&bytes).unwrap();
        // Total = 300 ms; 350 ms wraps to 50 ms → frame 0
        let f = dec.frame_at(Duration::from_millis(350)).unwrap().unwrap();
        assert_eq!(f.frame_index, 0);
    }

    #[test]
    fn distinct_content_ids_per_color() {
        let bytes = build_tiny_apng();
        let mut dec = ApngSource::open(&bytes).unwrap();
        let mut ids = std::collections::HashSet::new();
        for ms in [0, 100, 200] {
            ids.insert(
                dec.frame_at(Duration::from_millis(ms))
                    .unwrap()
                    .unwrap()
                    .content_id,
            );
        }
        assert_eq!(ids.len(), 3);
    }
}
