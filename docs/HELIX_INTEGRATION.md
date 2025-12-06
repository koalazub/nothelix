# Helix Integration: RawContent Implementation

Status of inline rendering integration between Helix and Nothelix.

## Implementation Status

### Helix Core (COMPLETE)

The RawContent rendering pipeline is fully implemented in `feature/inline-image-rendering` branch.

**Files modified**:

| File | Changes |
|------|---------|
| `helix-core/src/text_annotations.rs` | `RawContent` struct with ID-based equality |
| `helix-core/src/doc_formatter.rs` | `raw_content` field in `FormattedGrapheme` |
| `helix-term/src/ui/document.rs` | `draw_raw_content()` method, render loop integration |
| `helix-tui/src/buffer.rs` | `raw_writes` field, `write_raw_bytes()` method |
| `helix-tui/src/backend/mod.rs` | `draw_raw()` trait method |
| `helix-tui/src/backend/crossterm.rs` | `draw_raw()` implementation |
| `helix-tui/src/backend/termina.rs` | `draw_raw()` implementation |
| `helix-tui/src/terminal.rs` | ID-based diffing in `flush()` |

**Total**: ~150 LOC, no dependencies, backwards compatible.

### Steel API (COMPLETE)

```scheme
;; Add raw content to current document
(add-raw-content! payload height char-idx)
;; - payload: Vec<u8> - raw terminal escape sequences
;; - height: u16 - rows consumed
;; - char-idx: usize - document position
```

### Nothelix Plugin (COMPLETE)

All rendering components are now implemented:

## Implementation Status

### 1. Graphics Protocol (COMPLETE)

Implemented in `plugin/nothelix/graphics.scm` and `libnothelix/src/lib.rs`:

- Protocol detection (Kitty, iTerm2, Sixel)
- Escape sequence generation in Rust for performance
- `render-image-b64` function to render base64 image data
- `graphics-protocol` to check current terminal support

### 2. Image Capture (COMPLETE)

Implemented in Julia kernel (`kernel/output_capture.jl`, `kernel/cell_macros.jl`):

- Plot detection for Plots.jl, Makie, etc.
- PNG capture via `show(io, MIME("image/png"), plot)`
- Base64 encoding
- Images included in cell execution JSON result

### 3. Output Rendering Integration (COMPLETE)

Implemented in `plugin/nothelix/execution.scm`:

- `update-cell-output` checks for images after cell execution
- `json-get-first-image` (Rust FFI) extracts image data
- `render-image-b64` generates escape sequences via Rust
- `add-raw-content!` injects into document

### 4. Terminal Detection (COMPLETE)

Implemented in `libnothelix/src/lib.rs`:

- `detect-graphics-protocol` checks env vars
- `config-get-protocol` allows user override
- Supports: Kitty, Ghostty, iTerm2, WezTerm, Sixel

## Testing

### Manual Test

1. Build helix with the changes:
   ```bash
   cd ~/projects/helix
   git checkout feature/inline-image-rendering
   cargo build --release --features steel
   ```

2. Test raw content API:
   ```bash
   ./target/release/hx test.txt
   # In helix:
   :scm (add-raw-content! (string->bytes "\x1b[31mRED\x1b[0m") 1 0)
   ```

3. Should see "RED" in red at position 0.

### Image Test (After TODO Complete)

```scheme
:scm (render-image "/path/to/image.png" 0 10)
```

## Architecture

```
┌─────────────────────────────────────────────────────────┐
│                      Helix Core                          │
│  ┌──────────────┐    ┌──────────────┐    ┌───────────┐  │
│  │  RawContent  │───▶│ DocFormatter │───▶│ Terminal  │  │
│  │  (ID, bytes) │    │ (raw_content)│    │ (draw_raw)│  │
│  └──────────────┘    └──────────────┘    └───────────┘  │
│         ▲                                      │        │
│         │                                      ▼        │
│  ┌──────────────┐                      ┌───────────┐   │
│  │ Steel API    │                      │ Backend   │   │
│  │add-raw-content                      │crossterm/ │   │
│  └──────────────┘                      │termina    │   │
└─────────────────────────────────────────────────────────┘
         ▲
         │ (add-raw-content! payload height char-idx)
         │
┌─────────────────────────────────────────────────────────┐
│                    Nothelix Plugin                       │
│  ┌──────────────┐    ┌──────────────┐    ┌───────────┐  │
│  │ Cell Output  │───▶│ Kitty/Sixel  │───▶│ Escape    │  │
│  │ (PNG/SVG)    │    │ Encoder      │    │ Sequence  │  │
│  └──────────────┘    └──────────────┘    └───────────┘  │
└─────────────────────────────────────────────────────────┘
```

## Performance

The implementation uses ID-based diffing:

1. Each `RawContent` has unique `u64` ID
2. Terminal compares IDs between frames
3. Only sends content with new IDs
4. Unchanged images = 0 bytes sent

**Result**: Scrolling past images doesn't resend them.

