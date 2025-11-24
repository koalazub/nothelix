# Helix Notebook Plugin: Architecture & Vision

## Executive Summary

This document outlines the architecture for a high-performance, event-driven notebook plugin for Helix editor that aims to surpass VSCode notebooks, PyCharm notebooks, Pluto.jl, and Marimo in performance and developer experience.

**Core Principle**: Leverage Steel's Rust foundation and Helix's event system to build a truly async, multi-threaded notebook experience that doesn't block the editor.

---

## Vision & Goals

### Primary Objective
Create a notebook environment within Helix that:
- **Full `.ipynb` (Jupyter) compatibility**: Read, execute, and write standard Jupyter notebooks
- **Extensible architecture**: Plugin system for supporting Marimo, Pluto.jl, and future formats
- **Exceeds all competitors in performance**: Sub-millisecond UI responsiveness, parallel cell execution
- **Never blocks the editor**: All long-running operations are async
- **Leverages Rust's performance**: Zero-copy where possible, minimal allocations
- **Provides real-time feedback**: Streaming output, execution progress, resource usage

### Success Criteria
1. ✅ Open and render `.ipynb` files instantly (< 100ms for 1000 cells)
2. ✅ Execute cells without freezing the editor (async execution)
3. ✅ Stream cell output in real-time (not just on completion)
4. ✅ Full Jupyter protocol support (kernels, rich outputs, metadata)
5. ✅ Save back to `.ipynb` format preserving all metadata
6. ✅ Support multiple concurrent cell executions
7. ✅ Render markdown, LaTeX, plots, and tables inline
8. ✅ Language-agnostic via Jupyter kernels (Julia, Python, R, JavaScript, etc.)
9. ✅ Extensible to support other formats (Marimo `.py`, Pluto `.jl`, Quarto `.qmd`)

---

## Competitor Analysis

### Current Landscape

| Feature | VSCode | PyCharm | Pluto.jl | Marimo | **Helix (Target)** |
|---------|--------|---------|----------|--------|-------------------|
| Editor blocks during execution | ❌ Yes | ❌ Yes | ✅ No | ✅ No | ✅ **No** |
| Parallel cell execution | ❌ No | ❌ No | ⚠️ Limited | ⚠️ Limited | ✅ **Yes** |
| Streaming output | ⚠️ Limited | ⚠️ Limited | ❌ No | ✅ Yes | ✅ **Yes** |
| Reactive execution | ❌ No | ❌ No | ✅ Yes | ✅ Yes | ✅ **Yes** |
| Startup time | 🐌 3-5s | 🐌 5-10s | ⚡ 1-2s | ⚡ 1-2s | ⚡ **< 500ms** |
| Memory footprint | 🐘 500MB+ | 🐘 1GB+ | 🦎 50MB | 🦎 100MB | 🦎 **< 30MB** |
| Terminal-native | ❌ No | ❌ No | ❌ No | ⚠️ Web | ✅ **Yes** |

### Key Differentiators

1. **Steel + Rust Foundation**: Native performance, no Electron overhead
2. **Event-Driven Architecture**: Non-blocking by design
3. **Terminal-First**: No web server, no browser, SSH-friendly
4. **Modal Editing**: Vim keybindings for power users
5. **Minimal Dependencies**: Uses system Jupyter kernels, no bundled runtimes
6. **Full Jupyter Compatibility**: Standard `.ipynb` format, works with existing notebooks
7. **Extensible Format Support**: Plugin architecture for Marimo, Pluto.jl, Quarto, etc.

---

## Steel Capabilities & Limitations

### Current Capabilities ✅

Based on `/Users/alielali/projects/helix/STEEL.md` and source analysis:

#### 1. Native Threading
```scheme
(spawn-native-thread
  (lambda ()
    ;; Runs on OS thread
    (expensive-computation)))
```

