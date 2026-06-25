use crate::animation::decoder::DecodedFrame;
#[cfg(test)]
use crate::animation::renderer::select_renderer;
use crate::animation::renderer::{
    AnimationRenderer, RenderContext, RendererCapabilities, RendererEntry, TerminalCaps,
};
use crate::animation::renderers::encode_rgba_to_png;

pub struct StaticFallbackRenderer {
    last_id: Option<u64>,
}

impl StaticFallbackRenderer {
    pub fn try_new(_caps: &TerminalCaps) -> Option<Box<dyn AnimationRenderer>> {
        Some(Box::new(StaticFallbackRenderer { last_id: None }))
    }
}

inventory::submit! {
    RendererEntry { priority: 1000, factory: StaticFallbackRenderer::try_new }
}

impl AnimationRenderer for StaticFallbackRenderer {
    fn name(&self) -> &'static str {
        "static-fallback"
    }

    fn capabilities(&self) -> RendererCapabilities {
        RendererCapabilities {
            supports_native_animation: false,
            supports_diff_frames: false,
            max_dimensions: None,
        }
    }

    fn transmit_frame(&mut self, frame: &DecodedFrame, _ctx: &RenderContext) -> Vec<u8> {
        if Some(frame.content_id) == self.last_id {
            return Vec::new();
        }
        self.last_id = Some(frame.content_id);
        encode_rgba_to_png(frame.rgba.as_ref(), frame.width, frame.height)
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
        let caps = TerminalCaps::default();
        let mut r = StaticFallbackRenderer::try_new(&caps).unwrap();
        let bytes = r.transmit_frame(
            &make_frame(7),
            &RenderContext {
                engine_id: 1,
                cell_position: (0, 0),
                previous_content_id: None,
            },
        );
        assert!(!bytes.is_empty());
        assert_eq!(&bytes[0..4], b"\x89PNG");
    }

    #[test]
    fn static_fallback_skips_same_content() {
        let caps = TerminalCaps::default();
        let mut r = StaticFallbackRenderer::try_new(&caps).unwrap();
        let _ = r.transmit_frame(
            &make_frame(7),
            &RenderContext {
                engine_id: 1,
                cell_position: (0, 0),
                previous_content_id: None,
            },
        );
        let bytes = r.transmit_frame(
            &make_frame(7),
            &RenderContext {
                engine_id: 1,
                cell_position: (0, 0),
                previous_content_id: Some(7),
            },
        );
        assert!(bytes.is_empty());
    }

    #[test]
    fn select_renderer_returns_static_when_no_kitty() {
        let caps = TerminalCaps::default();
        let r = select_renderer(&caps);
        assert_eq!(r.name(), "static-fallback");
    }
}
