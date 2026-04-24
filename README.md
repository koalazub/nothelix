# Nothelix

Jupyter notebooks in the Helix editor.

Nothelix brings Jupyter into Helix. Execute cells, see outputs land in place, and render plots inline — all from the modal editor you already use, without detouring through a browser.

## Install

One line on macOS (Apple Silicon) or x86_64 Linux:

```bash
curl -sSL https://raw.githubusercontent.com/koalazub/nothelix/main/install.sh | sh
```

That downloads a pre-built tarball matching your OS/arch, places the Helix fork binary, the libnothelix dylib, the plugin cogs, and a runtime + demo notebook under `~/.local/bin` and `~/.local/share/nothelix`, and adds `(require "nothelix.scm")` to `~/.config/helix/init.scm` if it's not already there. After it finishes, run:

```bash
nothelix
```

to open the bundled demo notebook. See `nothelix --help` for the full subcommand list (`upgrade`, `uninstall`, `doctor`, `config`, `reset`, `version`).

**Requirements:**

- macOS arm64 or Linux x86_64 (other targets are not yet shipped)
- A Kitty-protocol terminal — Kitty, Ghostty, or WezTerm — for inline plots
- Julia 1.9+ on PATH. If you don't have it, install [juliaup](https://julialang.org/install/) first.

**If something's broken:**

```bash
nothelix doctor
```

runs a set of environment checks and tells you exactly what's wrong. Add `--smoke` to additionally spawn a Julia kernel and verify the full execution pipeline end to end.

**To uninstall:**

```bash
nothelix uninstall
```

Removes every file this install placed. Leaves `~/.julia/`, your existing Helix config, and your own notebooks completely untouched. Use `--purge` to also scrub `~/.cache/helix/helix.log`.

## Why Nothelix?

Jupyter's browser interface is great for exploration but painful if you live in a terminal. Nothelix lets you edit `.ipynb` files at the speed of modal editing, run cells against a real kernel, and view plot outputs inline without leaving Helix.

## Architecture

Nothelix is three moving pieces:

```
              ┌──────────────────────────────────────────┐
              │              Forked Helix                │
              │  (koalazub/helix feature/inline-image-   │
              │   rendering — adds the fork-only APIs)   │
              └──────────────┬───────────────────────────┘
                             │  loads
              ┌──────────────▼───────────────────────────┐
              │        plugin/ (Steel / Scheme)          │
              │   editor commands, keymaps, rendering,   │
              │   document lifecycle hooks               │
              └──────────────┬───────────────────────────┘
                             │  FFI (#%require-dylib)
              ┌──────────────▼───────────────────────────┐
              │     libnothelix (Rust cdylib)            │
              │   notebook parsing, kernel IPC, image    │
              │   encoding, error enrichment, LSP env    │
              └──────────────┬───────────────────────────┘
                             │  spawns + file IPC
              ┌──────────────▼───────────────────────────┐
              │        Julia kernel (one per doc)        │
              │   kernel/*.jl — cell registry,           │
              │   AST analysis, output capture           │
              └──────────────────────────────────────────┘
```

**Rough split.** Rust does everything system-shaped — notebook JSON parsing, kernel process management, image decoding, Kitty protocol encoding, error enrichment, LSP environment bootstrap. Steel does everything editor-shaped — commands, keymaps, picker UI, overlay placement, document-lifecycle hooks. Julia does everything language-shaped — cell execution, dependency tracking, runtime type introspection for error hints.

### Why a fork?

