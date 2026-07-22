use crate::animation::decoder::DecodedFrame;
#[cfg(test)]
use crate::animation::renderer::select_renderer;
use crate::animation::renderer::{AnimationRenderer, RendererEntry, TerminalCaps};
use crate::animation::renderers::png;
use crate::error::Result;

pub struct StaticFallbackRenderer {
    last_id: Option<u64>,
}

impl StaticFallbackRenderer {
    pub fn boxed() -> Box<dyn AnimationRenderer> {
        Box::new(StaticFallbackRenderer { last_id: None })
    }

    pub fn try_new(_caps: &TerminalCaps) -> Option<Box<dyn AnimationRenderer>> {
        Some(Self::boxed())
    }
}

inventory::submit! {
    RendererEntry { priority: 1000, factory: StaticFallbackRenderer::try_new }
}

impl AnimationRenderer for StaticFallbackRenderer {
    fn name(&self) -> &'static str {
        "static-fallback"
    }

    fn transmit_frame(&mut self, frame: &DecodedFrame, _engine_id: u64) -> Result<Vec<u8>> {
        if Some(frame.content_id) == self.last_id {
            return Ok(Vec::new());
        }
        let png = png::encode_rgba(frame.rgba.as_ref(), frame.width, frame.height)?;
        self.last_id = Some(frame.content_id);
        Ok(png)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::time::Duration;

    fn make_frame(content_id: u64) -> DecodedFrame {
        DecodedFrame {
            rgba: Arc::from(vec![0u8; 4 * 32 * 32].as_slice()),
            width: 32,
            height: 32,
            frame_index: 0,
            presentation_offset: Duration::ZERO,
            content_id,
        }
    }

    #[test]
    fn static_fallback_emits_png_first_call() {
        let mut renderer = StaticFallbackRenderer::try_new(&TerminalCaps::default()).unwrap();
        let bytes = renderer.transmit_frame(&make_frame(7), 1).unwrap();
        assert!(!bytes.is_empty());
        assert_eq!(&bytes[0..4], b"\x89PNG");
    }

    #[test]
    fn static_fallback_skips_same_content() {
        let mut renderer = StaticFallbackRenderer::try_new(&TerminalCaps::default()).unwrap();
        let _ = renderer.transmit_frame(&make_frame(7), 1).unwrap();
        let bytes = renderer.transmit_frame(&make_frame(7), 1).unwrap();
        assert!(bytes.is_empty());
    }

    #[test]
    fn select_renderer_returns_static_when_no_kitty() {
        let renderer = select_renderer(&TerminalCaps::default());
        assert_eq!(renderer.name(), "static-fallback");
    }
}
