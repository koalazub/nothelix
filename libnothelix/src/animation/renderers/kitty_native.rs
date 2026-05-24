use crate::animation::decoder::DecodedFrame;
use crate::animation::renderer::*;
use base64::Engine;
use std::collections::HashMap;

pub struct KittyNativeRenderer {
    sent_first_frame: HashMap<u64, bool>,
}

impl KittyNativeRenderer {
    pub fn try_new(caps: &TerminalCaps) -> Option<Box<dyn AnimationRenderer>> {
        if caps.kitty_graphics && caps.kitty_animation_protocol {
            Some(Box::new(KittyNativeRenderer {
                sent_first_frame: HashMap::new(),
            }))
        } else {
            None
        }
    }
}

inventory::submit! {
    RendererEntry { priority: 10, factory: KittyNativeRenderer::try_new }
}

impl AnimationRenderer for KittyNativeRenderer {
    fn name(&self) -> &'static str { "kitty-native" }

    fn capabilities(&self) -> RendererCapabilities {
        RendererCapabilities {
            supports_native_animation: true,
            supports_diff_frames: true,
            max_dimensions: None,
        }
    }

    fn transmit_frame(&mut self, frame: &DecodedFrame, ctx: &RenderContext) -> Vec<u8> {
        let first = !self
            .sent_first_frame
            .get(&ctx.engine_id)
            .copied()
            .unwrap_or(false);
        let png = encode_rgba_to_png(frame.rgba.as_ref(), frame.width, frame.height);
        let b64 = base64::engine::general_purpose::STANDARD.encode(&png);
        let image_id = ctx.engine_id as u32;
        if first {
            self.sent_first_frame.insert(ctx.engine_id, true);
            kitty_full_transmission(image_id, &b64)
        } else {
            kitty_add_frame(image_id, &b64)
        }
    }

    fn teardown(&mut self, engine_id: u64) -> Vec<u8> {
        self.sent_first_frame.remove(&engine_id);
        format!("\x1b_Ga=d,d=I,i={};\x1b\\", engine_id).into_bytes()
    }
}

fn kitty_full_transmission(image_id: u32, b64: &str) -> Vec<u8> {
    let mut out = Vec::new();
    chunked_apc(&mut out, &format!("a=T,f=100,i={},q=2", image_id), b64);
    out
}

fn kitty_add_frame(image_id: u32, b64: &str) -> Vec<u8> {
    let mut out = Vec::new();
    chunked_apc(&mut out, &format!("a=a,i={},r=1,q=2", image_id), b64);
    out
}

/// Emit one or more APC sequences to transmit `payload_b64` in 4096-byte chunks.
/// First chunk carries `key_value`; continuations carry only `m=<more>`.
fn chunked_apc(out: &mut Vec<u8>, key_value: &str, payload_b64: &str) {
    const CHUNK: usize = 4096;
    let bytes = payload_b64.as_bytes();
    let total_chunks = bytes.len().div_ceil(CHUNK).max(1);
    for (i, chunk) in bytes.chunks(CHUNK).enumerate() {
        let last = i + 1 == total_chunks;
        let m = if last { 0 } else { 1 };
        let chunk_str = std::str::from_utf8(chunk).unwrap_or("");
        let prefix = if i == 0 {
            format!("{},m={}", key_value, m)
        } else {
            format!("m={}", m)
        };
        out.extend_from_slice(format!("\x1b_G{};{}\x1b\\", prefix, chunk_str).as_bytes());
    }
}

fn encode_rgba_to_png(rgba: &[u8], w: u16, h: u16) -> Vec<u8> {
    use ::image::codecs::png::PngEncoder;
    use ::image::{ColorType, ImageEncoder};
    let mut buf = Vec::new();
    PngEncoder::new(&mut buf)
        .write_image(rgba, w as u32, h as u32, ColorType::Rgba8.into())
        .ok();
    buf
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::time::Duration;

    fn make_frame(idx: u64, content_id: u64) -> DecodedFrame {
        DecodedFrame {
            rgba: Arc::from(vec![0u8; 4 * 4 * 4].as_slice()),
            width: 4,
            height: 4,
            frame_index: idx,
            presentation_offset: Duration::ZERO,
            content_id,
        }
    }

    #[test]
    fn try_new_requires_animation_protocol() {
        let caps = TerminalCaps {
            kitty_graphics: true,
            kitty_animation_protocol: false,
            ..Default::default()
        };
        assert!(KittyNativeRenderer::try_new(&caps).is_none());
        let caps = TerminalCaps {
            kitty_graphics: true,
            kitty_animation_protocol: true,
            ..Default::default()
        };
        assert!(KittyNativeRenderer::try_new(&caps).is_some());
    }

    #[test]
    fn first_frame_uses_action_t() {
        let caps = TerminalCaps {
            kitty_graphics: true,
            kitty_animation_protocol: true,
            ..Default::default()
        };
        let mut r = KittyNativeRenderer::try_new(&caps).unwrap();
        let bytes = r.transmit_frame(
            &make_frame(0, 1),
            &RenderContext {
                engine_id: 7,
                cell_position: (0, 0),
                previous_content_id: None,
            },
        );
        let s = String::from_utf8_lossy(&bytes);
        assert!(s.contains("a=T"));
    }

    #[test]
    fn second_frame_uses_action_a() {
        let caps = TerminalCaps {
            kitty_graphics: true,
            kitty_animation_protocol: true,
            ..Default::default()
        };
        let mut r = KittyNativeRenderer::try_new(&caps).unwrap();
        let _ = r.transmit_frame(
            &make_frame(0, 1),
            &RenderContext {
                engine_id: 7,
                cell_position: (0, 0),
                previous_content_id: None,
            },
        );
        let bytes = r.transmit_frame(
            &make_frame(1, 2),
            &RenderContext {
                engine_id: 7,
                cell_position: (0, 0),
                previous_content_id: Some(1),
            },
        );
        let s = String::from_utf8_lossy(&bytes);
        assert!(s.contains("a=a"));
        assert!(!s.contains("a=T"));
    }

    #[test]
    fn teardown_emits_delete() {
        let caps = TerminalCaps {
            kitty_graphics: true,
            kitty_animation_protocol: true,
            ..Default::default()
        };
        let mut r = KittyNativeRenderer::try_new(&caps).unwrap();
        let bytes = r.teardown(42);
        let s = String::from_utf8_lossy(&bytes);
        assert!(s.contains("a=d"));
        assert!(s.contains("i=42"));
    }
}