Stock Helix will compile and load nothelix, but three capabilities only exist on [koalazub/helix feature/inline-image-rendering](https://github.com/koalazub/helix/tree/feature/inline-image-rendering):

| Fork addition | What it's for |
|---|---|
| **RawContent API** (`helix-view::document::raw_content`) | Document-attached image payloads. The terminal draws them via Kitty's Unicode-placeholder protocol so images survive scrolling and edits. |
| **Math line annotations** (`Document::math_lines_above/below` + `MathAnnotations` line annotation + decoration) | Virtual rows above/below source lines. Used to stack `\sum`-style limits without touching the underlying text. |
| **Steel FFI surfaces** (`helix.static.set-math-lines-{above,below}!`, `clear-math-lines!`, image-payload FFIs) | Scheme-callable entry points for both of the above. |

When the plugin runs against stock Helix, image rendering and stacked-math limits silently fall back to placeholders — `with-handler`/`eval` guards in the plugin catch the missing FFIs, so nothing crashes. You just lose those two features.

### The kernel protocol

IPC is file-based JSON. One kernel process per open `.ipynb` / `.jl` document, long-lived until you `:kernel-shutdown`:

1. Steel calls `kernel-execute-cell-start` → libnothelix writes `input.json` (cell code + index) into the kernel's scratch dir.
2. Kernel watches the scratch dir, reads `input.json`, runs the code through `@cell` (which registers deps, captures output, catches errors).
3. Kernel writes `output.json` with text/repr output, base64 PNG images, structured error info, and the updated cell registry snapshot.
4. Steel polls `kernel-poll-result`, reads `output.json`, renders outputs in place and fires image registrations through the fork's RawContent API.

Plots ride as base64 PNGs through Julia's `MIME("image/png")` display system — the kernel doesn't know or care about Kitty; it just writes pixels, and libnothelix handles the wire format.

### Error enrichment

Runtime errors are caught by the kernel and dumped as structured JSON (`kernel/output_capture.jl::extract_structured_error`). libnothelix runs them through a pipeline of `Enricher`s (`libnothelix/src/error_format.rs`) that fold source-context, cross-cell context, and kernel-side type hints into the rendered message:

- **`UndefVarError`** — kernel attaches `cell_context` saying where the variable is/would-be defined. If the kernel hasn't indexed it yet, a static `.jl` scanner in Rust catches the case "defined in a later cell you haven't run yet".
- **`MethodError`** — kernel snapshots `typeof(value)` of every binding in `VARIABLE_TYPES` on each successful assignment, plus runs `hasmethod(f, Tuple{typeof(val)})` for each in-scope value to find `method_candidates`. Rust's enricher maps the `::T` signatures to in-scope variables and renders a "variables by type" + "candidates" block.
- **`DimensionMismatch`, `BoundsError`, `ParseError`** — source-line aware enrichers that re-express the error using the user's actual variable names instead of generic `x`/`y` placeholders.

This means an error like `no method matching (Matrix)(::Vector{ComplexF64})` doesn't just echo the type mismatch — it tells you which variable in scope has that stray `Vector{ComplexF64}` and which in-scope value `Matrix()` would have accepted.

### Julia LSP

`lsp/julia-lsp` is a shell wrapper that:

1. Resolves `using LanguageServer` against a nothelix-owned bootstrap env (`~/.local/share/nothelix/lsp/`) — populated once by `ensure-lsp-environment` FFI on plugin first-load.
2. Forces LanguageServer.jl's analysis env to `~/.julia/environments/v#.#` (the user's default). That env accumulates every package the user has ever `Pkg.add`ed, so hover/completion/goto-def work regardless of which notebook directory they're in. No per-notebook `Project.toml` is created; nothing is littered next to notebooks.

## Building from source

If you need to iterate on the fork, libnothelix, or the plugin, skip the curl installer above and follow the manual build steps below.

## Requirements

You'll need four things:

