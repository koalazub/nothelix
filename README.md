# tree-sitter-notebook

Tree-sitter grammar for notebook cell format, designed for [Nothelix](https://github.com/your-org/nothelix) (Jupyter notebooks in Helix editor).

## Overview

This grammar parses notebook files that have been converted to a readable cell format:

```julia
# ─── Code Cell [1] ───
t = 0:0.001:1
y = sin.(2π*10*t)

# ─── Markdown Cell ───
This is a markdown cell with **bold** text.

# ─── Output ───
Result: [0.0, 0.001, 0.002, ...]
# ─────────────
```

The grammar enables:
- Syntax highlighting via tree-sitter injections (Julia, Python, Markdown)
- Cell navigation as text objects
- LSP integration for code cells
- Seamless editing of Jupyter notebooks in Helix

## Features

- **Multi-language injection**: Code cells automatically highlight as Julia (configurable)
- **Markdown support**: Markdown cells render with full markdown syntax
- **Output sections**: Captured and styled distinctly
- **Text objects**: Navigate and select cells with `]c`, `[c`, `ac`, `ic`

## Installation

### Using with Helix

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
source = { git = "https://github.com/your-org/tree-sitter-notebook", rev = "main" }
```

Then run:
```bash
hx --grammar fetch
hx --grammar build
```

### Building from source

```bash
npm install
npx tree-sitter generate
npx tree-sitter test
```

## Grammar Structure

```
source_file
├── code_cell
│   ├── code_cell_header
│   │   └── execution_count
│   └── cell_content (injected: Julia/Python)
├── markdown_cell
│   ├── markdown_cell_header
│   └── cell_content (injected: Markdown)
└── output_section
    ├── output_header
    ├── output_content
    └── output_footer
```

## Queries

### Injections (`queries/injections.scm`)
- Code cells → Julia syntax highlighting
- Markdown cells → Markdown rendering
- Output → Plain text

### Highlights (`queries/highlights.scm`)
- Cell headers styled as documentation comments
- Execution counts as constants
- Cell markers as keywords

### Text Objects (`queries/textobjects.scm`)
- `ac` / `ic` - Around/inside cell
- `]c` / `[c` - Next/previous cell (via Nothelix keybindings)

## Language Detection

Currently defaults to Julia for all code cells. Future enhancements:

1. **Header annotation**:
   ```
   # ─── Code Cell [1] (python) ───
   ```

2. **Metadata comments**:
   ```
   # ─── Code Cell [1] ───
   #| language: python
   ```

3. **Content-based detection**: Shebang or syntax patterns

## Integration with Nothelix

This grammar is designed for the [Nothelix](https://github.com/your-org/nothelix) plugin, which provides:
- Automatic `.ipynb` to cell format conversion
- Persistent Julia/Python kernels
- Cell execution with inline output
- Bidirectional sync (editing → saving back to .ipynb)

## Contributing

Contributions welcome! Areas for improvement:
- Python language detection
- Rich output parsing (HTML, LaTeX, images)
- Multi-language support in single notebook
- Performance optimization for large notebooks

## License

MIT

## Credits

Built for the Nothelix project - bringing Jupyter notebook editing to Helix with full IDE features.
