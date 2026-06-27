# Animated Media — Design Spec

Date: 2026-04-25
Status: Approved (brainstorm)
Scope: nothelix + helix fork (`koalazub/helix feature/inline-image-rendering`)

## Goal

Render animated media inline in nothelix cells with the same fidelity and ergonomics as the existing static-image pipeline. Cover GIF, APNG, animated WebP, MP4, WebM, and Lottie. Accept media from two sources — markdown attachments in `.ipynb` cells, and `display_data` outputs from any kernel — with no coupling to specific Julia (or other) libraries. Performance and battery efficiency are first-class constraints; phasing is not.

## Non-goals

- Editing animated media inside the editor.
- Audio playback.
- Streaming network sources (HTTP video). All media must arrive as decoded byte buffers.
- Backwards compatibility with vanilla upstream helix. The plugin probes for fork APIs and degrades to static-image-only when missing.

## Architectural principles

1. **Library-agnostic.** The contract is the MIME bundle. Whatever produces the bytes (Plots.jl, Makie, matplotlib, manual `display`) is irrelevant to nothelix.
2. **Strong abstractions.** Three orthogonal extension points: decoders (per MIME), renderers (per terminal protocol), kernel sources (already MIME-driven, no new code needed).
3. **No platform reach-around.** Only the editor talks to the OS / window system. libnothelix exposes pure functions; the plugin (Steel) is the conduit between editor events and libnothelix.
4. **Single failure mode.** Anything that can't animate becomes a static image. The cell always renders.

## High-level architecture

```
                 ┌────────────────────────────────────────────────┐
                 │              Source layer                      │
                 │   • markdown attachments (notebook.rs path)    │
                 │   • kernel display_data outputs (any language) │
                 │   Both produce a (mime, bytes) pair.           │
                 └─────────────────────┬──────────────────────────┘
                                       │
                 ┌─────────────────────▼──────────────────────────┐
                 │           libnothelix::animation               │
                 │   • DECODERS table:  mime  → AnimatedDecoder   │
                 │   • RENDERERS table: caps  → AnimationRenderer │
                 │   • AnimationEngine = (Decoder, Renderer,      │
                 │                        bounded LRU frame cache,│
                 │                        playback state)         │
                 │   FFI surface: register / tick / drop          │
                 └─────────────────────┬──────────────────────────┘
                                       │
                 ┌─────────────────────▼──────────────────────────┐
                 │          plugin/animation.scm                  │
                 │   • subscribes: focus-lost/gained,             │
                 │                 viewport-changed               │
                 │   • drives the tick loop via                   │
                 │     enqueue-thread-local-callback-with-delay   │
                 │   • holds *animations* state hash              │
                 │   • commands: register / toggle-at-cursor /    │
                 │               pause-all / resume-all           │
                 └─────────────────────┬──────────────────────────┘
                                       │
                 ┌─────────────────────▼──────────────────────────┐
                 │       Forked helix (new APIs added)            │
                 │   • DocumentFocusGained event                  │
                 │   • ViewportChanged event                      │
                 │   • RawContent { is_animating: bool }          │
                 │   • animation-aware redraw debounce            │
                 └────────────────────────────────────────────────┘
```

## Source layer

Animated MIMEs accepted from both sources:

```
ANIMATED_MIMES = {
  "image/gif", "image/apng", "image/webp",
  "video/mp4", "video/webm",
  "application/json+lottie",
}
```

### Notebook attachment path

`libnothelix/src/notebook.rs::mime_for_extension` already maps `.gif` → `image/gif` etc. for markdown cell attachments. The existing flow that injects image refs into markdown is unchanged. New code: when a recognised animated MIME is encountered, in addition to writing the static reference, register an `AnimationEngine` in the per-document `AnimationRegistry` and tag the resulting `RawContent` with `is_animating: true`.

### Kernel output path

`kernel/output_capture.jl` already pushes `MIME("image/png")` via Julia's display system. To accept arbitrary MIMEs without library coupling, extend it to walk the `displayable` MIMEs in `ANIMATED_MIMES` order before falling back to PNG. The kernel emits whatever the user's library produced — `Plots.gif(anim, "x.gif")`, `VideoIO.openvideo`, `MIME"image/apng"` from a third-party package — and writes a base64 payload to `output.json`. libnothelix decodes by MIME, not by library identity.

