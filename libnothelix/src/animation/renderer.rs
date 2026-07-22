use crate::animation::decoder::DecodedFrame;
use crate::animation::renderers::static_fallback::StaticFallbackRenderer;
use crate::error::Result;

#[derive(Debug, Clone, Default)]
pub struct TerminalCaps {
    pub kitty_graphics: bool,
    pub kitty_animation_protocol: bool,
}

impl TerminalCaps {
    pub const HOST_DEFAULT: Self = Self {
        kitty_graphics: true,
        kitty_animation_protocol: false,
    };
}

pub trait AnimationRenderer: Send {
    fn name(&self) -> &'static str;
    fn transmit_frame(&mut self, frame: &DecodedFrame, engine_id: u64) -> Result<Vec<u8>>;
    fn teardown(&mut self, _engine_id: u64) -> Vec<u8> {
        Vec::new()
    }
}

pub type RendererFactory = fn(&TerminalCaps) -> Option<Box<dyn AnimationRenderer>>;

pub struct RendererEntry {
    pub priority: u32,
    pub factory: RendererFactory,
}

inventory::collect!(RendererEntry);

pub fn select_renderer(caps: &TerminalCaps) -> Box<dyn AnimationRenderer> {
    let mut entries: Vec<&RendererEntry> = inventory::iter::<RendererEntry>.into_iter().collect();
    entries.sort_by_key(|entry| entry.priority);
    entries
        .into_iter()
        .find_map(|entry| (entry.factory)(caps))
        .unwrap_or_else(StaticFallbackRenderer::boxed)
}
