# Nothelix

Jupyter notebooks in the Helix editor.

Nothelix brings Jupyter into Helix. Execute cells, see outputs land in place, and render plots inline — all from the modal editor you already use, without detouring through a browser.

## Why Nothelix?

Jupyter's browser interface is great for exploration but painful if you live in a terminal. Nothelix lets you edit `.ipynb` files at the speed of modal editing, run cells against a real kernel, and view plot outputs inline without leaving Helix.

Under the hood, a Rust library handles everything performance-sensitive — notebook parsing, kernel IPC, image decoding, Kitty protocol encoding — while Steel, Helix's embedded Scheme, drives the editor integration.

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