The shape is exactly Jupyter's `display_data`:

```json
{
  "output_type": "display_data",
  "data": {
    "image/gif": "<base64>",
    "image/png": "<base64 fallback first frame>",
    "text/plain": "..."
  }
}
```

This is the entire coupling surface to "other toolkits." A new Julia library, a Python kernel, an R kernel — any kernel that emits a Jupyter-shaped `display_data` with one of the recognised MIMEs is rendered.

## libnothelix::animation module

### File layout

```
libnothelix/src/animation/
  mod.rs            -- public surface, AnimationRegistry, FFI
  decoder.rs        -- AnimatedDecoder trait, DECODERS table
  decoders/
    gif.rs          -- GifSource (image crate)
    apng.rs         -- ApngSource (image crate)
    webp.rs         -- WebpSource (image crate, animated path)
    mp4.rs          -- Mp4Source (mp4 + openh264)
    webm.rs         -- WebmSource (mp4 reader can't; use matroska + dav1d)
    lottie.rs       -- LottieSource (lottie-rs)
  renderer.rs       -- AnimationRenderer trait, RENDERERS table
  renderers/
    kitty_native.rs -- uses Kitty animation protocol
    kitty_replay.rs -- full-frame retransmit, works on any Kitty terminal
    static.rs       -- first-frame-only fallback
  cache.rs          -- bounded LRU per engine
  engine.rs         -- AnimationEngine: composes decoder + renderer + cache + state
  registry.rs       -- AnimationRegistry: per-document HashMap<u64, AnimationEngine>
```

### Decoder trait

```rust
pub trait AnimatedDecoder: Send {
    fn metadata(&self) -> AnimationMetadata;
    fn frame_at(&mut self, elapsed: Duration) -> Result<Option<DecodedFrame>>;
    fn seek(&mut self, elapsed: Duration) -> Result<()>;
}

pub struct AnimationMetadata {
    pub width: u16,
    pub height: u16,
    pub frame_count: Option<u64>,        // None for streaming sources
    pub native_fps: f32,                 // best-effort average
    pub total_duration: Option<Duration>,
    pub loops_natively: bool,
}

pub struct DecodedFrame {
    pub rgba: Arc<[u8]>,                 // tightly packed RGBA8
    pub frame_index: u64,
    pub presentation_offset: Duration,
    pub content_id: u64,                 // hash of rgba; renderer skips on match
}

type DecoderFactory = fn(&[u8]) -> Result<Box<dyn AnimatedDecoder>>;

static DECODERS: &[(&str, DecoderFactory)] = &[
    ("image/gif",                GifSource::open),
    ("image/apng",               ApngSource::open),
    ("image/webp",               WebpSource::open),
    #[cfg(feature = "video")]
    ("video/mp4",                Mp4Source::open),
    #[cfg(feature = "video")]
    ("video/webm",               WebmSource::open),
    #[cfg(feature = "lottie")]
    ("application/json+lottie",  LottieSource::open),
];
```

Cargo features: `gif` (default), `apng` (default), `webp` (default), `video` (off), `lottie` (off). A trimmed build is one-line.

### Renderer trait

```rust
pub trait AnimationRenderer: Send {
    fn capabilities(&self) -> RendererCapabilities;
    fn transmit_frame(&mut self, frame: &DecodedFrame, ctx: &RenderContext) -> Vec<u8>;
    fn teardown(&mut self, engine_id: u64) -> Vec<u8>;
}

pub struct RendererCapabilities {
    pub supports_native_animation: bool,
    pub supports_diff_frames: bool,
    pub max_dimensions: Option<(u16, u16)>,
}

pub struct RenderContext {
    pub engine_id: u64,
    pub cell_position: (u16, u16),
    pub previous_content_id: Option<u64>,
}

static RENDERERS: &[fn(&TerminalCaps) -> Option<Box<dyn AnimationRenderer>>] = &[
    KittyNativeRenderer::try_new,
    KittyReplayRenderer::try_new,
    StaticFallbackRenderer::try_new,
];
```

`StaticFallbackRenderer::try_new` always returns `Some`, so renderer selection cannot fail.

### Engine and cache

