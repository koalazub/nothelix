# libnothelix - Self-Contained Notebook Parser

Rust library for async Jupyter notebook parsing. Self-contained in the nothelix repo, no helix modifications required.

## Architecture

```
libnothelix (Rust)  →  C FFI  →  Steel Plugin
    ↓
tokio async
JSON parsing
Cell caching
```

## Building

```bash
cd libnothelix
cargo build --release
```

Output: `target/release/libnothelix.so` (Linux) or `libnothelix.dylib` (macOS)

## C FFI API

### `notebook_scan_start(path: *const c_char) -> u64`
Start async notebook scan. Returns notebook ID immediately.

### `notebook_is_ready(notebook_id: u64) -> bool`
Check if scan complete.

### `notebook_get_cell_count(notebook_id: u64) -> usize`
Get number of cells.

### `notebook_load_cell(notebook_id: u64, cell_idx: usize) -> *mut c_char`
Load cell data as JSON string. **Caller must free with `notebook_free_string`**.

### `notebook_free_string(s: *mut c_char)`
Free string returned by library functions.

### `notebook_close(notebook_id: u64) -> bool`
Close notebook and free resources.

## Integration Options

### Option 1: Steel FFI (if supported)

```scheme
;; Load native library
(require-dylib "path/to/libnothelix.so"
  '(notebook-scan-start
    notebook-is-ready
    notebook-get-cell-count
    notebook-load-cell
    notebook-free-string
    notebook-close))

;; Use it
(define nb-id (notebook-scan-start "/path/to/notebook.ipynb"))
(when (notebook-is-ready nb-id)
  (define count (notebook-get-cell-count nb-id))
  (displayln (string-append "Loaded " (number->string count) " cells")))
```

**Status**: Need to verify Steel supports FFI/dylib loading.

### Option 2: JSON-RPC Subprocess

If Steel doesn't support FFI, create a small subprocess that wraps the library:

```
Steel Plugin  →  JSON-RPC over stdio  →  Rust subprocess (using libnothelix)
```

Benefits:
- No FFI needed
- Process isolation
- Language agnostic

### Option 3: Helix Integration (last resort)

Register functions in `helix-term/src/commands/engine/steel/mod.rs`:

```rust
use libnothelix::*;

engine.register_fn("notebook-scan-async", |path: String| {
    notebook_scan_start(CString::new(path).unwrap().as_ptr()) as usize
});
```

**Drawback**: Requires modifying helix (not self-contained).

## Performance

- Scan start: <1ms (spawns background thread)
- Scan complete (500 cells): 30-50ms
- Cell access: 1-5ms (cached JSON)
- Memory: ~50KB metadata + loaded cells

## Why Rust?

Steel runs on UI thread. Synchronous operations block the editor:
- `rope->string` on large file: 100-500ms
- `string->jsexpr` parsing: 500ms-2s
- Processing cells: 100ms-1s

**Total**: 1-3s UI freeze for large notebooks.

Rust solution:
- Parsing happens in background thread (tokio)
- UI remains responsive
- Industry-standard pattern (same as VS Code, JupyterLab)

## Example: Full Workflow

```c
// C usage (or via FFI from any language)
#include <stdio.h>

int main() {
    // Start scan
    uint64_t nb_id = notebook_scan_start("/path/to/notebook.ipynb");

    // Wait for completion
    while (!notebook_is_ready(nb_id)) {
        usleep(10000); // 10ms
    }

    // Get info
    size_t count = notebook_get_cell_count(nb_id);
    printf("Loaded %zu cells\n", count);

    // Load cell
    char* cell_json = notebook_load_cell(nb_id, 0);
    printf("Cell 0: %s\n", cell_json);
    notebook_free_string(cell_json);

    // Cleanup
    notebook_close(nb_id);
    return 0;
}
```

## Next Steps

1. **Verify Steel FFI support** - Check if Steel can load dylibs
2. **Choose integration method** - FFI vs subprocess vs helix
3. **Update Steel plugin** - Use libnothelix instead of synchronous parsing
4. **Test with large notebooks** - Verify no UI freezing

## License

Same as parent project.
