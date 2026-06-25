use crate::animation::decoder::DecodedFrame;
use crate::animation::renderer::{
    AnimationRenderer, RenderContext, RendererCapabilities, RendererEntry, TerminalCaps,
};
use crate::animation::renderers::encode_rgba_to_png_into;
use base64::Engine;
use std::collections::HashSet;
use std::io::Write;

pub struct KittyNativeRenderer {
    /// Engine ids whose first frame has already been transmitted with
    /// action `T`; subsequent frames for those ids use action `a`.
    sent_first_frame: HashSet<u64>,
    /// Reusable scratch for the PNG bytes of the current frame, cleared and
    /// refilled each `transmit_frame` rather than reallocated per frame.
    png_scratch: Vec<u8>,
    /// Reusable scratch for the base64 encoding of `png_scratch`.
    b64_scratch: String,
}

impl KittyNativeRenderer {
    pub fn try_new(caps: &TerminalCaps) -> Option<Box<dyn AnimationRenderer>> {
        if caps.kitty_graphics && caps.kitty_animation_protocol {
            Some(Box::new(KittyNativeRenderer {
                sent_first_frame: HashSet::new(),
                png_scratch: Vec::new(),
                b64_scratch: String::new(),
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
    fn name(&self) -> &'static str {
        "kitty-native"
    }

    fn capabilities(&self) -> RendererCapabilities {
        RendererCapabilities {
            supports_native_animation: true,
            supports_diff_frames: true,
            max_dimensions: None,
        }
    }

    fn transmit_frame(&mut self, frame: &DecodedFrame, ctx: &RenderContext) -> Vec<u8> {
        let first = !self.sent_first_frame.contains(&ctx.engine_id);
        self.png_scratch.clear();
        encode_rgba_to_png_into(
            &mut self.png_scratch,
            frame.rgba.as_ref(),
            frame.width,
            frame.height,
        );
        self.b64_scratch.clear();
        base64::engine::general_purpose::STANDARD
            .encode_string(&self.png_scratch, &mut self.b64_scratch);
        let image_id = ctx.engine_id as u32;
        let mut out = Vec::new();
        let key_value = if first {
            self.sent_first_frame.insert(ctx.engine_id);
            format!("a=T,f=100,i={image_id},q=2")
        } else {
            format!("a=a,i={image_id},r=1,q=2")
        };
        chunked_apc(&mut out, &key_value, &self.b64_scratch);
        out
    }

    fn teardown(&mut self, engine_id: u64) -> Vec<u8> {
        self.sent_first_frame.remove(&engine_id);
        format!("\x1b_Ga=d,d=I,i={engine_id};\x1b\\").into_bytes()
    }
}

/// Emit one or more APC sequences to transmit `payload_b64` in 4096-byte chunks
/// directly into `out`. First chunk carries `key_value`; continuations carry
/// only `m=<more>`. The chunk bytes are appended verbatim — no throwaway
/// per-chunk `String` — using `write!` for the small APC framing only.
fn chunked_apc(out: &mut Vec<u8>, key_value: &str, payload_b64: &str) {
    const CHUNK: usize = 4096;
    let bytes = payload_b64.as_bytes();
    let total_chunks = bytes.len().div_ceil(CHUNK).max(1);
    for (i, chunk) in bytes.chunks(CHUNK).enumerate() {
        let last = i + 1 == total_chunks;
        let m = if last { 0 } else { 1 };
        if i == 0 {
            let _ = write!(out, "\x1b_G{key_value},m={m};");
        } else {
            let _ = write!(out, "\x1b_Gm={m};");
        }
        out.extend_from_slice(chunk);
        out.extend_from_slice(b"\x1b\\");
    }
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
