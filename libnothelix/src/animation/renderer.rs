use crate::animation::decoder::DecodedFrame;

#[derive(Debug, Clone, Default)]
pub struct TerminalCaps {
    pub kitty_graphics: bool,
    pub kitty_animation_protocol: bool,
    pub max_fps: u32,
}

pub struct RenderContext {
    pub engine_id: u64,
    pub cell_position: (u16, u16),
    pub previous_content_id: Option<u64>,
}

#[derive(Debug, Clone, Default)]
pub struct RendererCapabilities {
    pub supports_native_animation: bool,
    pub supports_diff_frames: bool,
    pub max_dimensions: Option<(u16, u16)>,
}

pub trait AnimationRenderer: Send {
    fn name(&self) -> &'static str;
    fn capabilities(&self) -> RendererCapabilities;
    fn transmit_frame(&mut self, frame: &DecodedFrame, ctx: &RenderContext) -> Vec<u8>;
    fn teardown(&mut self, _engine_id: u64) -> Vec<u8> { Vec::new() }
}

pub type RendererFactory = fn(&TerminalCaps) -> Option<Box<dyn AnimationRenderer>>;

pub struct RendererEntry {
    pub priority: u32, // lower = preferred
    pub factory: RendererFactory,
}

inventory::collect!(RendererEntry);

pub fn select_renderer(caps: &TerminalCaps) -> Box<dyn AnimationRenderer> {
    let mut entries: Vec<&RendererEntry> = inventory::iter::<RendererEntry>.into_iter().collect();
    entries.sort_by_key(|e| e.priority);
    entries
        .into_iter()
        .find_map(|e| (e.factory)(caps))
        // The static-fallback renderer registers with `priority = MAX`
        // and unconditionally returns `Some(_)`, so the iterator always
        // yields. If this fires, the inventory registration was lost
        // (build-system regression) — surface the invariant explicitly
        // rather than panicking via index.
        .unwrap_or_else(|| {
            unreachable!(
                "static fallback renderer must always succeed — \
                 inventory registration was lost",
            )
        })
}
