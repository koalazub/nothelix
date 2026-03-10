# Nothelix

Jupyter notebooks in the Helix editor.

Nothelix brings interactive notebook workflows to Helix, letting you execute code cells, view outputs, and render inline images without leaving your terminal. It's built for developers and researchers who prefer modal editing but need the exploratory capabilities of notebooks.

## Why Nothelix?

Jupyter notebooks are powerful for data exploration, but the browser-based interface doesn't suit everyone. If you've ever wanted to edit `.ipynb` files with the speed of Helix's modal editing, or run notebook cells from a proper text editor, Nothelix bridges that gap.

The plugin converts notebooks into a readable cell format, manages kernel connections for code execution, and handles inline image rendering for plot outputs. All the heavy lifting happens in a Rust library, while Steel (Helix's embedded Scheme) handles the editor integration.

## Requirements

- Helix built with Steel plugin support (from the [koalazub/helix fork](https://github.com/koalazub/helix/tree/feature/inline-image-rendering), which includes the RawContent API for inline rendering)
- A Julia installation (for kernel execution)
- A terminal with graphics support (Kitty, Ghostty, WezTerm, iTerm2) for inline images

## Installation

### Using devenv (recommended)

If you have devenv set up:

```bash
git clone https://github.com/koalazub/nothelix
cd nothelix
devenv shell
nothelix-install
```

### Manual installation

Build the library:

```bash
cargo build --release -p libnothelix
```

Copy the library to Steel's native directory:

```bash
mkdir -p ~/.steel/native
cp target/release/libnothelix.dylib ~/.steel/native/  # macOS
cp target/release/libnothelix.so ~/.steel/native/     # Linux
```

Copy the plugin files to your Helix config:

```bash
cp plugin/nothelix.scm ~/.config/helix/
cp -r plugin/nothelix ~/.config/helix/nothelix
```

Add to your `~/.config/helix/init.scm`:

```scheme
(require "nothelix.scm")
```

## Usage

Open any `.ipynb` file in Helix. The plugin provides these commands:

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

**libnothelix** is a Rust library that handles performance-critical operations: parsing notebook JSON, detecting image formats, converting raster images to PNG, and generating Kitty terminal escape sequences for inline images.

**nothelix.scm** is a Steel plugin that integrates with Helix. It manages kernel processes, handles cell navigation, and orchestrates the rendering pipeline. Steel handles the editor-side logic while Rust does the heavy computation.

This split keeps the plugin responsive. Notebook parsing and image conversion happen in compiled Rust code, while the Scheme layer stays focused on editor integration.

## Current Limitations

- Only Julia kernels are supported currently
- The RawContent API for true inline rendering requires additional Helix patches (available in the [koalazub/helix fork](https://github.com/koalazub/helix/tree/feature/inline-image-rendering))
- Sixel encoding is not yet implemented (falls back to text placeholders)
- Python kernel support is planned but not yet available

## Development

With devenv:

```bash
devenv shell
nothelix-build      # build the library
nothelix-install    # build and install to ~/.steel and ~/.config/helix
nothelix-uninstall  # remove installed files
```

Or with the Nix flake directly:

```bash
nix develop
nothelix-build
nothelix-install
```

Generate documentation:

```bash
cargo doc --open -p libnothelix
```

## Licence

MIT
