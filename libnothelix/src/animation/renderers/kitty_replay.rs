use crate::animation::decoder::DecodedFrame;
use crate::animation::renderer::{AnimationRenderer, RendererEntry, TerminalCaps};
use crate::animation::renderers::png;
use crate::error::{Error, FFI_ERROR_PREFIX, RenderStage, Result};
use crate::kitty_placeholder::kitty_placeholder_payload_bytes;

pub struct KittyReplayRenderer {
    last_id: Option<u64>,
}

impl KittyReplayRenderer {
    pub fn try_new(caps: &TerminalCaps) -> Option<Box<dyn AnimationRenderer>> {
        caps.kitty_graphics
            .then(|| Box::new(KittyReplayRenderer { last_id: None }) as Box<dyn AnimationRenderer>)
    }
}

inventory::submit! {
    RendererEntry { priority: 100, factory: KittyReplayRenderer::try_new }
}

impl AnimationRenderer for KittyReplayRenderer {
    fn name(&self) -> &'static str {
        "kitty-replay"
    }

    fn transmit_frame(&mut self, frame: &DecodedFrame, engine_id: u64) -> Result<Vec<u8>> {
        if Some(frame.content_id) == self.last_id {
            return Ok(Vec::new());
        }
        let png = png::encode_rgba(frame.rgba.as_ref(), frame.width, frame.height)?;
        let payload = kitty_placeholder_payload_bytes(png.into(), engine_id as isize);
        if let Some(detail) = payload.strip_prefix(FFI_ERROR_PREFIX) {
            return Err(Error::Render {
                stage: RenderStage::Rasterize,
                subject: format!("animation engine {engine_id} frame {}", frame.frame_index),
                detail: detail.to_string(),
            });
        }
        self.last_id = Some(frame.content_id);
        Ok(payload.into_bytes())
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

    fn kitty_caps() -> TerminalCaps {
        TerminalCaps {
            kitty_graphics: true,
            kitty_animation_protocol: false,
        }
    }

    #[test]
    fn try_new_returns_none_without_kitty() {
        assert!(KittyReplayRenderer::try_new(&TerminalCaps::default()).is_none());
    }

    #[test]
    fn try_new_returns_some_with_kitty() {
        assert!(KittyReplayRenderer::try_new(&kitty_caps()).is_some());
    }

    #[test]
    fn transmit_skips_same_content() {
        let mut renderer = KittyReplayRenderer::try_new(&kitty_caps()).unwrap();
        let _ = renderer.transmit_frame(&make_frame(1), 1).unwrap();
        let bytes = renderer.transmit_frame(&make_frame(1), 1).unwrap();
        assert!(bytes.is_empty());
    }

    #[test]
    fn transmit_emits_apc_kitty_escape() {
        let mut renderer = KittyReplayRenderer::try_new(&kitty_caps()).unwrap();
        let bytes = renderer.transmit_frame(&make_frame(2), 7).unwrap();
        assert!(!bytes.is_empty());
        assert!(bytes.starts_with(b"\x1b_G"));
    }
}