#### 2. Context Marshalling
```scheme
(hx.block-on-task
  (lambda ()
    ;; Acquires helix context, blocks calling thread
    (insert-string "result")))

(hx.with-context
  (lambda ()
    ;; Schedules on main thread, non-blocking
    (insert-string "result")))
```

#### 3. Future/Callback Pattern
```scheme
(await-callback future-obj
  (lambda (result)
    ;; Called when future completes
    (process-result result)))
```

#### 4. Event System Integration
- `register-hook!` for document/mode events
- Typed commands via `provide`
- Keymap event interception

### Current Limitations ❌

#### 1. **No Built-in Process Spawning**
- Missing: `(run-process-async cmd args callback)`
- Workaround: Shell out via `insert-output`, but it's **synchronous**
- **Impact**: Blocks editor during cell execution ⚠️

#### 2. **No Streaming I/O**
- Missing: `(process-stdout-stream pid callback)`
- Workaround: Poll temp files, inefficient
- **Impact**: Can't show real-time output ⚠️

#### 3. **Limited Future Documentation**
- Steel supports futures, but API is undocumented
- Unknown: How to create futures, combine them, handle errors
- **Impact**: Can't build proper async primitives ⚠️

#### 4. **No Process Management**
- Missing: `(kill-process pid)`, `(process-alive? pid)`
- Workaround: Manual shell process management
- **Impact**: Can't cancel long-running cells ⚠️

#### 5. **No IPC Primitives**
- Missing: Channels, message passing between threads
- Available: Only shared mutable state via `set!`
- **Impact**: Race conditions, difficult coordination ⚠️

### Required Steel Enhancements

**Critical Path Items** (block async architecture):

1. **Async Process API**
   ```scheme
   ;; Proposed API
   (run-process-async
     command: "julia"
     args: ["/tmp/cell.jl"]
     on-stdout: (lambda (line) (append-output line))
     on-stderr: (lambda (line) (log-error line))
     on-exit: (lambda (code) (finalize-cell code)))
   ```

2. **Future Combinators**
   ```scheme
   (future-map f future)           ; Transform future result
   (future-all futures)            ; Await all futures
   (future-race futures)           ; First to complete
   (future-timeout ms future)      ; Timeout wrapper
   ```

3. **Channel-based Communication**
   ```scheme
   (make-channel)                  ; Create channel
   (channel-send! ch val)          ; Non-blocking send
   (channel-recv ch callback)      ; Async receive
   ```

---

## Proposed Architecture

### High-Level Design

```
┌─────────────────────────────────────────────────────────┐
│                    Helix Editor (Rust)                  │
│  ┌──────────────────────────────────────────────────┐   │
│  │              Steel Runtime (Main Thread)          │   │
│  │  • Event Loop                                     │   │
│  │  • Command Dispatch                               │   │
│  │  • UI Updates                                     │   │
│  └───────────┬──────────────────────────┬────────────┘   │
│              │                          │                │
│    ┌─────────▼────────┐      ┌─────────▼─────────┐      │
│    │ Notebook Manager │      │  Execution Queue  │      │
│    │  • Cell parsing  │      │  • Priority queue │      │
│    │  • State tracking│      │  • Dependency DAG │      │
│    │  • Rendering     │      │  • Cancellation   │      │
│    └──────────────────┘      └─────────┬─────────┘      │
│                                        │                │
└────────────────────────────────────────┼────────────────┘
                                         │
                    ┌────────────────────▼────────────┐
                    │   Worker Thread Pool (Rust)     │
                    │  ┌──────────┬──────────┬─────┐  │
                    │  │ Worker 1 │ Worker 2 │ ... │  │
                    │  └─────┬────┴─────┬────┴─────┘  │
                    └────────┼──────────┼─────────────┘
                             │          │
                    ┌────────▼──┐  ┌────▼─────┐
                    │  Julia    │  │  Python  │
                    │  Kernel   │  │  Kernel  │
                    └───────────┘  └──────────┘
```

### Component Breakdown

