# Nothelix Architecture

Self-contained Jupyter notebook plugin for Helix editor.

## Overview

Nothelix enables Jupyter notebook execution within Helix using Steel plugins and a Rust async parser.

```
┌─────────────────────────────────────────────────────┐
│ Helix Editor (with Steel support)                  │
└────────────────┬────────────────────────────────────┘
                 │ Steel Plugin API
┌────────────────┴────────────────────────────────────┐
│ nothelix.scm - Main Plugin                         │
│ - Cell navigation & execution                       │
│ - Kernel management                                 │
│ - Output rendering                                  │
└────────┬───────────────────────────┬────────────────┘
         │                           │
         │ Rust FFI/IPC              │ Rust FFI
         ↓                           ↓
┌────────────────────┐    ┌──────────────────────────┐
│ libnothelix        │    │ Julia/Python Kernel      │
│ - Async parsing    │    │ - Code execution         │
│ - Cell caching     │    │ - Output capture         │
└────────────────────┘    └──────────────────────────┘
```

## Components

### 1. Steel Plugin (`plugin/`)

**nothelix.scm**:
- Main notebook interface
- Cell navigation (`:next-cell`, `:previous-cell`)
- Cell execution (`:execute-cell`)
- Cell picker UI

**notebook-rust.scm**:
- Interface to Rust parser
- Handles async communication

**nothelix-autoconvert.scm**:
- Optional auto-conversion to readable format
- Size limits to prevent UI freezing

### 2. Rust Parser (`libnothelix/`)

**Purpose**: Async notebook parsing without blocking UI

**Features**:
- Background scanning (30-50ms for 500 cells)
- On-demand cell loading (1-5ms each)
- Global cache for O(1) access
- C FFI for language-agnostic integration

**API**:
```c
uint64_t notebook_scan_start(const char* path);
bool notebook_is_ready(uint64_t notebook_id);
size_t notebook_get_cell_count(uint64_t notebook_id);
char* notebook_load_cell(uint64_t notebook_id, size_t cell_idx);
bool notebook_close(uint64_t notebook_id);
```

### 3. Kernel Manager

Currently uses file-based IPC:
```
/tmp/helix-kernel-{id}/
├── input.jl          # Code to execute
├── output.txt        # Execution results
└── completion.marker # Signals completion
```

**Registered Steel Functions** (in helix):
- `(kernel-start "julia" 1)` - Start kernel process
- `(kernel-execute 1 code)` - Execute code
- `(kernel-stop 1)` - Stop kernel

## Integration Methods

### Option 1: Steel FFI (Preferred)

If Steel supports native library loading:

```scheme
(require-dylib "~/.local/lib/libnothelix.so"
  '(notebook-scan-start
    notebook-is-ready
    notebook-load-cell
    notebook-close))
```

### Option 2: JSON-RPC Subprocess

Use `notebook-server` binary:

```scheme
;; Start subprocess
(define proc (spawn-process "notebook-server"))

;; Send JSON-RPC request
(write-line proc "{\"method\":\"scan\",\"params\":{...}}")

;; Read response
(define response (json-parse (read-line proc)))
```

### Option 3: File-based IPC (Fallback)

Using shell commands:

```scheme
;; Write request
(helix.run-shell-command "echo '{...}' > /tmp/nb-req.json")

;; Run parser
(helix.run-shell-command "notebook-server --once < /tmp/nb-req.json > /tmp/nb-resp.json")

;; Read response
(define response (file-read "/tmp/nb-resp.json"))
```

## Why This Architecture?

### Self-Contained

All notebook functionality in `nothelix` repo:
- No modifications to helix core
- Works with any helix build with Steel support
- Easy to install, remove, update

### Async Performance

**Synchronous Steel** (problematic):
```
Open notebook → rope->string (100-500ms) → JSON parse (500ms-2s) → UI frozen
```

**Async Rust** (solution):
```
Open notebook → spawn background task (<1ms) → UI responsive
                    ↓
              Parse in background (30-50ms)
                    ↓
              Cache cells, load on-demand (1-5ms each)
```

### Industry Standard Pattern

Same approach as:
- VS Code Jupyter extension
- JupyterLab
- Google Colab