For Kitty protocol specifically:
- First render: full base64 data (~2MB for large image)
- Subsequent renders: `\x1b_Ga=p,i={id}\x1b\\` (30 bytes)

## Detailed Data Flow

The complete path from cell execution to image display:

```
1. Cell Execution (execution.scm)
   └── kernel-execute-cell-start → Julia kernel
       └── Captures plot as PNG, base64 encodes, returns in JSON

2. Image Extraction (execution.scm:203)
   └── json-get-first-image (Rust FFI) → extracts base64 image data

3. Escape Sequence Generation (graphics.scm:127)
   └── render-image-b64 → render-b64-for-protocol (Rust FFI)
       └── libnothelix/src/lib.rs:382 → ffi_render_b64
           └── libnothelix/src/graphics/mod.rs:71 → render_base64_to_string
               └── libnothelix/src/graphics/kitty.rs:140 → encode()
                   └── Returns: "\x1b_Gf=100,t=d,a=T,i={id};{base64}\x1b\\"

4. RawContent Injection (graphics.scm:152)
   └── add-raw-content! (Helix Steel binding)
       └── helix-term/src/commands/engine/steel/mod.rs:5765
           └── doc.add_raw_content(view_id, RawContent{id, payload, height, char_idx})

5. Document Storage (helix-view/src/document.rs)
   └── raw_content: HashMap<ViewId, Vec<RawContent>>

6. Text Annotations (helix-view/src/view.rs:513)
   └── text_annotations.add_raw_content(raw_content)

7. Document Formatting (helix-core/src/doc_formatter.rs:454)
   └── FormattedGrapheme includes raw_content reference

8. Text Rendering (helix-term/src/ui/document.rs:321)
   └── draw_raw_content() → surface.write_raw_bytes(id, x, y, payload)

9. Buffer Storage (helix-tui/src/buffer.rs:300)
   └── raw_writes: Vec<(u64, u16, u16, Vec<u8>)>

10. Terminal Flush (helix-tui/src/terminal.rs:159)
    └── ID-based diffing: only sends new IDs not in previous frame
        └── backend.draw_raw(&new_writes)

11. Backend Output (helix-tui/src/backend/termina.rs:562)
    └── write!(terminal, "\x1b[{};{}H", y+1, x+1) → cursor position
        └── terminal.write_all(bytes) → escape sequence to terminal
```

## Troubleshooting

### Issue: Image data flows but nothing displays

**Symptoms**:
- Logs show `add-raw-content!` called with correct payload
- Logs show `backend.draw_raw completed successfully`
- No image visible in terminal

**Diagnostic Steps**:

1. **Verify terminal protocol support**:
   ```bash
   /tmp/test_kitty_graphics.sh  # Created test script
   ```

2. **Check escape sequence format** (in helix.log):
   ```
   [termina.rs:draw_raw] first_100_str="\x1b_Gf=100,t=d,a=T,i=1;..."
   ```
   Should start with `\x1b_G` for Kitty protocol.

3. **Verify ID-based diffing**:
   ```
   [terminal.rs:flush] new_writes (after diff) count=1
   ```
   If count=0, the image was already sent in a previous frame.

4. **Check screen position**:
   ```
   [termina.rs:draw_raw] MoveTo(8,55)
   ```
   Position must be within visible terminal bounds.

### Issue: Protocol detection returns wrong value

**Fix for Ghostty**:
`libnothelix/src/graphics/registry.rs:40-47` now checks for `term.contains("ghostty")`.

### Issue: Escape sequence corrupted

If hex dump shows unexpected bytes, check:
1. String encoding in `render_base64_to_string` (should use `String::from_utf8_lossy`)
2. Steel string handling (UTF-8 safe)

### Issue: Image shows as empty square / line numbers disappear

**Root cause**: Kitty graphics protocol requires chunked transmission for payloads > 4096 bytes.

**Fix applied**: `libnothelix/src/graphics/kitty.rs` now chunks large payloads:
- First chunk: `\x1b_Gf=100,t=d,a=T,i=1,m=1;[data]\x1b\\`
- Middle chunks: `\x1b_Gm=1;[data]\x1b\\`
- Last chunk: `\x1b_Gm=0;[data]\x1b\\`

**Tests added**:
- `chunked_transmission_for_large_payloads` - verifies chunking for >4096 byte payloads
- `small_payload_not_chunked` - verifies small payloads remain unchunked

## Test Infrastructure

```bash
# Rust library tests
cd libnothelix && cargo test

# Steel diagnostic tests (in Helix)
:scm (require "plugins/tests/diagnostic-tests.scm")
```

## References

- [Kitty Graphics Protocol](https://sw.kovidgoyal.net/kitty/graphics-protocol/)
- [iTerm2 Inline Images](https://iterm2.com/documentation-images.html)
- [Sixel Graphics](https://en.wikipedia.org/wiki/Sixel)
- [Helix Steel PR #8675](https://github.com/helix-editor/helix/pull/8675)
