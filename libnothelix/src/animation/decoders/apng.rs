use crate::animation::decoder::{
    AnimatedDecoder, DecoderEntry, DecoderError, fit_dimensions_to_u16,
};
use crate::animation::decoders::frames::{FrameSequence, Looping};

pub fn open(bytes: &[u8]) -> Result<Box<dyn AnimatedDecoder>, DecoderError> {
    use ::image::codecs::png::PngDecoder;
    use ::image::{AnimationDecoder, ImageDecoder};

    let animated = PngDecoder::new(std::io::Cursor::new(bytes))?;
    let (width, height) = fit_dimensions_to_u16(animated.dimensions())?;
    let mut sequence = FrameSequence::new(width, height);
    sequence.absorb(animated.apng()?.into_frames())?;

    if sequence.is_empty() {
        sequence.push_still_from(PngDecoder::new(std::io::Cursor::new(bytes))?)?;
    }

    Ok(sequence.into_source(Looping::WhenMultiFrame))
}

inventory::submit! {
    DecoderEntry { mime: "image/apng", factory: open }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::animation::decoders::frames::frame_at_ms;

    fn tiny_apng() -> Box<dyn AnimatedDecoder> {
        open(&build_tiny_apng()).expect("decode tiny apng")
    }

    fn build_tiny_apng() -> Vec<u8> {
        use png::{BitDepth, ColorType, Encoder};
        let palette = [[255u8, 0, 0, 255], [0, 255, 0, 255], [0, 0, 255, 255]];
        let mut buf = Vec::new();
        {
            let mut enc = Encoder::new(&mut buf, 4, 4);
            enc.set_color(ColorType::Rgba);
            enc.set_depth(BitDepth::Eight);
            enc.set_animated(palette.len() as u32, 0).unwrap();
            enc.set_frame_delay(100, 1000).unwrap();
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
        let meta = tiny_apng().metadata();
        assert_eq!(meta.frame_count, 3);
        assert_eq!(meta.width, 4);
        assert_eq!(meta.height, 4);
    }

    #[test]
    fn frame_at_timing_picks_correct_frame() {
        let mut decoder = tiny_apng();
        assert_eq!(frame_at_ms(decoder.as_mut(), 50).frame_index, 0);
        assert_eq!(frame_at_ms(decoder.as_mut(), 150).frame_index, 1);
        assert_eq!(frame_at_ms(decoder.as_mut(), 250).frame_index, 2);
    }

    #[test]
    fn frame_at_loops_modulo_total() {
        assert_eq!(frame_at_ms(tiny_apng().as_mut(), 350).frame_index, 0);
    }

    #[test]
    fn distinct_content_ids_per_color() {
        let mut decoder = tiny_apng();
        let ids: std::collections::HashSet<u64> = [0, 100, 200]
            .into_iter()
            .map(|ms| frame_at_ms(decoder.as_mut(), ms).content_id)
            .collect();
        assert_eq!(ids.len(), 3);
    }
}
