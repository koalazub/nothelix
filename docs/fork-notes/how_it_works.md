Hey! I've been thinking about what "notebook-like" workflows could look like in Helix, and I decided to create a proof of concept to demo how utilising the [plugin system being worked on](https://github.com/helix-editor/helix/pull/8675) and the amendments to the feature-set in Helix are needed to demo it. And just to be very clear it's just an experiment and a concept that might help get other developers who are more intimate with the codebase to start thinking about what structures are required to potentially make something like this a reality. 

This started as two related questions: (1) whether Helix could support notebook-style interaction without adopting the full `.ipynb` model, and (2) what it would actually take to get inline rendering (specifically images/plots) to work inside the editor UI. I know the general direction has been "this should be leveraged via the plugin system once that lands", and I agree with that in principle, but I wanted to test where the boundary really is between "a plugin can do it" and "the editor itself must grow new surface area to make it possible".

I wanted to address the .ipynb part because of how bloated it really is. To the point where Helix would stall and requiring me to close the panel and relaunching. With JSON blobs stuffed with cells, metadata, and base64-encoded outputs all serialised together. Opening large notebooks through the Steel plugin layer would freeze the UI for seconds at a time, since the entire file had to be parsed synchronously on the main thread. The angle I took was to sidestep the format entirely, following Marimo's lead: instead of treating notebooks as opaque JSON, treat them as decorated source files where cell boundaries are marked explicitly in the code itself.

In a traditional notebook, you run a cell and its variables persist. you don't re-import libraries or re-declare data structures every time. That behaviour comes from evaluating code at the module's top level, so I needed the kernel to do the same. I needed it to execute each cell in Julia's Main module using include_string, ensuring that definitions accumulate across cells exactly as they would in a REPL session.

The payoff is that I can edit notebooks as plain text in the terminal without wading through escaped JSON, and the editor doesn't burn cycles re-parsing the same bloated structure on every interaction. The compute cost shifts from "parse everything, always" to "parse once in the background, load cells on demand."

## Notebook Conversion: .ipynb to .jl with Decorators

The insight here is that `.ipynb` files are JSON blobs with cells, metadata, and outputs all serialised together. They're huge, they're slow to parse, and they're hostile to version control. Marimo showed that you can sidestep this by treating notebooks as decorated source files instead.

My conversion takes a `.ipynb` and emits a plain `.jl` file with cell markers:

```julia
# Nothelix Notebook - Runnable Julia Script
# Source: wavelength_analysis.ipynb

macro cell(idx, exec_count) end
macro markdown(idx) end

# ═══════════════════════════════════════════════════════════════════
@cell 0 nothing
using Plots

# ═══════════════════════════════════════════════════════════════════
@markdown 1
#=
# Welcome to Nothelix

This is a markdown cell wrapped in Julia block comments.
=#

# ═══════════════════════════════════════════════════════════════════
@cell 2 42
x = 1:10
y = x.^2
plot(x, y)
```

The `@cell` and `@markdown` macros are defined as no-ops at the top of the file, so the `.jl` can run standalone with `julia my_notebook.jl`. The first argument is the cell index, the second is the execution count from the original notebook (or `nothing` if unexecuted). Markdown cells are wrapped in `#= ... =#` block comments so they're valid Julia syntax.

The conversion logic lives in Rust (`libnothelix/src/notebook.rs`). The `Notebook::to_jl()` method walks the cells array and emits the decorated format. Going the other direction, `sync_from_jl()` parses the `.jl` looking for `@cell` and `@markdown` markers, extracts the updated source, and writes back to the original `.ipynb` JSON. This means you edit a readable text file in Helix, then sync changes back when you need the notebook for Jupyter or collaborators.

The Rust implementation handles the edge cases: multi-line sources stored as arrays in the JSON, raw cells that become comments, trailing newlines, and so on. Steel calls into this via FFI:

```scheme
(#%require-dylib "libnothelix"
                 (only-in nothelix
                          notebook-convert-sync
                          convert-to-ipynb
                          notebook-cell-count
                          get-cell-at-line))
```

The `:convert-notebook` command loads the `.ipynb`, converts it, and opens the resulting `.jl` in a new buffer. The `:sync-to-ipynb` command reads the current `.jl` buffer and updates the source notebook in place.

## Inline Image Rendering (PoC)

On the rendering side, the main thing I explored was: "If a plugin (or external tool) can produce image data, how does that data make its way into the editor in a way that's renderable and doesn't destroy the editing experience?"