#### 1. Notebook Manager (Steel)
**Location**: `~/.config/helix/notebook-manager.scm`

**Responsibilities**:
- Parse `.ipynb` JSON → internal cell representation
- Track cell state (pending, running, completed, error)
- Manage cell dependencies (for reactive execution)
- Coordinate with Execution Queue
- Render cells and output to buffer

**Key Functions**:
```scheme
(notebook-open path)              ; Load and parse notebook
(notebook-cell-at-cursor)         ; Get current cell
(notebook-execute-cell cell-id)   ; Queue cell for execution
(notebook-cancel-cell cell-id)    ; Cancel running cell
(notebook-render-output cell-id)  ; Update cell output display
```

#### 2. Execution Queue (Steel + Rust FFI)
**Location**: Rust extension in Helix core

**Responsibilities**:
- Priority queue for cell execution
- Dependency resolution (detect cell dependencies)
- Parallel execution of independent cells
- Resource management (limit concurrent kernels)
- Cancellation support

**Steel API**:
```scheme
(queue-add-cell cell-id code lang priority)
(queue-cancel cell-id)
(queue-status)                    ; Get queue state
```

#### 3. Kernel Manager (Rust FFI)
**Location**: Rust extension + Steel wrapper

**Responsibilities**:
- Spawn and manage Julia/Python/R processes
- Jupyter protocol implementation
- Streaming stdout/stderr capture
- Process lifecycle management
- Session persistence

**Steel API**:
```scheme
(kernel-start lang)               ; Start kernel process
(kernel-execute kernel-id code on-output on-complete)
(kernel-interrupt kernel-id)      ; Send SIGINT
(kernel-shutdown kernel-id)       ; Graceful shutdown
(kernel-restart kernel-id)        ; Kill and respawn
```

### Execution Flow

```
User triggers :execute-cell
        │
        ▼
┌───────────────────┐
│ execute-cell      │ (Steel command in helix.scm)
│ • Get cell bounds │
│ • Extract code    │
│ • Detect language │
└─────────┬─────────┘
          │
          ▼
┌───────────────────┐
│ notebook-manager  │
│ • Find existing   │
│   output section  │
│ • Delete old      │
│   output          │
│ • Set status:     │
│   "Running..."    │
└─────────┬─────────┘
          │
          ▼
┌───────────────────┐
│ execution-queue   │ (Rust)
│ • Enqueue cell    │
│ • Check deps      │
│ • Assign to       │
│   worker          │
└─────────┬─────────┘
          │
          ▼
┌───────────────────┐
│ kernel-manager    │ (Rust)
│ • Get/start       │
│   kernel          │
│ • Send code       │
│ • Stream output ──┼──→ Callback to Steel
└───────────────────┘     │
                          ▼
                ┌─────────────────────┐
                │ on-output callback  │ (Steel)
                │ • hx.with-context   │
                │   (append-output    │
                │    line)            │
                └─────────────────────┘
```

### Threading Model

```
Main Thread (Helix Event Loop)
├── Steel Runtime
│   ├── User commands (execute-cell, etc.)
│   ├── Event hooks (document-opened, etc.)
│   └── Callbacks from workers
│
Worker Thread Pool (tokio/rayon)
├── Kernel processes (Julia, Python)
│   ├── stdin writer thread
│   ├── stdout reader thread
│   └── stderr reader thread
│
Background Thread (Steel)
└── Long-running Steel computations
    └── Use spawn-native-thread
```

**Key Principles**:
1. **Never block main thread**: All I/O is async
2. **Use hx.with-context for UI updates**: Schedule on main thread
3. **Parallel cell execution**: Independent cells run concurrently
4. **Streaming by default**: Output appears as it's generated

---

## Implementation Phases

### Phase 1: Foundation (Current - Week 1) ✅

**Status**: 80% Complete

