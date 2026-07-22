use crate::animation::decoder::{AnimatedDecoder, DecoderEntry, DecoderError};
use crate::animation::decoders::frames::{FrameSequence, Looping};

pub fn open(bytes: &[u8]) -> Result<Box<dyn AnimatedDecoder>, DecoderError> {
    use ::image::{AnimationDecoder, codecs::gif::GifDecoder};
    let decoder = GifDecoder::new(std::io::Cursor::new(bytes))?;
    let mut sequence = FrameSequence::new(0, 0);
    sequence.absorb(decoder.into_frames())?;
    Ok(sequence.into_source(Looping::Always))
}

inventory::submit! {
    DecoderEntry { mime: "image/gif", factory: open }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::animation::decoders::frames::frame_at_ms;
    use crate::animation::decoders::gif_fixture::tiny_gif_bytes;

    fn tiny_gif() -> Box<dyn AnimatedDecoder> {
        open(&tiny_gif_bytes()).expect("decode tiny gif")
    }

    #[test]
    fn metadata_reports_four_frames() {
        let meta = tiny_gif().metadata();
        assert_eq!(meta.frame_count, 4);
        assert_eq!(meta.width, 32);
        assert_eq!(meta.height, 32);
    }

    #[test]
    fn frame_at_returns_index_one_at_150ms() {
        assert_eq!(frame_at_ms(tiny_gif().as_mut(), 150).frame_index, 1);
    }

    #[test]
    fn frame_at_loops_after_total_duration() {
        assert_eq!(frame_at_ms(tiny_gif().as_mut(), 450).frame_index, 0);
    }

    #[test]
    fn content_ids_are_distinct_per_frame() {
        let mut decoder = tiny_gif();
        let ids: std::collections::HashSet<u64> = [0, 100, 200, 300]
            .into_iter()
            .map(|ms| frame_at_ms(decoder.as_mut(), ms).content_id)
            .collect();
        assert_eq!(ids.len(), 4);
    }

    #[test]
    fn malformed_bytes_return_error() {
        assert!(open(b"not a gif").is_err());
    }
}