```rust
pub struct AnimationEngine {
    pub id: u64,
    decoder: Box<dyn AnimatedDecoder>,
    renderer: Box<dyn AnimationRenderer>,
    cache: FrameCache,
    metadata: AnimationMetadata,
    state: PlaybackState,
}

pub enum PlaybackState {
    Playing { started_at: Instant, accumulated_paused: Duration },
    Paused  { at_offset: Duration },
    Errored { reason: String },
    Finished,
}

pub struct FrameCache { /* bounded LRU keyed by frame_index, byte budget */ }
```

`AnimationEngine::tick(now: Instant) -> Option<TickOutput>`:
1. Compute `elapsed = now - started_at - accumulated_paused`.
2. Apply loop policy if configured.
3. `decoder.frame_at(elapsed)` (consult cache first).
4. Compare `content_id` with last frame; if equal, return `None`.
5. `renderer.transmit_frame(...)` to bytes.
6. Compute `next_delay_ms` from frame index + native fps + fps ceiling.
7. Return `Some(TickOutput { bytes, height, next_delay_ms })`.

### FFI surface

Exposed to Steel via existing `#%require-dylib`:

```rust
#[no_mangle]
pub extern "C" fn nothelix_animation_register(
    doc_id: u64,
    mime: *const c_char,
    bytes_ptr: *const u8,
    bytes_len: usize,
    out_engine_id: *mut u64,
    out_first_frame_png: *mut Buffer,  // for static fallback
    out_metadata_json: *mut Buffer,
) -> i32;

#[no_mangle]
pub extern "C" fn nothelix_animation_tick(
    engine_id: u64,
    elapsed_ms: u64,
    out_payload: *mut Buffer,
    out_height: *mut u16,
    out_next_delay_ms: *mut u32,
) -> i32; // 0 = wrote frame, 1 = no change, 2 = finished, <0 = error

#[no_mangle]
pub extern "C" fn nothelix_animation_drop(engine_id: u64);
```

Steel wraps these via the dylib bindings already used elsewhere in the plugin.

## Fork extensions

### `DocumentFocusGained` event

Add to `helix-view/src/events.rs`:

```rust
pub struct DocumentFocusGained<'a> {
    pub editor: &'a mut Editor,
    pub doc: DocumentId,
}
```

Fire from the same code path that currently fires `DocumentFocusLost` (helix-view tree focus change), on the doc that's gaining focus. Steel binding: add a `"document-focus-gained"` arm in `helix-term/src/commands/engine/steel/mod.rs` adjacent to the existing `"document-focus-lost"` arm.

### `ViewportChanged` event

Add to `helix-view/src/events.rs`:

```rust
pub struct ViewportChanged {
    pub view_id: ViewId,
    pub doc_id: DocumentId,
    pub anchor_char_idx: usize,
    pub height: u16,
}
```

Fire from `helix-view/src/view.rs` whenever `view.offset.anchor` or `view.inner_area().height` changes — wrap mutations in setters that emit the event. Coalesce: at most one fire per view per redraw cycle (track a `viewport_dirty: bool` flag, drain on `start_frame`).

Steel binding: `"viewport-changed"` hook with callback signature `(view-id doc-id anchor-char-idx height)`.

### `RawContent::is_animating`

Add to `helix-core/src/text_annotations.rs::RawContent`:

```rust
pub struct RawContent {
    pub id: u64,
    pub payload: Arc<Vec<u8>>,
    pub height: u16,
    pub char_idx: usize,
    pub width: Option<u16>,
    pub placeholder_rows: Option<Arc<Vec<String>>>,
    pub is_animating: bool,    // NEW
}
```

`Document::add_or_replace_raw_content` is unchanged in semantics — id-based replace already does what we need for frame swapping. Steel `add-raw-content!` and `add-or-replace-raw-content!` gain an optional `:animating?` keyword arg defaulting to `#f`.

### Animation-aware redraw

Today `helix-view/src/editor.rs:2347` debounces redraw to 33 ms. Change:

```rust
// pseudo
let interval = if any_open_doc_has_animating_content(self) {
    Duration::from_millis(self.config.animation.redraw_interval_ms)
} else {
    Duration::from_millis(33)
};
```

`any_open_doc_has_animating_content` walks open documents and checks each `raw_content` map for any entry with `is_animating == true`. O(docs * raw_contents) — fine because both are small.

Config addition in `helix-view/src/editor.rs::Config` (or a sub-config struct):