- The [koalazub/helix fork](https://github.com/koalazub/helix/tree/feature/inline-image-rendering), which adds the RawContent API that inline rendering depends on.
- A Rust nightly toolchain to build Helix and libnothelix.
- Julia, since that's currently the only supported kernel.
- A terminal with Kitty graphics protocol support — Kitty, Ghostty, or WezTerm.

**Avoid terminal multiplexers.** Zellij and tmux intercept escape sequences and strip the ones Kitty uses for inline graphics, so plots won't render. Run Helix directly in your terminal, or use a multiplexer that passes the Kitty protocol through untouched.

## Getting Started

This walks through the full setup from scratch: building the Helix fork, building libnothelix, installing the plugin, and running a notebook.

### 1. Clone and build the Helix fork

The fork carries the RawContent API that nothelix relies on for inline images. Both Helix and libnothelix must be built against the same Steel commit so the FFI ABI matches.

```
git clone https://github.com/koalazub/helix.git
cd helix

# Build with Steel plugin support. Grammar builds are slow; run them after.
HELIX_DISABLE_AUTO_GRAMMAR_BUILD=1 cargo build --release --features steel

./target/release/hx --grammar fetch
./target/release/hx --grammar build
```

Put the `hx` binary on your PATH or symlink it into place:

```
ln -sf "$(pwd)/target/release/hx" ~/.local/bin/hx
```

Helix also needs to find its runtime directory. If you're running from a non-system install, point `HELIX_RUNTIME` at the fork's runtime:

```
export HELIX_RUNTIME="/path/to/helix/runtime"
```

### 2. Set STEEL_HOME

Steel needs a home directory for its native dylibs. Add this to your shell profile so it's set every time you launch Helix:

```
export STEEL_HOME="$HOME/.steel"
```

### 3. Clone and install Nothelix

```
git clone https://github.com/koalazub/nothelix.git
cd nothelix
just install
```

That's the entire install. Behind the scenes, `just install` builds libnothelix in release mode, copies and codesigns the dylib into `~/.steel/native/`, symlinks the plugin sources into `~/.steel/cogs/`, drops a small `julia-lsp` wrapper into `~/.local/bin/`, and instantiates the minimal Julia LSP environment under `lsp/`.

Everything except the LSP wrapper lives under `$STEEL_HOME` (default: `~/.steel`). Steel's resolver already searches `$STEEL_HOME/cogs`, so a plain `(require "nothelix.scm")` in your `init.scm` is enough to load the plugin — no extra config wiring.

No `just`? Install it with `cargo install just` or your package manager, or `cat justfile` and run the steps by hand.

> **Why `~/.steel/cogs` instead of `~/.config/helix/nothelix/`?** Earlier versions installed under the Helix config dir, which broke on systems where a config manager (home-manager, stow, chezmoi, a dotfiles script) owns that tree. Per-file symlinks from those managers follow nothelix's whole-directory symlink and clobber the cloned repo with nix-store pointers, eventually producing circular symlinks that fail to load. `$STEEL_HOME/cogs` is Steel's own territory — no config manager touches it, and Steel finds modules there without needing the Helix config dir in the resolver path at all.

> **macOS note:** the dylib must be re-codesigned after every rebuild. When the file on disk changes, macOS invalidates its signature and will SIGKILL the process that tries to load it. Don't symlink the dylib either — codesign stamps the real file, and after a rebuild the symlink target's signature won't match. Copy, then sign. `just install` does this for you.

### 4. Load the plugin

Add this line to `~/.config/helix/init.scm` (create the file if it doesn't exist):

```scheme
(require "nothelix.scm")
```

Steel resolves it against `~/.steel/cogs/nothelix.scm`. If you previously installed nothelix under `~/.config/helix/nothelix*`, delete those files first — Helix searches the config dir before `$STEEL_HOME/cogs`, and a stale copy there will silently shadow the fresh install. `just install` prints a warning when it detects this state.

### 5. Try the example notebook

The repo ships with a demo notebook you can use to verify everything works:

```
hx examples/simple.ipynb
```

Raw `.ipynb` files are JSON and aren't directly editable, so the first thing to do is convert. Run `:convert-notebook` — nothelix writes out a `.jl` companion file and opens it in place. The cells look like this:

```
@cell 0 :julia
using Plots

@cell 1 :julia
x = 1:10
y = x.^2

@markdown 2
# # Results

@cell 3 :julia
plot(x, y)
```

Run `:execute-all-cells` to walk the cells top-to-bottom. The first run is slow — Julia has to precompile `Plots` — but subsequent runs reuse the warm kernel. When execution finishes, outputs appear inline below each cell: cell 1 prints its computed vector, and cell 3 renders the plot directly in the terminal through the Kitty protocol.

To execute a single cell, put your cursor inside it and run `:execute-cell` (or press `<space>nr`). To push your edits back into the original `.ipynb`, run `:sync-to-ipynb`.

### Starting from scratch

You don't need an `.ipynb` to begin with. Nothelix ships a scaffold command and an autofill hook that build up a notebook as you type.

The fastest path from an empty project:

```
:new-notebook maths.jl
```

That creates `maths.jl` with a one-cell template and opens it. From there, everything you need to know is **one keystroke away**:

- **Type `@cell` and press space.** A small popup appears asking whether you want a code cell or a markdown cell. Pick one and the plugin stamps the next available cell index, the file's language, and parks the cursor right where you start typing. You never type a number or `:julia` yourself.
- **Type `@md` (or `@mark`, or `@markdown`) and press space.** Same idea but skips the popup — markdown is unambiguous so the plugin just expands directly.
- **Press `<space>nn`** on an existing notebook to pop the same picker without typing anything. Useful when you're at the bottom of a cell and want to start another one.

Under the hood, typing any `@<word>` followed by a space on an otherwise-blank line opens the picker — so typos and guesses like `@code` or `@c` still give you something instead of silently doing nothing. You never have to remember the exact marker syntax.

When you save the buffer (`:w`), nothelix runs a quiet renumber pass that compacts the cell indices to a contiguous `0, 1, 2, …` sequence. Holes that appeared from deleting or rearranging cells during editing get cleaned up automatically. If you'd rather trigger it yourself, `:renumber-cells` does the same thing on demand.

## Usage

### Commands

| Command | Description |
|---------|-------------|
| `:new-notebook [path]` | Create a new `.jl` notebook with a starter template and open it |
| `:new-cell` | Insert a new cell at the cursor (opens the code/markdown picker) |
| `:renumber-cells` | Renumber `@cell` / `@markdown` markers to a contiguous `0, 1, 2, …` |
| `:convert-notebook` | Convert the raw JSON to the readable cell format |
| `:sync-to-ipynb` | Sync edits in the `.jl` file back to the `.ipynb` |
| `:execute-cell` | Run the code cell under your cursor |
| `:execute-all-cells` | Run every cell top-to-bottom |
| `:execute-cells-above` | Run every cell from the top to the current one |
| `:cancel-cell` | Interrupt a running execution |
| `:next-cell` | Jump to the next cell |
| `:previous-cell` | Jump to the previous cell |
| `:cell-picker` | Open an interactive cell navigator |
| `:select-cell` | Select the entire current cell |
| `:select-cell-code` | Select only the code portion |
| `:select-output` | Select the output section |
| `:kernel-shutdown` | Stop the kernel for the current document |
| `:kernel-shutdown-all` | Stop all running kernels |
| `:graphics-check` | Show which graphics protocol is active |
| `:nothelix-status` | Show full status info |
| `:nothelix-debug-on` / `:nothelix-debug-off` / `:nothelix-debug-toggle` | Toggle debug logging |

### Keybindings

Nothelix adds these bindings for `.ipynb` and `.jl` files:

- `]l` — next cell
- `[l` — previous cell
- `<space>nr` — execute cell
- `<space>nn` — new cell (opens the code/markdown picker)
- `<space>nj` — cell picker (jump to any cell)
- `<space>nc` — select cell
- `<space>ns` — select cell code
- `<space>no` — select output

### Autofill shortcuts

At the start of a line in a notebook file, these expand automatically when you press space:

| You type | You get |
|----------|---------|
| `@cell<space>` | Code / markdown picker, then `@cell N :julia` stamped with the next index |
| `@md<space>`, `@mark<space>`, `@markdown<space>` | `@markdown N` with the cursor parked after `# ` on the next line |
| `@<anything><space>` | Same picker as `@cell`, forgiving of typos like `@code` or `@c` |

## Configuration

Create `~/.config/helix/nothelix.toml` to override defaults:

```toml
[graphics]
# Graphics protocol: "auto", "kitty", "iterm", or "block"
protocol = "auto"
```

Auto-detection inspects your terminal environment and picks the best available protocol. Override it if detection misses your setup.

## Graphics Protocol Support

Nothelix renders plot outputs inline through your terminal's graphics protocol:

| Protocol | Terminals | Quality |
|----------|-----------|---------|
| Kitty | Kitty, Ghostty, WezTerm | Excellent |
| iTerm2 | iTerm2 | Good |

If no graphics protocol is available, plots fall back to text placeholders. Sixel is planned but not yet implemented.

## Architecture

Nothelix splits cleanly into two halves.

**libnothelix** is a Rust cdylib that owns everything performance-sensitive: notebook JSON parsing, Julia kernel process management, image format detection, raster-to-PNG conversion, and Kitty protocol payload generation. Steel loads it at runtime via `#%require-dylib`.

**plugin/** is a collection of Steel modules that glue the Rust pieces into Helix. They handle cell navigation, orchestrate execution, render output, register keybindings, and wire up document lifecycle hooks. Steel handles anything editor-shaped; Rust handles anything system-shaped.

Kernel IPC is file-based. Rust drops an `input.json` containing the cell code, Julia processes it and writes `output.json` with the text and image results, and Steel polls for completion. Plots ride out as base64 PNGs through Julia's `MIME("image/png")` display system.

## LaTeX Rendering

Nothelix renders LaTeX math as Unicode overlays directly in the buffer. Inside `$...$` regions, `\alpha` becomes `α`, `\leq` becomes `≤`, `\begin{cases}` becomes `⎧⎨⎩`, and so on. This works in converted notebooks (`.jl` files) and in any file type where Helix finds math regions.

The pipeline is straightforward:

```
Helix buffer → conceal.scm finds $...$ regions
            → Rust latex_overlays() maps each region to (offset, replacement) pairs
            → Helix overlays replace source characters with Unicode glyphs
```

### Unicode overlay engine (`libnothelix/src/unicode.rs`)

The overlay engine is a byte-offset scanner that walks the text inside each `$...$` region and produces JSON overlay pairs:

| Construct | Rendering |
|---|---|
| `\alpha`, `\beta`, ... (2544 entries) | Unicode symbol lookup (Julia-compatible table) |
| `\mathbf{x}`, `\mathcal{L}`, ... | Unicode math bold/italic/script variants |
| `^{2}`, `^n` | Superscript digits and letters |
| `_{n}`, `_0`, ... | Subscript digits and letters |
| `\begin{cases}...\end{cases}` | `⎧` `⎨` `⎩` brace fences |
| `\begin{pmatrix}...\end{pmatrix}` | `⎛` `⎜` `⎞` paren fences |
| `\begin{bmatrix}...\end{bmatrix}` | `⎡` `⎢` `⎤` bracket fences |
| `\begin{vmatrix}...\end{vmatrix}` | `│` bar fences |
| `\begin{aligned}`, `\begin{matrix}`, ... | delimiters hidden, content preserved |
| `\text{...}`, `\mathrm{...}`, `\operatorname{...}` | content preserved, command hidden |
| `\|...\|` | `‖` (norm delimiters) |
| `\\` in environments | hidden, fence char emitted |
| `&` in environments | replaced with space |

All overlay generation goes through a hand-rolled byte-offset scanner that walks source text in a single O(n) pass with no pathological cases.

## Current Limitations

- Julia is the only supported kernel.
- Inline rendering needs the [koalazub/helix fork](https://github.com/koalazub/helix/tree/feature/inline-image-rendering); stock Helix won't work.
- Terminal multiplexers (Zellij, tmux) strip Kitty graphics escape sequences.
- Sixel encoding isn't implemented yet — unsupported terminals fall back to text placeholders.
- Python kernel support is planned but not yet available.

## Development

### With the Nix flake (recommended)

```
nix develop
```

This drops you into a shell with every dependency pinned — Rust nightly, Julia, tree-sitter, and friends — and prints the available recipes. Project tasks are driven by a justfile:

| Recipe | Description |
|--------|-------------|
| `just install` | Build, install, and codesign the dylib |
| `just install debug` | Same as above but with the debug profile |
| `just build` | Build without installing |
| `just test` | Run libnothelix tests |
| `just uninstall` | Remove the installed dylib |

Run `just install` and restart Helix after any Rust change.

### Without Nix

```
cargo build --release -p libnothelix
```

Then follow the manual install steps from the Getting Started section above, and remember to codesign on macOS after every rebuild.

### Running tests

Rust tests:

```
just test
```

From inside Helix, you can also run the plugin's Steel test suites:

- `:run-all-tests` — every test suite
- `:run-cell-tests` — cell extraction tests
- `:run-kernel-tests` — kernel persistence tests
- `:run-execution-tests` — execution flow tests

### Debugging

Nothelix ships a debug mode that's off by default. When something misbehaves, flip it on from Helix's command mode, reproduce the problem, read the log, and flip it off again.

| Command | Effect |
|---------|--------|
| `:nothelix-debug-on` | Start emitting debug lines |
| `:nothelix-debug-off` | Stop emitting debug lines |
| `:nothelix-debug-toggle` | Flip the current state |

Each command also updates the status line so you always know which mode you're in.

Debug lines land in the same file as the rest of Helix's logging:

```
~/.cache/helix/helix.log
```

Helix's default log level is `warn`, which swallows the `info`-level lines nothelix writes. Launch `hx` with `-v` to let info-level lines through, or `-vv` for everything:

```
hx -vv path/to/notebook.ipynb
```

Every nothelix debug line is prefixed with `nothelix:` so you can filter cleanly:

```
grep 'nothelix:' ~/.cache/helix/helix.log
```

The most useful categories to watch for:

- **`execution.update-cell-output: register image cell=…`** — fires every time a freshly-executed cell registers an inline plot with Helix's RawContent layer. Includes the cell index, image id, anchor line, char index, and payload/row byte counts. If you see this line but no plot appears, the break is downstream of nothelix (Helix's renderer or the terminal itself). If you don't see it at all, the plugin short-circuited before reaching the register step — scan upward for stdout or cache errors.

- **`execution.render-cached-images: REGISTER cell=…`** and **`SKIP cell=… reason=…`** — emitted once per `# @image` marker when a document is opened. A `SKIP` reason tells you exactly why a cached plot didn't come back on reopen: missing cache file, unparseable cell index, oversized grid, and so on.

- **`execution.render-cached-images: entry path=…`** / **`done … registered=N`** — bracket each document-opened pass so you can confirm the hook actually fired for the buffer you expected.

For image bugs specifically, there's a scripted repro harness:

```
nu scripts/capture-image-log.nu path/to/notebook.ipynb
```

The script truncates the log before launch, starts `hx -vv`, and prints a summary of the raw_content lifecycle when you `:q!` out. Toggle debug mode on inside the session, reproduce the bug, quit, and the tail includes every debug line nothelix wrote — usually enough to decide whether the problem is in the plugin, the Helix fork, or the terminal.

Remember to turn debug mode back off (`:nothelix-debug-off`) when you're done. Otherwise every subsequent cell execution keeps writing to the log.

### Generating documentation

```
cargo doc --open -p libnothelix
```

## Licence

MIT
