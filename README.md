# Nothelix

Interactive Jupyter notebook support for Helix editor with persistent kernel execution, vim-style navigation, and full syntax highlighting.

## Overview

Nothelix brings the power of Jupyter notebooks to Helix, combining:
- **Tree-sitter grammar** for syntax highlighting and cell parsing
- **Steel plugin** for interactive cell execution with persistent kernels
- **vim-style navigation** for seamless notebook editing

Execute code cells and see output inline, similar to VSCode or JupyterLab, but entirely within your terminal.

## Features

### Interactive Execution
- Execute code cells with live output capture
- Persistent REPL kernel maintains state across executions
- Output sections update in-place without stacking
- Visual cell picker with live preview

### Syntax Highlighting
- Multi-language injection (Julia, Python, Markdown)
- Code cells get full language-specific highlighting
- Output sections styled distinctly
- Cell boundaries clearly marked

### Navigation
- Jump between cells with vim-style keybindings (`[l`, `]l`)
- Visual picker to jump to any cell (`<space>nj`)
- Text objects for cell selection (`ac`, `ic`)
- All keybindings scoped to `.ipynb` files only

## Installation

### Prerequisites

- Helix editor (v25.07+)
- Julia installed and in PATH
- Steel scripting support (built into Helix)

### Step 1: Install Tree-sitter Grammar

Add to `~/.config/helix/languages.toml`:

```toml
[[language]]
name = "notebook"
scope = "source.notebook"
injection-regex = "notebook|jupyter|ipynb"
file-types = ["ipynb"]
comment-token = "#"
indent = { tab-width = 4, unit = "    " }

[[grammar]]
name = "notebook"
source = { git = "https://github.com/your-org/nothelix", rev = "main" }
```

Then run:
```bash
hx --grammar fetch
hx --grammar build
```

### Step 2: Install Steel Plugin

Copy the plugin files to your Helix config directory:

```bash
cp plugin/helix.scm ~/.config/helix/
cp plugin/kernel-manager.scm ~/.config/helix/
```

**Optional:** Auto-convert raw .ipynb JSON files to cell format on open:

```bash
cp plugin/init.scm ~/.config/helix/
```

Restart Helix and you're done!

## Usage

### Keybindings

All keybindings are scoped to `.ipynb` files only:

- `,r` - Execute (run) current cell
- `,j` - Open cell picker (jump to cell)
- `]l` - Jump to next cell
- `[l` - Jump to previous cell

### Cell Picker

Press `,j` to open an interactive cell picker:

- `j` / `k` - Navigate up/down
- `1-9` - Jump directly to cell number
- `Enter` - Jump to selected cell
- `ESC` or `q` - Close picker

The picker shows a live preview of each cell's content as you navigate.

### Executing Cells

1. Position cursor anywhere in a code cell
2. Press `,r`
3. Editor will block while kernel executes (shows "⚙ Executing cell..." status)
4. Output appears below the cell when complete

The kernel stays alive between executions, so variables persist across cells.

## Notebook Format

Nothelix works with Jupyter notebooks converted to a text-based cell format:

```julia
# ─── Code Cell [1] ───
x = 10
y = x + 5

# ─── Output ───
15
# ─────────────

# ─── Code Cell [2] ───
println(y)
```

Conversion tools for `.ipynb` files coming soon.

## Architecture

Nothelix consists of two main components:

### 1. Tree-sitter Grammar (`grammar.js`, `src/`, `queries/`)
Parses notebook cell format and provides:
- Syntax highlighting via language injection
- Cell boundary detection
- Text objects for navigation

### 2. Steel Plugin (`plugin/`)
Provides interactive execution:
- Spawns persistent Julia kernel per notebook
- Sends code to kernel via file-based IPC
- Captures output and inserts into buffer
- Custom UI components (cell picker)

See [docs/NOTEBOOK_ARCHITECTURE.md](docs/NOTEBOOK_ARCHITECTURE.md) for full technical details.

## Current Limitations

**Phase 1 Implementation:**
- Editor blocks during execution (async coming in Phase 2)
- Julia only (Python support planned)
- Text output only (rich output rendering planned)
- Synchronous kernel communication via files

**Not Yet Implemented:**
- Error highlighting
- Execution interruption (Ctrl-C)
- Multiple concurrent cell executions
- Markdown cell rendering
- .ipynb save/export

## Development

### Building the Grammar

```bash
npm install
npx tree-sitter generate
npx tree-sitter test
```

### Testing the Plugin

Open any `.ipynb` file in Helix and use the keybindings above.

## Contributing

This is Phase 1 of the implementation. Contributions welcome for:

- Async execution implementation
- Python kernel support
- Error handling improvements
- Rich output rendering (plots, tables, LaTeX)
- .ipynb bidirectional conversion

## License

MIT

## Project Structure

```
nothelix/
├── grammar.js              # Tree-sitter grammar definition
├── src/                    # Generated parser (C)
├── queries/                # Syntax highlighting queries
│   ├── highlights.scm      # Highlighting rules
│   ├── injections.scm      # Language injection
│   └── textobjects.scm     # Cell text objects
├── plugin/                 # Helix Steel plugin
│   ├── helix.scm          # Main plugin (commands, keybindings, UI)
│   └── kernel-manager.scm  # Kernel lifecycle management
├── docs/                   # Documentation
├── package.json            # Tree-sitter metadata
└── README.md              # This file
```
