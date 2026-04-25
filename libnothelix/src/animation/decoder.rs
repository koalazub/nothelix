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
