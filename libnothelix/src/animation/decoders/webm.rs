//! WebM (VP8/VP9/AV1) decoder. Feature-gated behind `video`.
//!
//! Pure-Rust AV1 decode is available via dav1d-rs (libdav1d binding) or
//! rav1d. VP9 needs vpx or wasm-builds. This module currently registers
//! the MIME and returns `UnsupportedCodec` so callers fall through to the
//! plugin's static-image fallback. Replace the body of `WebmSource::open`
//! when the native deps are wired in.

use crate::animation::decoder::*;

pub struct WebmSource;

impl WebmSource {
    pub fn open(_bytes: &[u8]) -> Result<Box<dyn AnimatedDecoder>, DecoderError> {
        Err(DecoderError::UnsupportedCodec(
            "WebM (VP8/VP9/AV1) decoding requires libnothelix to be built \
             with a working video decoder dependency. Open \
             libnothelix/src/animation/decoders/webm.rs and replace this \
             stub. The plugin's static-image fallback handles rendering \
             until then."
                .to_string(),
        ))
    }
}

inventory::submit! {
    DecoderEntry { mime: "video/webm", factory: |b| WebmSource::open(b) }
}