- [x] Notebook parsing (`.ipynb` → readable cells)
- [x] Cell boundary detection
- [x] Cell code extraction
- [x] Basic synchronous execution
- [x] Output insertion
- [x] Output replacement (no stacking)
- [ ] Keybindings (`<space>nr` to execute)
- [ ] Error handling

**Deliverable**: Can execute cells, but editor blocks

### Phase 2: Async Foundation (Week 2)

**Goal**: Non-blocking execution using Steel's current capabilities

**Tasks**:
1. **Implement worker thread pattern**
   ```scheme
   (define (execute-cell-async cell-id code lang)
     (spawn-native-thread
       (lambda ()
         ;; Run on background thread
         (define result (run-julia-sync code))
         ;; Schedule UI update on main thread
         (hx.with-context
           (lambda ()
             (insert-cell-output cell-id result))))))
   ```

2. **Add execution state tracking**
   - Cell states: pending, running, completed, error
   - Visual indicators in buffer (spinner, checkmark, X)

3. **Implement cancellation**
   - Store process PIDs in state
   - `kill -SIGINT <pid>` on cancel command

4. **Stream output via polling**
   - Write to temp file: `julia script.jl > /tmp/output.txt 2>&1 &`
   - Poll file on background thread
   - Stream new lines to main thread via `hx.with-context`

**Deliverable**: Non-blocking execution, real-time output

**Limitations**: Polling is inefficient, no true async process API

### Phase 3: Rust FFI Extensions (Week 3-4)

**Goal**: Implement missing primitives in Helix/Steel

**Work with Steel maintainer (@mattwparas) to add**:

1. **Async Process API**
   - Add to `helix-term/src/commands/engine/steel/mod.rs`
   - Expose via `helix/process.scm` module

2. **Future combinators**
   - Leverage tokio futures
   - Map to Steel futures

3. **Channel API**
   - Use tokio MPSC channels
   - Expose to Steel

**Steel Contributions Needed**:
```rust
// In helix-term/src/commands/engine/steel/process.rs
pub fn run_process_async(
    cmd: String,
    args: Vec<String>,
    on_stdout: SteelVal, // Steel function
    on_stderr: SteelVal,
    on_exit: SteelVal,
) -> Result<SteelVal> {
    // Spawn tokio process
    // Stream stdout/stderr via callbacks
    // Return process handle (future)
}
```

**Deliverable**: True async primitives available in Steel

### Phase 4: Parallel Execution (Week 5)

**Goal**: Execute independent cells concurrently

**Architecture**:
1. **Dependency analysis**
   - Parse cell code for variable definitions/usage
   - Build dependency graph (DAG)

2. **Smart scheduling**
   - Topological sort of DAG
   - Execute all cells at same level in parallel
   - Wait for dependencies before next level

3. **Resource limits**
   - Max concurrent kernels (default: CPU cores)
   - Memory-aware scheduling

**Example**:
```julia
# Cell 1
x = compute_expensive()  # 10 seconds

# Cell 2
y = other_computation()  # 10 seconds

# Cell 3
z = x + y  # Depends on 1 and 2
```

Result: Cells 1 and 2 run in parallel (10s total), then Cell 3 runs (instant)
Traditional: 20+ seconds sequential

**Deliverable**: Marimo-level parallel execution

### Phase 5: Reactive Execution (Week 6)

**Goal**: Pluto.jl/Marimo-style reactivity

**Mechanism**:
1. **Track cell dependencies**
   - Variable definitions: `x = 5` in Cell 1
   - Variable usage: `y = x + 10` in Cell 2

2. **Invalidation on edit**
   - User edits Cell 1
   - Mark Cell 1 as stale
   - Mark all downstream cells (Cell 2) as stale

3. **Auto-execute on save** (optional)
   - When user saves/leaves cell
   - Re-run stale cells in dependency order

**Deliverable**: Live notebook experience

### Phase 6: Rich Output (Week 7-8)

**Goal**: Render plots, tables, LaTeX, HTML