So for this part I prototyped an abstract interface for inline images and then modified Helix to accept data emitted via terminal image protocols (e.g. Kitty graphics and Sixel). From there, the hard part wasn't just "sending escape sequences", but representing those rendered results in the editor model: I needed a way to render graphics inline while keeping the text buffer coherent (cursor motion, selections, layout, redraws, etc.). That pushed me toward making targeted changes in how the buffer/view layer accounts for non-text renderables. I was trying to take into consideration the forethought that everyone who contributes/merges PRs into Helix operate, which was why I went all in on a more abstract consideration rather than just a first pass by only using a single protocol such as Kitty Graphics Protocol. 

### The RawContent Abstraction

The core primitive I added is `RawContent` in `helix-core/src/text_annotations.rs`:

```rust
pub struct RawContent {
    pub id: u64,              // Unique identifier for O(1) diffing
    pub payload: Arc<Vec<u8>>, // Raw bytes (escape sequences)
    pub height: u16,          // Terminal rows consumed
    pub char_idx: usize,      // Document position
    pub width: Option<u16>,   // Columns (for placeholder rendering)
    pub placeholder_rows: Option<Arc<Vec<String>>>, // Unicode placeholders
}
```

The design follows a "dumb core, smart plugins" principle. The core provides mechanism: write bytes to the terminal, reserve vertical space. Plugins provide policy: which protocol to use, how to encode images, what format to request from the kernel.

A few deliberate choices here:

1. `Arc<Vec<u8>>` for the payload means cloning during layout calculations costs so that copying is reduced. Images don't change once rendered, so the Arc makes sense.

2. ID-based equality (`PartialEq` compares `id` and `char_idx`, not `payload`) means the render loop can diff efficiently. I was trying to consider payload management like when dealing with 500KB PNGs for example.

3. The `height` field tells the document formatter how many visual lines to skip. This keeps scrolling, cursor motion, and soft-wrap calculations coherent with the image's presence.

4. Optional `placeholder_rows` supports Kitty's Unicode placeholder mode, where the terminal replaces special Unicode characters with image tiles. This is more reliable for scrolling than direct image placement.

### Wiring It Through the Stack

Getting raw bytes from a plugin to the terminal required changes in several places:

**DocumentFormatter** (`helix-core/src/doc_formatter.rs`): When iterating graphemes, the formatter checks `TextAnnotations::raw_content_at()` for each character position. If there's a `RawContent` at that index, it attaches it to the `FormattedGrapheme` and advances the visual row counter by the content's height.

**Render Loop** (`helix-term/src/ui/document.rs`): The `render_text` function checks for `raw_content` on each grapheme. When present, it calls `TextRenderer::draw_raw_content()` instead of rendering the character normally. This writes the raw bytes via the surface.

**Buffer** (`helix-tui/src/buffer.rs`): Added a `raw_writes: Vec<(u64, u16, u16, Vec<u8>)>` field to collect raw byte writes with their screen positions and image IDs. The `write_raw_bytes()` method appends to this list.

**Terminal** (`helix-tui/src/terminal.rs`): The `Terminal::flush()` method handles raw_writes after normal cell diffing. It compares image IDs between frames, deletes moved/removed images, and draws new ones. This happens outside the normal cell-based rendering.

The total diff for the rendering pipeline is about 200 lines across these files. Each layer has a single point where it checks for raw content and handles it specially.

### Steel Bindings

Plugins access this via `add-raw-content!`:

```scheme
(define (render-plot-output image-b64 image-id image-rows)
  (define escape-seq (kitty-display-image-bytes image-b64 image-id image-rows))
  (when (not (string-starts-with? escape-seq "ERROR:"))
    (define char-idx (cursor-position))
    (helix.static.add-raw-content! escape-seq image-id image-rows char-idx)))
```

The escape sequence generation happens in Rust (`libnothelix/src/graphics.rs`), which handles Kitty protocol encoding, chunked transmission for large images, and format detection. Steel just orchestrates when and where to render.

## Kernel Management

For actually executing code, I built a kernel manager that spawns a Julia process and communicates via file-based IPC. The kernel lives in `/tmp/helix-kernel-{id}/` with files for input commands, output results, and lifecycle markers.

The kernel runner (`nothelix/kernel/runner.jl`) is a Julia script that:
1. Writes a `ready` marker when initialisation completes
2. Watches for `input.json` containing execution requests
3. Evaluates the code in a persistent module (so variables survive across cells)
4. Writes results to `output.json` with stdout, stderr, and any display outputs
5. Writes `output.json.done` as a completion marker