They all:
1. Parse notebooks async (off UI thread)
2. Keep structured cell data
3. Load/render cells on-demand
4. Never flatten to text

## File Structure

```
nothelix/
├── README.md                    # Main entry point
├── docs/
│   ├── ARCHITECTURE.md          # This file
│   ├── HELIX_INTEGRATION.md     # RawContent integration
│   ├── KEYBINDINGS.md           # User keybindings
│   └── TESTING.md               # Testing guide
├── plugin/
│   ├── nothelix.scm             # Main plugin entry
│   └── nothelix/
│       ├── string-utils.scm     # String utilities
│       ├── graphics.scm         # Graphics rendering
│       ├── kernel.scm           # Kernel lifecycle
│       ├── conversion.scm       # Notebook conversion
│       ├── navigation.scm       # Cell navigation
│       ├── execution.scm        # Cell execution
│       ├── selection.scm        # Text selection
│       └── picker.scm           # Cell picker UI
├── libnothelix/
│   ├── Cargo.toml
│   └── src/
│       └── lib.rs               # Rust FFI (parsing, graphics, kernel)
└── kernel/
    ├── runner.jl                # Kernel main loop
    ├── cell_registry.jl         # Cell state management
    ├── ast_analysis.jl          # Dependency extraction
    ├── output_capture.jl        # Output/plot capture
    └── cell_macros.jl           # @cell and @markdown macros
```

## Design Principles

### 1. Dumb Core, Smart Plugins

Helix core remains minimal. All notebook logic in plugins.

### 2. Language Agnostic

Rust library has C FFI, usable from:
- Steel (Scheme)
- Python
- Julia
- Any language with C interop

### 3. Minimal Dependencies

- Steel plugins: Only builtins + JSON
- Rust library: tokio, serde, anyhow
- No heavyweight dependencies

### 4. Graceful Degradation

- Large notebooks (>2000 lines): Skip auto-convert, manual trigger available
- Missing kernel: Viewing still works
- Terminal without graphics: Text-only output

## Performance Targets

- Open notebook: <100ms perceived
- Scan 500 cells: <50ms background
- Load single cell: <5ms
- Execute cell: Depends on code (kernel latency)
- Memory: <1MB for metadata, loaded cells only

## Completed Features

1. ✅ **Async execution**: Non-blocking kernel execution via `enqueue-thread-local-callback-with-delay`
2. ✅ **Inline images**: RawContent API for plots/graphs (Kitty/iTerm2 protocols)
3. ✅ **Kernel lifecycle**: Start, stop, cleanup on editor exit

## Future Enhancements

1. **Virtual scrolling**: Only render visible cells
2. **Persistent cache**: Save parsed data, instant reopens
3. **Progressive rendering**: Display cells as they load
4. **Output streaming**: Real-time output during execution

## Testing Strategy

1. **Unit tests**: Rust parser, Steel utilities
2. **Integration tests**: End-to-end cell execution
3. **Performance tests**: Large notebook handling
4. **Terminal compatibility**: Kitty, iTerm2, xterm, etc.

## Upstreaming to Helix

Components that could be upstreamed:
- RawContent abstraction (see `docs/HELIX_INTEGRATION.md`)
- Performance optimizations
- Steel plugin examples

Components staying in plugin:
- All notebook-specific logic
- Kernel management
- Cell parsing
- Output rendering

This maintains separation: Helix provides primitives, plugins provide policy.

## Cell Identification System

### Problem Statement

Nothelix works with two file formats:
1. **Raw `.ipynb` files** - JSON format (preserved for interoperability with JupyterLab, VS Code, etc.)
2. **Converted `.jl` files** - Editable text format with cell markers (created by `:convert-notebook`)

The challenge: How do we map a cursor position in a converted file back to the original cell in the `.ipynb` file? This is required for commands like `:execute-cells-above` which need to know "which cells come before the current one".

### Solution: Metadata-Based Cell Tracking

#### Conversion Format

When converting a `.ipynb` file, we add metadata to enable bidirectional mapping:

```julia
# nothelix-source: /path/to/original.ipynb

# ─── Code Cell 0 [ ] ───
using Plots

# ─── Code Cell 1 [2] ───
plot([1,2,3], [1,4,9])

# ─── Markdown Cell 2 ───
# This is a markdown cell
```