```rust
pub struct AnimationConfig {
    pub redraw_interval_ms: u64,    // default 1000/60 ≈ 16
    pub max_fps: u32,               // default 60, used to clamp redraw_interval
}
```

`redraw_interval_ms` floor is 8 (120 fps cap, sanity). `max_fps` is the user's stated display refresh — it's the only piece of info we don't auto-detect. Documented in `nothelix.toml` as the value to bump for high-refresh monitors.

## Plugin (Steel) layer

### State

```scheme
;; engine_id -> hash with keys
;;   :char_idx, :view_id, :doc_id,
;;   :playback_started_ms, :paused_at_ms (or #f),
;;   :manual_paused?, :visible?, :focused?,
;;   :height, :native_fps, :status (one of 'playing 'paused 'errored 'finished)
(define *animations* (hash))
```

### Hook subscriptions (one-time, at plugin load)

```scheme
(register-hook! "document-focus-lost"
  (lambda (doc-id)
    (mark-doc-focus! doc-id #f)
    (cancel-doc-callbacks! doc-id)))

(register-hook! "document-focus-gained"
  (lambda (doc-id)
    (mark-doc-focus! doc-id #t)
    (resume-doc-engines! doc-id)))

(register-hook! "viewport-changed"
  (lambda (view-id doc-id anchor height)
    (recompute-visibility! doc-id anchor height)
    (resume-or-pause-engines! doc-id)))
```

### Tick loop

```scheme
(define (schedule-tick engine-id)
  (define st (hash-get *animations* engine-id))
  (when (and st (engine-active? st))
    (define elapsed-ms (compute-elapsed st))
    (define result (libnothelix/animation-tick engine-id elapsed-ms))
    (when (animation-result-has-frame? result)
      (define new-rc
        (rawcontent-from-bytes
          engine-id
          (animation-result-bytes result)
          (animation-result-height result)
          :animating? #t))
      (add-or-replace-raw-content! (state-view-id st) new-rc)
      (request-redraw))
    (define delay (animation-result-next-delay-ms result))
    (when (and delay (engine-active? st))
      (enqueue-thread-local-callback-with-delay delay
        (lambda () (schedule-tick engine-id))))))
```

Gating: `engine-active?` returns true iff `:visible? && :focused? && (not :manual_paused?) && status == 'playing`. When any flips false, the in-flight tick fires once, sees the gate, exits without rescheduling. Resuming reschedules from the appropriate offset.

### Commands and keybindings

| Command | Default binding | Behavior |
|---|---|---|
| `nothelix-animation/toggle-at-cursor` | `<space>p` (normal mode, when cursor is over an animated `RawContent`) | Flip `:manual_paused?`, update indicator, reschedule or cancel |
| `nothelix-animation/pause-all` | none | Sets `:manual_paused?` on every engine in current doc |
| `nothelix-animation/resume-all` | none | Clears `:manual_paused?` on every engine in current doc |

Keybindings registered in `plugin/nothelix.scm` setup, idempotent. The keymap entry uses the existing nothelix keymap-extension mechanism.

### Status indicator

Single grapheme rendered into the top-right cell of each animated overlay's `placeholder_rows` (which already carry per-frame text). States:

- `▶` playing
- `⏸` manual paused
- `⊘` off-viewport or unfocused
- `!` errored

Theme key: `ui.virtual.animation-state` (new). Default in nothelix theme: muted gray. Indicator suppressed when `animation.show_indicator = false` in `nothelix.toml`.

### First-run discoverability

On the first animation registration in any nothelix session, set status line: `"animation playing — <space>p to pause"`. Suppressed via `nothelix.toml` `animation.first_run_hint = false`. Tracked by a session-scoped `*first-hint-shown?*` flag.

## Configuration (`nothelix.toml`)

```toml
[animation]
enabled = true
max_fps = 60
decode_cache_mb = 64
max_dimensions = [3840, 2160]
max_duration_seconds = 600
preferred_renderer = "auto"        # auto | kitty-native | kitty-replay | static
first_run_hint = true
show_indicator = true
pause_on_focus_lost = true
pause_off_viewport = true

[animation.formats]
gif = true
apng = true
webp = true
mp4 = true
webm = true
lottie = false
```

## Error handling