**Approach**:
1. **Detect output type** (MIME types from Jupyter protocol)
2. **Render based on type**:
   - **Text/plain**: Insert as-is
   - **Image/png**: Convert to sixel/kitty graphics protocol
   - **Text/html**: Render with html2text or mdfried
   - **Text/latex**: Render with mdfried or store for later
   - **Application/json**: Pretty-print with syntax highlighting

**Deliverable**: Full Jupyter output compatibility

---

## Steel Shortcomings to Report

### Critical Issues

**Issue #1: No Async Process API**
- **Problem**: `insert-output` is synchronous, blocks editor
- **Needed**: `run-process-async` with streaming callbacks
- **Workaround**: Background threads + polling (inefficient)
- **Priority**: 🔴 Critical

**Issue #2: Undocumented Future API**
- **Problem**: `await-callback` exists but no docs on creating futures
- **Needed**:
  - `(make-future thunk)` - Create future from Steel code
  - `(future-map f fut)` - Transform futures
  - `(future-all futs)` - Combine futures
- **Priority**: 🔴 Critical

**Issue #3: No IPC/Channel Primitives**
- **Problem**: Can't communicate between threads safely
- **Needed**: Channel API (send/recv with callbacks)
- **Workaround**: Shared mutable state (unsafe)
- **Priority**: 🟡 High

### Enhancement Requests

**Enhancement #1: Process Management**
```scheme
(define proc (spawn-process "julia" args))
(process-kill proc)
(process-alive? proc)
(process-wait proc callback)
```

**Enhancement #2: File Watching**
```scheme
(watch-file path callback)  ; Trigger on file change
```

**Enhancement #3: Timer API**
```scheme
(set-timeout ms callback)  ; Single shot
(set-interval ms callback) ; Repeating
```

---

## Technical Decisions

### Format Support Strategy

**Core Format**: `.ipynb` (Jupyter Notebook)
- Industry standard, maximum compatibility
- Rich metadata support (kernelspec, cell metadata, outputs)
- Works with all existing Jupyter tools and kernels

**Extension Points**:
```scheme
;; Plugin architecture for format handlers
(register-notebook-format!
  name: "jupyter"
  extensions: [".ipynb"]
  parser: parse-ipynb
  serializer: serialize-ipynb
  executor: jupyter-kernel-execute)

(register-notebook-format!
  name: "marimo"
  extensions: [".py"]  ; Marimo files are Python files
  parser: parse-marimo
  serializer: serialize-marimo
  executor: marimo-execute)
```

**Planned Formats**:
- ✅ **Jupyter** (`.ipynb`) - Phase 1-6
- 🔄 **Marimo** (`.py`) - Phase 7
- 🔄 **Pluto.jl** (`.jl`) - Phase 8
- 🔄 **Quarto** (`.qmd`) - Phase 9
- 🔄 **Observable** (`.ojs`) - Future

**Kernel Protocol**:
- **Phase 1-3**: Simple stdin/stdout (limited compatibility)
- **Phase 4+**: Full Jupyter protocol via ZMQ (standard Jupyter kernels)
  - Supports any language with a Jupyter kernel
  - ijulia, ipykernel, IRkernel, ijavascript, etc.

### State Management

**Cell State**:
```scheme
(define cell-states (hash))  ; cell-id -> state

(define (cell-state-get id)
  (hash-ref cell-states id))

(define (cell-state-set! id state)
  (hash-set! cell-states id state))

;; State: { status: 'running | 'completed | 'error
;;          output: "...",
;;          error: "...",
;;          pid: 12345 }
```

**Notebook State**:
```scheme
(define notebook-state
  (hash
    'path "/path/to/notebook.ipynb"
    'cells [cell-ids...]
    'kernel-id 123
    'deps (dependency-graph)))
```

### Output Format

