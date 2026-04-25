//! MP4 (H.264) decoder. Feature-gated behind `video`.
//!
//! Decoding H.264 in pure Rust is unsettled; openh264-rs and dav1d-rs are
//! the practical paths but both pull in native dependencies that may not
//! link on every host. This module currently registers the MIME and
//! returns an `UnsupportedCodec` error so callers fall through to the
//! static-image fallback registered by the plugin. To wire actual decode,
//! add an `openh264` (or equivalent) dependency and replace the body of
//! `Mp4Source::open`. The trait and registration surfaces stay the same.

use crate::animation::decoder::*;
use std::sync::Arc;
use std::time::Duration;

pub struct Mp4Source;

impl Mp4Source {
    pub fn open(_bytes: &[u8]) -> Result<Box<dyn AnimatedDecoder>, DecoderError> {
        Err(DecoderError::UnsupportedCodec(
            "MP4 (H.264) decoding requires libnothelix to be built with a \
             working H.264 decoder dependency. Open libnothelix/src/animation/\
             decoders/mp4.rs and replace this stub with an openh264 / \
             ffmpeg-next implementation. The plugin's static-image fallback \
             will render the first frame of the bundle in the meantime."
                .to_string(),
        ))
    }
}

inventory::submit! {
    DecoderEntry { mime: "video/mp4", factory: |b| Mp4Source::open(b) }
}

// Reference impl scaffold so future contributors don't have to rebuild
// from scratch. Compiles when the contributor wires in the native dep.
#[allow(dead_code)]
fn _unused_frame_shape_reference() -> DecodedFrame {
    DecodedFrame {
        rgba: Arc::from([].as_slice()),
        width: 0,
        height: 0,
        frame_index: 0,
        presentation_offset: Duration::ZERO,
        content_id: 0,
    }
}
