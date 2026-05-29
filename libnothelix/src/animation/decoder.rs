use std::sync::Arc;
use std::time::Duration;

#[derive(Debug, Clone)]
pub struct AnimationMetadata {
    pub width: u16,
    pub height: u16,
    pub frame_count: Option<u64>,
    pub native_fps: f32,
    pub total_duration: Option<Duration>,
    pub loops_natively: bool,
}

#[derive(Debug, Clone)]
pub struct DecodedFrame {
    pub rgba: Arc<[u8]>,
    pub width: u16,
    pub height: u16,
    pub frame_index: u64,
    pub presentation_offset: Duration,
    pub content_id: u64,
}

pub trait AnimatedDecoder: Send {
    fn metadata(&self) -> AnimationMetadata;
    fn frame_at(&mut self, elapsed: Duration) -> Result<Option<DecodedFrame>, DecoderError>;
    fn seek(&mut self, elapsed: Duration) -> Result<(), DecoderError>;
}

#[derive(Debug, thiserror::Error)]
pub enum DecoderError {
    #[error("malformed: {0}")]
    Malformed(String),
    #[error("unsupported codec: {0}")]
    UnsupportedCodec(String),
    #[error("io: {0}")]
    Io(String),
}

pub type DecoderFactory = fn(&[u8]) -> Result<Box<dyn AnimatedDecoder>, DecoderError>;

/// Narrow image dimensions returned by upstream decoders (`u32` each)
/// to the terminal-cell-counted (`u16`, `u16`) form the renderer takes.
/// An image with either dimension ≥ 65 536 px is rejected as malformed
/// — no terminal can usefully render that, and silently truncating
/// would leave decoded RGBA buffers misaligned with the reported
/// (width, height) pair.
pub fn fit_dimensions_to_u16(width: u32, height: u32) -> Result<(u16, u16), DecoderError> {
    let w = u16::try_from(width).map_err(|_| {
        DecoderError::Malformed(format!("image width {width} exceeds u16 limit"))
    })?;
    let h = u16::try_from(height).map_err(|_| {
        DecoderError::Malformed(format!("image height {height} exceeds u16 limit"))
    })?;
    Ok((w, h))
}

pub struct DecoderEntry {
    pub mime: &'static str,
    pub factory: DecoderFactory,
}

inventory::collect!(DecoderEntry);

pub fn lookup_decoder(mime: &str) -> Option<DecoderFactory> {
    inventory::iter::<DecoderEntry>
        .into_iter()
        .find(|e| e.mime == mime)
        .map(|e| e.factory)
}
