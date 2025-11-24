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

### Nothelix Plugin (TODO)

What remains to implement in this repo:

## TODO List

### 1. Kitty Graphics Protocol Helper

Create `plugin/kitty-graphics.scm`:

```scheme
;; Generate Kitty graphics escape sequence
(define (kitty-image-payload id base64-data width height rows)
  ;; Returns bytes for: \x1b_Gf=100,a=T,i={id},s={width},v={height};{base64}\x1b\\
  ...)

;; Check if terminal supports Kitty graphics
(define (kitty-graphics-supported?)
  ;; Check TERM/TERM_PROGRAM env vars
  ...)

;; Render image at position (high-level API)
(define (render-image path char-idx rows)
  (let* ((data (read-file-bytes path))
         (b64 (base64-encode data))
         (id (generate-unique-id))
         (payload (kitty-image-payload id b64 0 0 rows)))
    (add-raw-content! payload rows char-idx)))
```

### 2. Base64 Encoding

Either:
- Use Rust FFI from libnothelix
- Implement in Steel (slower but simpler)

```scheme
;; In libnothelix, expose:
(define (base64-encode bytes) ...)
(define (read-file-bytes path) ...)
```

### 3. Output Rendering Integration

Update `plugin/nothelix.scm` to render cell outputs:

```scheme
(define (render-cell-output cell-idx)
  (let* ((output (notebook-get-cell-output cell-idx))
         (mime-type (output-mime-type output)))
    (cond
      ((string=? mime-type "image/png")
       (render-image-output output))
      ((string=? mime-type "text/plain")
       (render-text-output output))
      (else
       (render-fallback output)))))

(define (render-image-output output)
  (let* ((b64-data (output-data output))
         (char-idx (output-char-position output))
         (rows (calculate-image-rows output)))
    (when (kitty-graphics-supported?)
      (let ((payload (kitty-image-payload (output-id output) b64-data 0 0 rows)))
        (add-raw-content! payload rows char-idx)))))
```

### 4. Terminal Detection

```scheme
(define (detect-graphics-protocol)
  (let ((term (getenv "TERM"))
        (term-program (getenv "TERM_PROGRAM")))
    (cond
      ((or (string-contains? term "kitty")
           (string=? term-program "kitty")
           (string=? term-program "ghostty"))
       'kitty)
      ((or (string=? term-program "WezTerm")
           (string=? term-program "iTerm.app"))
       'iterm2)
      ((string-contains? term "xterm")
       'sixel)
      (else 'none))))
```

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

## Files to Create

```
nothelix/
├── plugin/
│   ├── kitty-graphics.scm    # TODO: Kitty protocol encoder
│   ├── sixel.scm             # TODO: Sixel fallback (optional)
│   └── image-render.scm      # TODO: High-level render API
├── libnothelix/
│   └── src/
│       └── base64.rs         # TODO: Base64 + file reading FFI
```

## References

- [Kitty Graphics Protocol](https://sw.kovidgoyal.net/kitty/graphics-protocol/)
- [iTerm2 Inline Images](https://iterm2.com/documentation-images.html)
- [Sixel Graphics](https://en.wikipedia.org/wiki/Sixel)
- [Helix Steel PR #8675](https://github.com/helix-editor/helix/pull/8675)
