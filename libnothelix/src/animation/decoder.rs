use std::sync::Arc;
use std::time::Duration;

const FALLBACK_FPS: f32 = 30.0;
const MIN_TICK_INTERVAL_MS: u32 = 8;

#[derive(Debug, Clone)]
pub struct AnimationMetadata {
    pub width: u16,
    pub height: u16,
    pub frame_count: u64,
    pub native_fps: f32,
    pub total_duration: Duration,
}

impl AnimationMetadata {
    pub fn tick_interval_ms(&self) -> u32 {
        let fps = if self.native_fps > 0.0 {
            self.native_fps
        } else {
            FALLBACK_FPS
        };
        ((1000.0 / fps).round() as u32).max(MIN_TICK_INTERVAL_MS)
    }
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
}

#[derive(Debug, thiserror::Error)]
pub enum DecoderError {
    #[error("malformed: {0}")]
    Malformed(String),
    #[error("unsupported codec: {0}")]
    UnsupportedCodec(String),
}

impl From<::image::ImageError> for DecoderError {
    fn from(error: ::image::ImageError) -> Self {
        DecoderError::Malformed(error.to_string())
    }
}

pub type DecoderFactory = fn(&[u8]) -> Result<Box<dyn AnimatedDecoder>, DecoderError>;

pub fn fit_dimensions_to_u16((width, height): (u32, u32)) -> Result<(u16, u16), DecoderError> {
    let w = u16::try_from(width)
        .map_err(|_| DecoderError::Malformed(format!("image width {width} exceeds u16 limit")))?;
    let h = u16::try_from(height)
        .map_err(|_| DecoderError::Malformed(format!("image height {height} exceeds u16 limit")))?;
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
        .find(|entry| entry.mime == mime)
        .map(|entry| entry.factory)
}