The Rust side (`libnothelix/src/lib.rs`) provides FFI functions for starting kernels, sending commands, and polling for results. Polling is non-blocking: `kernel-poll-result` returns immediately with either `{"status": "pending"}` or the actual result.

Steel uses this for async execution:

```scheme
(define (poll-for-result kernel-dir)
  (define result-json (kernel-poll-result kernel-dir))
  (define status (json-get result-json "status"))
  
  (cond
    [(equal? status "pending")
     ;; Still running - update spinner and poll again
     (update-spinner-frame)
     (enqueue-thread-local-callback-with-delay 100
       (lambda () (poll-for-result kernel-dir)))]
    [else
     ;; Done - update UI with result
     (update-cell-output result-json)]))
```

The spinner updates in the document while execution runs, giving visual feedback without blocking the editor. When results arrive, `update-cell-output` inserts the output text and, if there's an image, calls `add-raw-content!` to render it inline.

## Plugin Boundary + Performance Notes

On the plugin side, I ran into performance constraints early. Even though the plugin work is leaning on Steel, I still needed to rely on Rust for the hot paths to keep things responsive (both for conversion and for handling/rendering-related work). So part of this experiment was also about discovering what the plugin system makes elegant, and what still feels like it needs first-class hooks in Helix itself.

Specifically:
- **JSON parsing and notebook manipulation**: Rust. serde_json is fast; doing this in Steel was noticeably slow for large notebooks and I kinda figured I may as well keep the heavier computation stuff on the rust side and get Steel to be more of a caller instead.
- **Image encoding and protocol handling**: Rust. Base64 encoding, chunked Kitty sequences, format detection.
- **Navigation and cell detection**: Rust. Doing it in Steel with rope operations was sluggish.
- **UI orchestration, keybindings, status messages**: Steel. This is where the plugin layer shines, it was much easier to iterate on UX without recompiling.

The split feels natural: Rust handles data transformation and terminal protocol details, Steel handles editor integration and user interaction.

## What Required Editor Changes

Some things couldn't be done purely from the plugin:

1. **RawContent annotation layer**: The core had to grow awareness of non-text content that consumes visual space. This is the ~85 LOC `RawContent` struct and the integration with `TextAnnotations`.

2. **Raw byte writes in the buffer**: The TUI layer needed a path for bytes that bypass cell-based rendering. This is the `raw_writes` field and `write_raw_bytes()` method.

3. **Terminal flush for raw content**: The Terminal layer needed to diff and flush raw writes after normal content, handling image ID tracking and deletion of stale images. About 50 lines.

4. **Document formatter height tracking**: The formatter needed to know that some annotations consume vertical space. About 15 lines.

These are all minimal, targeted changes. The total is around 150 lines of Rust in helix-core and helix-tui, plus about 100 lines in helix-term for the rendering integration. The design is intentionally "dumb": the core doesn't know about image formats, terminal protocols, or notebooks. It just knows that plugins can emit raw bytes at document positions, and those bytes consume some number of visual lines.

It's also good to note that when it comes to plugin service development, I'm not sure where the line should be drawn for the plugin to interact. So for this case I just went in from the Helix side and relied on Steel's current(at the time of developing this concept) architecture and capabilities. 

## Why I'm Posting

I want to stress that this is a thought experiment and a learning exercise more than a feature proposal. I mainly wanted to understand whether notebook-style workflows are feasible in Helix, how powerful plugins can be, and where they fall flat (i.e., where editor-level changes are unavoidable). I'd love feedback on whether this is better as a draft PR, a design discussion, or just a "here's a weird prototype what does it imply by way of capabilities?"

I really want notebook capabilities before I begin my disseration year this year and took it upon myself to work on this concept with the hopes that something could be built out by March. I'm pretty over jumping between IDEs for larger mathematical and scientific computation. And this was my attempt at hopefully getting people to have a larger discussion on how to get this over the finish line.

I would expect that there's a lot of toes I've stepped on when it comes to the alterations made in Helix, and potentially the overall plugin structure in the Steel plugin. But again, this is about discussing it to see what can be taken from this with the hopes of my goal being achieved. 

Here's everything, including a recording and the code/branches:

- https://github.com/koalazub/nothelix
- https://github.com/koalazub/helix/tree/feature/inline-image-rendering


https://github.com/user-attachments/assets/553907d9-544b-4c18-a156-e3ce2eaa1dce

