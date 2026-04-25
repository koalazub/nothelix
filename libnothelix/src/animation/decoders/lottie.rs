//! Lottie (application/json+lottie) decoder. Feature-gated behind `lottie`.
//!
//! Lottie is JSON-defined vector animation; rasterisation requires a real
//! renderer. `rlottie` (Rust binding to libsamsung lottie) is the canonical
//! C++ option but pulls in heavy native deps. Pure-Rust paths exist
//! (lottie-rs in early state) but coverage is partial. This stub returns
//! UnsupportedCodec so the plugin falls through to static rendering.

use crate::animation::decoder::*;

pub struct LottieSource;

impl LottieSource {
    pub fn open(_bytes: &[u8]) -> Result<Box<dyn AnimatedDecoder>, DecoderError> {
        Err(DecoderError::UnsupportedCodec(
            "Lottie decoding requires libnothelix to be built with a Lottie \
             rasteriser. Wire rlottie or lottie-rs into \
             libnothelix/src/animation/decoders/lottie.rs to enable. The \
             plugin's static fallback will render the first frame in the \
             meantime."
                .to_string(),
        ))
    }
}

inventory::submit! {
    DecoderEntry { mime: "application/json+lottie", factory: |b| LottieSource::open(b) }
}
