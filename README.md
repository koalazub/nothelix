# Nothelix

Jupyter notebooks in the Helix editor.

Nothelix brings interactive notebook workflows to Helix, letting you execute code cells, view outputs, and render inline images without leaving your terminal. It's built for developers and researchers who prefer modal editing but need the exploratory capabilities of notebooks.

## Why Nothelix?

Jupyter notebooks are powerful for data exploration, but the browser-based interface doesn't suit everyone. If you've ever wanted to edit `.ipynb` files with the speed of Helix's modal editing, or run notebook cells from a proper text editor, Nothelix bridges that gap.

The plugin converts notebooks into a readable cell format, manages kernel connections for code execution, and handles inline image rendering for plot outputs. All the heavy lifting happens in a Rust library, while Steel (Helix's embedded Scheme) handles the editor integration.

## Requirements

- **Helix with Steel plugin support** -- from the [koalazub/helix fork](https://github.com/koalazub/helix/tree/feature/inline-image-rendering), which includes the RawContent API for inline rendering
- **Rust nightly toolchain** -- for building both Helix and libnothelix
- **Julia** -- for kernel execution (only Julia kernels are supported currently)
- **A terminal with Kitty graphics protocol support** -- for inline images (Kitty, Ghostty, WezTerm)

### Terminal multiplexer caveat

Terminal multiplexers like **Zellij** and **tmux** intercept escape sequences and will prevent inline images from rendering. Run Helix directly in your terminal for image support, or use a multiplexer that passes through Kitty graphics protocol escapes.

## Getting Started

This walks through the full setup from scratch: building the Helix fork, building libnothelix, installing the plugin, and running a notebook.

### 1. Clone and build the Helix fork

The Helix fork adds the RawContent API that Nothelix needs for inline image rendering. Both Helix and libnothelix must be built against the same Steel commit for ABI compatibility.

```
git clone https://github.com/koalazub/helix.git
cd helix

# Build with Steel plugin support (skip grammar builds -- they take a while)
HELIX_DISABLE_AUTO_GRAMMAR_BUILD=1 cargo build --release --features steel

# Fetch tree-sitter grammars separately
./target/release/hx --grammar fetch
./target/release/hx --grammar build
```

Put the `hx` binary on your PATH or symlink it:

```
ln -sf "$(pwd)/target/release/hx" ~/.local/bin/hx
```

The Helix runtime directory must also be findable. Set `HELIX_RUNTIME` if you don't install Helix system-wide:

```
export HELIX_RUNTIME="/path/to/helix/runtime"
```

### 2. Set STEEL_HOME

Steel needs a home directory for native dylibs. Set this in your shell profile:

```
export STEEL_HOME="$HOME/.steel"
```

Make sure this is set whenever you run `hx`.

### 3. Clone and build Nothelix

```
git clone https://github.com/koalazub/nothelix.git
cd nothelix
cargo build --release -p libnothelix
```

### 4. Install the dylib and plugin files

The library goes into Steel's native directory. The plugin files go into your Helix config.

**Option A: Symlinks (recommended for development)**

Symlinks let you rebuild without re-copying. This is what `nothelix-install` does in the Nix dev shell.

```
mkdir -p ~/.steel/native
mkdir -p ~/.config/helix

# Dylib
ln -sf "$(pwd)/target/release/libnothelix.dylib" ~/.steel/native/   # macOS
ln -sf "$(pwd)/target/release/libnothelix.so" ~/.steel/native/      # Linux

# Plugin files
ln -sf "$(pwd)/plugin/nothelix.scm" ~/.config/helix/nothelix.scm
ln -sf "$(pwd)/plugin/nothelix" ~/.config/helix/nothelix
```

**Option B: Copy**

```
mkdir -p ~/.steel/native
mkdir -p ~/.config/helix

# Dylib
cp target/release/libnothelix.dylib ~/.steel/native/   # macOS
cp target/release/libnothelix.so ~/.steel/native/       # Linux

# Plugin files
cp plugin/nothelix.scm ~/.config/helix/
cp -r plugin/nothelix ~/.config/helix/nothelix
```

### 5. Load the plugin

Add this to `~/.config/helix/init.scm` (create the file if it doesn't exist):

```scheme
(require "nothelix.scm")
```

### 6. Try the example notebook

There's a demo notebook in the repo you can use to verify everything works:

```
hx examples/simple.ipynb
```

The raw `.ipynb` is JSON. You need to convert it to the editable cell format first:

1. Run `:convert-notebook` -- this creates a `.jl` file and opens it. You'll see cells like:

```
@cell 0 julia
using Plots

@cell 1 julia
x = 1:10
y = x.^2

@markdown 2
# # Results

@cell 3 julia
plot(x, y)
```

2. Run `:execute-all-cells` to execute every cell top-to-bottom. The first run takes a while because Julia needs to precompile `Plots`.

3. Once execution finishes, outputs appear inline below each cell. Cell 1 shows the computed vector, and cell 3 renders the plot as an image directly in the terminal.

You can also execute individual cells with `:execute-cell` (or `<space>nr`) while your cursor is inside a cell.

To save edits back to the original `.ipynb`, run `:sync-to-ipynb`.

## Usage

### Commands

| Command | Description |
|---------|-------------|
| `:convert-notebook` | Convert the raw JSON to readable cell format |
| `:sync-to-ipynb` | Sync edits in the `.jl` file back to the `.ipynb` |
| `:execute-cell` | Run the code cell under your cursor |
| `:execute-all-cells` | Run all cells top-to-bottom |
| `:execute-cells-above` | Run all cells from the top to the current cell |
| `:cancel-cell` | Interrupt a running execution |
| `:next-cell` | Jump to the next cell |
| `:previous-cell` | Jump to the previous cell |
| `:cell-picker` | Open an interactive cell navigator |
| `:select-cell` | Select the entire current cell |
| `:select-cell-code` | Select just the code portion |
| `:select-output` | Select the output section |
| `:kernel-shutdown` | Stop the kernel for the current document |
| `:kernel-shutdown-all` | Stop all running kernels |
| `:graphics-check` | Show which graphics protocol is active |
| `:nothelix-status` | Show full status info |

### Keybindings

The plugin adds these bindings for `.ipynb` and `.jl` files:

- `]l` -- next cell
- `[l` -- previous cell
- `<space>nr` -- execute cell
- `<space>nj` -- cell picker
- `<space>nc` -- select cell
- `<space>ns` -- select cell code
- `<space>no` -- select output

## Configuration

Create `~/.config/helix/nothelix.toml` to override defaults:

```toml
[graphics]
# Graphics protocol: "auto", "kitty", "iterm", or "block"
protocol = "auto"
```

The auto-detection checks your terminal environment and selects the best available protocol. Override this if detection doesn't work for your setup.

## Graphics Protocol Support

Nothelix renders plot outputs inline using your terminal's graphics protocol:

| Protocol | Terminals | Quality |
|----------|-----------|---------|
| Kitty | Kitty, Ghostty, WezTerm | Excellent |
| iTerm2 | iTerm2 | Good |

If no graphics protocol is available, plot outputs display as text placeholders. Sixel support is planned but not yet implemented.

## Architecture

The project has two parts:

**libnothelix** is a Rust library (compiled as a cdylib) that handles performance-critical operations: parsing notebook JSON, managing Julia kernel processes via IPC, detecting image formats, converting raster images to PNG, and generating Kitty terminal escape sequences for inline images. It's loaded by Steel at runtime via `#%require-dylib`.

**plugin/** is a set of Steel (Scheme) modules that integrate with Helix. They manage cell navigation, orchestrate execution, handle output rendering, and register keybindings. Steel handles the editor-side logic while Rust does the heavy computation and system-level work.

The kernel IPC protocol is file-based: Rust writes `input.json` with the cell code, Julia processes it and writes `output.json` (with text and image data), and Steel polls for results. Plot images are captured as PNG via Julia's `MIME("image/png")` display system.

## Current Limitations

- Only Julia kernels are supported currently
- The RawContent API for inline rendering requires the [koalazub/helix fork](https://github.com/koalazub/helix/tree/feature/inline-image-rendering)
- Terminal multiplexers (Zellij, tmux) block Kitty graphics protocol escape sequences
- Sixel encoding is not yet implemented (falls back to text placeholders)
- Python kernel support is planned but not yet available

## Development

### With Nix flake (recommended)

```
nix develop
nothelix-build      # build the library
nothelix-install    # build, symlink to ~/.steel and ~/.config/helix
nothelix-uninstall  # remove installed files
```

### Without Nix

```
cargo build --release -p libnothelix
```

Then follow the manual install steps from the Getting Started section above.

### Running tests

From within Helix, you can run the plugin's Steel tests:

- `:run-all-tests` -- run all test suites
- `:run-cell-tests` -- cell extraction tests
- `:run-kernel-tests` -- kernel persistence tests
- `:run-execution-tests` -- execution flow tests

### Generating documentation

```
cargo doc --open -p libnothelix
```

## Licence

MIT