**Structured Output Sections**:
```julia
# ─── Code Cell [N] ───
<code>

# ─── Output [Running] ───  ← Status indicator
<streaming output>
# ─── [Completed in 2.3s] ───

# ─── Error ───  ← Only if error
<error message>
# ─────────────
```

### Performance Targets

| Metric | Target | Measurement |
|--------|--------|-------------|
| Notebook open time | < 100ms | 1000 cell file |
| Cell execution latency | < 10ms | Time to start async task |
| Output streaming latency | < 50ms | Time from kernel output to display |
| Max concurrent cells | = CPU cores | Parallel execution |
| Memory overhead | < 10MB | Per notebook |

---

## Success Metrics

### Performance
- ✅ Editor never freezes (0 blocking operations)
- ✅ Output appears within 50ms of generation
- ✅ Can execute 10 cells in parallel
- ✅ < 100ms to open 1000-cell notebook

### Compatibility
- ✅ Full `.ipynb` spec compliance (nbformat v4+)
- ✅ All Jupyter output types render (text, html, png, svg, latex, json)
- ✅ Preserves all notebook metadata on save
- ✅ Works with standard Jupyter kernels (ijulia, ipykernel, IRkernel, etc.)
- ✅ Can open notebooks from VSCode/JupyterLab and save back without data loss

### Developer Experience
- ✅ Vim keybindings for cell navigation
- ✅ No external dependencies (uses system kernels)
- ✅ Works over SSH
- ✅ Plugin is < 1000 LOC Steel + < 500 LOC Rust

---

## Next Steps

### Immediate (This Week)
1. ✅ Document current architecture
2. ⏳ Implement keybindings
3. ⏳ Add error handling
4. ⏳ Test with real Julia notebooks

### Short Term (Next 2 Weeks)
1. Implement async execution with `spawn-native-thread`
2. Add output streaming via polling
3. Create execution state tracking
4. Build cancellation support

### Medium Term (Month 2)
1. Work with @mattwparas on async process API
2. Contribute Rust FFI for process management
3. Implement parallel cell execution
4. Add dependency analysis

### Long Term (Month 3+)
1. Reactive execution (Pluto/Marimo style)
2. Rich output rendering (plots, tables)
3. Full Jupyter protocol support
4. Multi-language kernel support

---

## Questions for Steel Maintainer

1. **Async Process API**: Is there an existing way to spawn async processes with streaming callbacks? If not, would you accept a PR adding this?

2. **Future Creation**: How do we create Steel futures from Rust tokio futures? The `await-callback` function exists but creating futures is undocumented.

3. **Thread Safety**: What's the recommended pattern for thread-safe state sharing? Should we use Rust mutexes exposed to Steel?

4. **Performance**: Any known performance gotchas when spawning many native threads? Any limits?

5. **Debugging**: Best practices for debugging Steel + Rust FFI code?

---

## Conclusion

This architecture positions Helix's notebook plugin to be the **fastest, most responsive Jupyter-compatible notebook environment available**. By leveraging Steel's Rust foundation and implementing true async execution, we can achieve:

- **Sub-50ms UI responsiveness** (better than VSCode/PyCharm)
- **Parallel cell execution** (better than current Jupyter)
- **Terminal-native** (better than all web-based solutions)
- **Full `.ipynb` compatibility** (works with existing Jupyter ecosystem)
- **Extensible architecture** (easy to add Marimo, Pluto.jl, Quarto support)

### Core Principles

1. **Jupyter First**: `.ipynb` is the primary format, full spec compliance
2. **Zero Lock-in**: Standard Jupyter kernels, no proprietary extensions
3. **Extensible by Design**: Plugin system for supporting other notebook formats
4. **Performance**: Async-first architecture, never block the editor
5. **Terminal Native**: Works over SSH, no browser required

The critical path is implementing async primitives in Steel. Once available, the rest of the architecture falls into place naturally.

**Status**: Ready to proceed with Phase 2 (async foundation) using current Steel capabilities while working with maintainer on Phase 3 (FFI extensions).
