use crate::animation::decoder::DecodedFrame;
use crate::animation::renderer::{AnimationRenderer, RendererEntry, TerminalCaps};
use crate::animation::renderers::png;
use crate::error::Result;
use base64::Engine;
use std::collections::HashSet;
use std::io::Write;

const APC_CHUNK_BYTES: usize = 4096;

pub struct KittyNativeRenderer {
    transmitted: HashSet<u64>,
    png_scratch: Vec<u8>,
    base64_scratch: String,
}

impl KittyNativeRenderer {
    pub fn try_new(caps: &TerminalCaps) -> Option<Box<dyn AnimationRenderer>> {
        (caps.kitty_graphics && caps.kitty_animation_protocol).then(|| {
            Box::new(KittyNativeRenderer {
                transmitted: HashSet::new(),
                png_scratch: Vec::new(),
                base64_scratch: String::new(),
            }) as Box<dyn AnimationRenderer>
        })
    }
}

inventory::submit! {
    RendererEntry { priority: 10, factory: KittyNativeRenderer::try_new }
}

impl AnimationRenderer for KittyNativeRenderer {
    fn name(&self) -> &'static str {
        "kitty-native"
    }

    fn transmit_frame(&mut self, frame: &DecodedFrame, engine_id: u64) -> Result<Vec<u8>> {
        self.png_scratch.clear();
        png::encode_rgba_into(
            &mut self.png_scratch,
            frame.rgba.as_ref(),
            frame.width,
            frame.height,
        )?;
        self.base64_scratch.clear();
        base64::engine::general_purpose::STANDARD
            .encode_string(&self.png_scratch, &mut self.base64_scratch);
        let image_id = engine_id as u32;
        let key_value = if self.transmitted.insert(engine_id) {
            format!("a=T,f=100,i={image_id},q=2")
        } else {
            format!("a=a,i={image_id},r=1,q=2")
        };
        let mut out = Vec::new();
        write_chunked_apc(&mut out, &key_value, &self.base64_scratch);
        Ok(out)
    }

    fn teardown(&mut self, engine_id: u64) -> Vec<u8> {
        self.transmitted.remove(&engine_id);
        format!("\x1b_Ga=d,d=I,i={engine_id};\x1b\\").into_bytes()
    }
}

fn write_chunked_apc(out: &mut Vec<u8>, key_value: &str, payload_base64: &str) {
    let bytes = payload_base64.as_bytes();
    let total = bytes.len().div_ceil(APC_CHUNK_BYTES).max(1);
    for (index, chunk) in bytes.chunks(APC_CHUNK_BYTES).enumerate() {
        let more = u8::from(index + 1 != total);
        if index == 0 {
            let _ = write!(out, "\x1b_G{key_value},m={more};");
        } else {
            let _ = write!(out, "\x1b_Gm={more};");
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

    fn native_caps() -> TerminalCaps {
        TerminalCaps {
            kitty_graphics: true,
            kitty_animation_protocol: true,
        }
    }

    #[test]
    fn try_new_requires_animation_protocol() {
        let without = TerminalCaps {
            kitty_graphics: true,
            kitty_animation_protocol: false,
        };
        assert!(KittyNativeRenderer::try_new(&without).is_none());
        assert!(KittyNativeRenderer::try_new(&native_caps()).is_some());
    }

    #[test]
    fn first_frame_uses_action_t() {
        let mut renderer = KittyNativeRenderer::try_new(&native_caps()).unwrap();
        let bytes = renderer.transmit_frame(&make_frame(0, 1), 7).unwrap();
        assert!(String::from_utf8_lossy(&bytes).contains("a=T"));
    }

    #[test]
    fn second_frame_uses_action_a() {
        let mut renderer = KittyNativeRenderer::try_new(&native_caps()).unwrap();
        let _ = renderer.transmit_frame(&make_frame(0, 1), 7).unwrap();
        let bytes = renderer.transmit_frame(&make_frame(1, 2), 7).unwrap();
        let text = String::from_utf8_lossy(&bytes);
        assert!(text.contains("a=a"));
        assert!(!text.contains("a=T"));
    }

    #[test]
    fn teardown_emits_delete() {
        let mut renderer = KittyNativeRenderer::try_new(&native_caps()).unwrap();
        let text = String::from_utf8_lossy(&renderer.teardown(42)).into_owned();
        assert!(text.contains("a=d"));
        assert!(text.contains("i=42"));
    }
}