| Failure | Behavior |
|---|---|
| Decoder rejects bytes | Static fallback via `image::load_from_memory`; if that fails, placeholder card with error text. Cell renders. No engine registered. |
| Source exceeds `max_dimensions` or `max_duration_seconds` | Static first frame + status warning naming the limit. |
| Renderer probe finds no native renderer | `StaticFallbackRenderer` always succeeds. First frame only. |
| Mid-stream decode error | Engine state set to `Errored`; pauses on last good frame; indicator shows `!`. Manual toggle attempts `seek(0)` retry. |
| FFI tick returns negative code | Steel marks engine `:errored`, stops scheduling. Last good frame stays visible. |
| Plugin loaded against vanilla helix (no fork APIs) | Probe at load time for `viewport-changed` hook registration. If absent, plugin sets a `degraded` flag, skips animation registration entirely, every animated MIME renders as static first frame, prints one-time startup warning. |

All errors flow through `libnothelix/src/error_format.rs` enrichment and surface in the cell output gutter, matching the existing kernel-error UX.

## Testing

### Decoder unit tests

`libnothelix/src/animation/decoders/<format>_test.rs`. One fixture per format under `libnothelix/tests/fixtures/animation/`:

- `tiny.gif` — 4 frames, 100 ms each, 32×32. Asserts: `metadata.frame_count == 4`, `frame_at(150ms)` returns frame index 1, `frame_at(450ms)` returns frame index 4 / loops, all `content_id` distinct.
- Equivalent fixtures for APNG, WebP. MP4/WebM/Lottie under their respective Cargo features.

### Engine tests

Deterministic clock — drive `engine.tick(now)` with a controlled `now`. Assertions:

- LRU evicts oldest entry when budget exceeded.
- `Paused` state freezes elapsed; subsequent resume advances from saved offset.
- `Finished` returned exactly once for non-looping sources past `total_duration`.

### Renderer tests

Golden-bytes snapshot. For a known `DecodedFrame`, assert the exact bytes returned by `KittyReplayRenderer::transmit_frame` against a checked-in golden file. Same for `KittyNativeRenderer` (separate golden — different escape sequences).

### Fork tests

Add to helix's existing integration test framework. Test: open a document, register a `RawContent` with `is_animating: true`, observe the editor's redraw cadence shortens to `animation.redraw_interval_ms`. Drop the content, observe the cadence returns to 33 ms.

### Plugin tests

Steel `cog-test` framework. Mock `libnothelix/animation-tick` to return a deterministic frame sequence. Assertions:

- `document-focus-lost` cancels in-flight callbacks.
- `viewport-changed` outside the cell pauses; back inside resumes.
- `<space>p` toggles `:manual_paused?` and indicator state.

### End-to-end smoke

Extend `nothelix doctor --smoke` with `--animation` flag. Spawns a Julia kernel, executes a cell that emits a known 10-frame GIF, captures the byte stream the plugin writes to the terminal, asserts ≥ 9 distinct frame transmissions occurred within 1.2× the GIF's natural duration. Exit code reports pass/fail; designed to run under CI with a Kitty-protocol terminal emulator.

## Performance budget

- Default 60 fps, capped by `animation.max_fps` and floored at 8 ms (120 fps).
- Idle cost when no animations registered: zero — the `is_animating` check is O(open-docs).
- Idle cost when one off-viewport animation: zero — the gate short-circuits before any FFI call.
- Active 30 fps GIF, 320×240, on Kitty native: ~25 KB/s wire bytes, one decode per frame, one cache hit per frame on repeat loops.
- Decode cache budget enforced per-engine; global memory cap = `decode_cache_mb × engine_count`. Engines pruned LRU when document closes.

## Build matrix

Default `cargo build`: gif + apng + webp.
`cargo build --features video`: + mp4 + webm.
`cargo build --features lottie`: + lottie.
`cargo build --no-default-features`: animation module compiles to a stub that returns "no decoder for MIME" for everything; static images still work.

## Open items deferred to implementation plan

- Exact timing semantics for the `viewport-changed` coalescing flag (where in `editor.rs::start_frame` to drain).
- Whether `KittyNativeRenderer` uses Kitty animation protocol's frame composition or transmits new images per frame and disposes the previous (the protocol supports both).
- Lottie crate selection (`lottie-rs` is the default but the ecosystem is unsettled).
- Whether the first-run hint persists across sessions (a state file in `~/.local/share/nothelix/`) or is per-session only.

These don't block the design; they're decisions the implementation plan will make.
