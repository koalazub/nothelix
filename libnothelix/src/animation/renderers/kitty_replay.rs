use crate::animation::decoder::DecodedFrame;
use crate::animation::renderer::{
    AnimationRenderer, RenderContext, RendererCapabilities, RendererEntry, TerminalCaps,
};
use crate::animation::renderers::encode_rgba_to_png;

pub struct KittyReplayRenderer {
    last_id: Option<u64>,
}

impl KittyReplayRenderer {
    pub fn try_new(caps: &TerminalCaps) -> Option<Box<dyn AnimationRenderer>> {
        if caps.kitty_graphics {
            Some(Box::new(KittyReplayRenderer { last_id: None }))
        } else {
            None
        }
    }
}

inventory::submit! {
    RendererEntry { priority: 100, factory: KittyReplayRenderer::try_new }
}

impl AnimationRenderer for KittyReplayRenderer {
    fn name(&self) -> &'static str {
        "kitty-replay"
    }

    fn capabilities(&self) -> RendererCapabilities {
        RendererCapabilities {
            supports_native_animation: false,
            supports_diff_frames: false,
            max_dimensions: None,
        }
    }

    fn transmit_frame(&mut self, frame: &DecodedFrame, ctx: &RenderContext) -> Vec<u8> {
        if Some(frame.content_id) == self.last_id {
            return Vec::new();
        }
        self.last_id = Some(frame.content_id);
        let png = encode_rgba_to_png(frame.rgba.as_ref(), frame.width, frame.height);
        let escape = crate::kitty_placeholder::kitty_placeholder_payload_bytes(
            png.into(),
            ctx.engine_id as isize,
        );
        escape.into_bytes()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::time::Duration;

    fn make_frame(id: u64) -> DecodedFrame {
        DecodedFrame {
            rgba: Arc::from(vec![255u8; 4 * 16 * 16].as_slice()),
            width: 16,
            height: 16,
            frame_index: id,
            presentation_offset: Duration::ZERO,
            content_id: id,
        }
    }

    #[test]
    fn try_new_returns_none_without_kitty() {
        assert!(KittyReplayRenderer::try_new(&TerminalCaps::default()).is_none());
    }

    #[test]
    fn try_new_returns_some_with_kitty() {
        let caps = TerminalCaps {
            kitty_graphics: true,
            ..Default::default()
        };
        assert!(KittyReplayRenderer::try_new(&caps).is_some());
    }

    #[test]
    fn transmit_skips_same_content() {
        let caps = TerminalCaps {
            kitty_graphics: true,
            ..Default::default()
        };
        let mut r = KittyReplayRenderer::try_new(&caps).unwrap();
        let _ = r.transmit_frame(
            &make_frame(1),
            &RenderContext {
                engine_id: 1,
                cell_position: (0, 0),
                previous_content_id: None,
            },
        );
        let bytes = r.transmit_frame(
            &make_frame(1),
            &RenderContext {
                engine_id: 1,
                cell_position: (0, 0),
                previous_content_id: Some(1),
            },
        );
        assert!(bytes.is_empty());
    }

    #[test]
    fn transmit_emits_apc_kitty_escape() {
        let caps = TerminalCaps {
            kitty_graphics: true,
            ..Default::default()
        };
        let mut r = KittyReplayRenderer::try_new(&caps).unwrap();
        let bytes = r.transmit_frame(
            &make_frame(2),
            &RenderContext {
                engine_id: 7,
                cell_position: (0, 0),
                previous_content_id: None,
            },
        );
        // Kitty APC begins with ESC _G
        assert!(!bytes.is_empty());
        assert!(bytes.starts_with(b"\x1b_G"));
    }
}
