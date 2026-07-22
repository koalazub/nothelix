use crate::animation::decoder::{AnimatedDecoder, DecoderEntry, DecoderError};

struct MissingCodec {
    container: &'static str,
    needs: &'static str,
}

impl MissingCodec {
    fn reject(&self) -> Result<Box<dyn AnimatedDecoder>, DecoderError> {
        let Self { container, needs } = self;
        Err(DecoderError::UnsupportedCodec(format!(
            "{container} decoding needs libnothelix built with {needs}; wire one into \
             libnothelix/src/animation/decoders/unsupported.rs to enable it. Until then the \
             plugin's static-image fallback renders the first frame of the bundle."
        )))
    }
}

#[cfg(feature = "video")]
pub fn open_mp4(_bytes: &[u8]) -> Result<Box<dyn AnimatedDecoder>, DecoderError> {
    MissingCodec {
        container: "MP4 (H.264)",
        needs: "an openh264 or ffmpeg-next dependency",
    }
    .reject()
}

#[cfg(feature = "video")]
pub fn open_webm(_bytes: &[u8]) -> Result<Box<dyn AnimatedDecoder>, DecoderError> {
    MissingCodec {
        container: "WebM (VP8/VP9/AV1)",
        needs: "a dav1d-rs, rav1d or vpx dependency",
    }
    .reject()
}

#[cfg(feature = "lottie")]
pub fn open_lottie(_bytes: &[u8]) -> Result<Box<dyn AnimatedDecoder>, DecoderError> {
    MissingCodec {
        container: "Lottie",
        needs: "an rlottie or lottie-rs rasteriser",
    }
    .reject()
}

#[cfg(feature = "video")]
inventory::submit! {
    DecoderEntry { mime: "video/mp4", factory: open_mp4 }
}

#[cfg(feature = "video")]
inventory::submit! {
    DecoderEntry { mime: "video/webm", factory: open_webm }
}

#[cfg(feature = "lottie")]
inventory::submit! {
    DecoderEntry { mime: "application/json+lottie", factory: open_lottie }
}
