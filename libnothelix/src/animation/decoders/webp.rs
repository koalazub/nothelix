use crate::animation::decoder::{
    AnimatedDecoder, DecoderEntry, DecoderError, fit_dimensions_to_u16,
};
use crate::animation::decoders::frames::{FrameSequence, Looping};

pub fn open(bytes: &[u8]) -> Result<Box<dyn AnimatedDecoder>, DecoderError> {
    use ::image::codecs::webp::WebPDecoder;
    use ::image::{AnimationDecoder, ImageDecoder};

    let animated = WebPDecoder::new(std::io::BufReader::new(std::io::Cursor::new(bytes)))?;
    let (width, height) = fit_dimensions_to_u16(animated.dimensions())?;
    let mut sequence = FrameSequence::new(width, height);
    sequence.absorb(animated.into_frames())?;

    if sequence.is_empty() {
        let still = WebPDecoder::new(std::io::BufReader::new(std::io::Cursor::new(bytes)))?;
        sequence.push_still_from(still)?;
    }

    Ok(sequence.into_source(Looping::WhenMultiFrame))
}

inventory::submit! {
    DecoderEntry { mime: "image/webp", factory: open }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::animation::decoders::frames::frame_at_ms;

    fn static_webp() -> Box<dyn AnimatedDecoder> {
        open(&build_static_webp()).expect("decode static webp")
    }

    fn build_static_webp() -> Vec<u8> {
        use ::image::codecs::webp::WebPEncoder;
        use ::image::{ColorType, ImageEncoder};
        let rgba = [255u8, 0, 0, 255].repeat(16);
        let mut buf = Vec::new();
        WebPEncoder::new_lossless(&mut buf)
            .write_image(&rgba, 4, 4, ColorType::Rgba8.into())
            .unwrap();
        buf
    }

    #[test]
    fn decoder_yields_one_frame_with_correct_dimensions() {
        let meta = static_webp().metadata();
        assert_eq!(meta.frame_count, 1);
        assert_eq!(meta.width, 4);
        assert_eq!(meta.height, 4);
    }

    #[test]
    fn frame_at_zero_returns_index_zero() {
        let frame = frame_at_ms(static_webp().as_mut(), 0);
        assert_eq!(frame.frame_index, 0);
        assert_eq!(frame.width, 4);
        assert_eq!(frame.height, 4);
    }

    #[test]
    fn content_id_stable_across_calls() {
        let mut decoder = static_webp();
        let first = frame_at_ms(decoder.as_mut(), 0).content_id;
        let later = frame_at_ms(decoder.as_mut(), 50).content_id;
        assert_eq!(first, later);
    }

    #[test]
    fn malformed_bytes_return_error() {
        assert!(open(b"not a webp at all").is_err());
    }
}