**Key elements:**
- **Header line**: `# nothelix-source: <path>` - Stores path to original `.ipynb`
- **Cell indices**: `# ─── Code Cell 0 [ ] ───` - The `0` is the cell index in the original notebook
- **Execution count**: `[2]` shows this cell was executed (execution_count = 2), `[ ]` means never executed

#### Rust API Functions

**1. `get-cell-at-line(path: String, line: usize) -> JSON`**

Returns information about the cell containing the given line number.

```rust
// Implementation: libnothelix/src/lib.rs:1071-1136
fn get_cell_at_line_impl(path: &str, line_number: usize) -> Result<String> {
    // 1. Read file contents
    // 2. Extract source path from header: "# nothelix-source: <path>"
    // 3. Search backwards from line_number to find cell marker
    // 4. Parse cell index from marker: "# ─── Code Cell {index} ..."
    // 5. Return JSON: {"cell_index": idx, "source_path": path}
}
```

**Returns:**
```json
{
  "cell_index": 2,
  "source_path": "/Users/alielali/notebooks/analysis.ipynb"
}
```

**2. `notebook-list-cells(path: String) -> JSON`**

Returns metadata for all cells in a notebook.

**Returns:**
```json
{
  "cells": [
    {"index": 0, "type": "code"},
    {"index": 1, "type": "code"},
    {"index": 2, "type": "markdown"}
  ]
}
```

**3. `notebook-get-cell-code(path: String, index: isize) -> JSON`**

Gets the source code for a specific cell by index.

**Returns:**
```json
{
  "code": "plot([1,2,3], [1,4,9])",
  "type": "code"
}
```

### Usage in Execute Commands

#### execute-all-cells

Works on both `.ipynb` and converted files:

```scheme
(define (execute-all-cells)
  ;; 1. Get source notebook path
  (define notebook-path
    (if (string-suffix? path ".ipynb")
        path  ;; Use directly
        ;; Extract from metadata header
        (json-get-string (get-cell-at-line path 0) "source_path")))

  ;; 2. Iterate through all cells using indices
  (define cell-count (notebook-cell-count notebook-path))
  (let loop ([cell-idx 0])
    (when (< cell-idx cell-count)
      (define cell-code (notebook-get-cell-code notebook-path cell-idx))
      ;; Execute cell...
      (loop (+ cell-idx 1)))))
```

#### execute-cells-above

Only works properly on converted files (requires position tracking):

```scheme
(define (execute-cells-above)
  ;; 1. Find current cell and source notebook
  (define cell-info (get-cell-at-line path current-line))
  (define current-idx (string->number (json-get-string cell-info "cell_index")))
  (define notebook-path (json-get-string cell-info "source_path"))

  ;; 2. Execute cells 0 to current-idx (inclusive)
  (let loop ([cell-idx 0])
    (when (<= cell-idx current-idx)
      (define cell-code (notebook-get-cell-code notebook-path cell-idx))
      ;; Execute cell...
      (loop (+ cell-idx 1)))))
```

**For raw `.ipynb` files:** Falls back to `execute-all-cells` with a warning, since we can't determine cursor position in JSON.

### Advantages

✅ **No fragile string matching** - Cell identification uses structured metadata, not text searching
✅ **Bidirectional mapping** - Can go from line number → cell index → cell code
✅ **Format agnostic** - Works with both `.ipynb` and converted files
✅ **Persistent** - Cell indices survive edits to cell contents
✅ **Rust-backed** - All parsing happens in Rust for performance and reliability

### Limitations

⚠️ **Converted file required for position-aware commands** - `:execute-cells-above` needs a converted file to determine "which cell am I in?"

⚠️ **Manual reconversion** - If the original `.ipynb` changes, the converted file needs to be regenerated

⚠️ **Cell reordering** - Moving cells in the converted file doesn't update the indices (indices reflect original order)

### Future Improvements

1. **Auto-sync** - Detect when source `.ipynb` changes and offer to reconvert
2. **Two-way sync** - Write execution results back to `.ipynb` outputs
3. **Smart reordering** - Update cell indices when cells are moved
4. **Language detection** - Parse notebook metadata to detect kernel language automatically
