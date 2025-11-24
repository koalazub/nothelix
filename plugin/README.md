# Nothelix Steel Plugin

Interactive cell execution plugin for Jupyter notebooks in Helix editor.

## Overview

This Steel plugin provides:
- Cell execution with persistent kernel state
- vim-style cell navigation
- Custom UI components (cell picker with preview)
- File-scoped keybindings for `.ipynb` files

## Files

- `helix.scm` - Main plugin with commands, UI components, and keybindings
- `kernel-manager.scm` - Kernel process lifecycle management

## Installation

Copy both files to your Helix config directory:

```bash
cp helix.scm ~/.config/helix/
cp kernel-manager.scm ~/.config/helix/
```

Restart Helix for changes to take effect.

## Commands

### `:execute-cell`
Executes the code cell under the cursor.
- Finds cell boundaries by parsing `# ─── Code Cell` markers
- Sends code to persistent Julia REPL
- Waits synchronously for completion
- Inserts output below cell

### `:next_cell`
Jumps to the next cell in the notebook.
- Searches forward for next `# ─── Code Cell` marker
- Handles edge case when cursor is on a marker

### `:previous_cell`
Jumps to the previous cell in the notebook.
- Searches backward for previous `# ─── Code Cell` marker
- Handles edge case when cursor is on a marker

### `:cell-picker`
Opens an interactive cell picker UI.
- Lists all cells in the notebook
- Live preview pane shows cell contents
- Navigate with j/k or number keys
- Press Enter to jump to selected cell

## Keybindings

All keybindings are scoped to `.ipynb` file extension only using the `keymap` macro:

```scheme
(keymap (extension "ipynb")
        (normal
          ("[" (l ":previous_cell"))
          ("]" (l ":next_cell"))
          (space (n (r ":execute-cell") (j ":cell-picker")))))
```

- `[l` - Previous cell
- `]l` - Next cell
- `<space>nr` - Execute cell
- `<space>nj` - Cell picker

## How It Works

### Kernel Management

The kernel manager (`kernel-manager.scm`) spawns a persistent Julia REPL process for each notebook:

1. Creates temp directory `/tmp/helix-kernel-<id>/`
2. Generates Julia runner script that:
   - Polls for input file (`input.jl`)
   - Executes code via `include_string`
   - Captures stdout/stderr to output file
   - Writes completion marker
3. Keeps REPL alive for entire editing session

### Cell Execution Flow

1. User presses `<space>nr`
2. Plugin finds cell boundaries using rope operations
3. Extracts cell code (everything between header and next marker)
4. Writes code to kernel input file
5. Waits synchronously for `.done` flag file
6. Reads output file and inserts into buffer
7. Cleans up old output section if it exists

### Cell Picker Component

Built using Steel's component system (`new-component!`):

```scheme
(struct CellPickerState (cells selected) #:mutable)

(define (render-cell-picker state rect buf)
  ;; Render list of cells and preview pane
  ...)

(define (handle-picker-event state event)
  ;; Handle keyboard input (j/k/1-9/Enter/ESC)
  ...)
```

The picker:
- Parses all cells when opened
- Renders two panels: list + preview
- Truncates long lines to fit preview width
- Updates preview as you navigate

## Limitations (Phase 1)

- **Synchronous execution**: Editor blocks during cell execution
- **Julia only**: Python kernel not yet implemented
- **Text output only**: No rich output (plots, HTML, LaTeX)
- **File-based IPC**: Uses temp files for kernel communication
- **No error display**: Errors captured but not syntax highlighted

## Technical Details

### Steel APIs Used

- **Text operations**: `helix.current-line`, `text.rope->string`, `text.rope->line`
- **Buffer manipulation**: `helix.static.insert_string`, `helix.select-regex`, `helix.static.delete_selection`
- **Shell commands**: `helix.run-shell-command`, `helix.insert-output`
- **UI components**: `new-component!`, `buffer/clear`, `frame-set-string!`, `block/render`
- **Theming**: `theme-scope` for consistent styling
- **Keybindings**: `keymap` macro with file extension scoping

### Why Synchronous?

Steel has no built-in file I/O APIs. `helix.run-shell-command` returns void, not strings. This means:
- Cannot read kernel output directly in Steel
- Cannot poll file existence without blocking
- Must use shell-based waiting (`while [ ! -f file ]; do sleep 0.1; done`)

Phase 2 will explore either:
1. Adding file I/O primitives to Steel
2. Using Steel's callback system more creatively
3. Kernel push model (kernel writes directly to Helix socket)

## Future Improvements

- Async execution using `enqueue-thread-local-callback`
- Python kernel support (detect language from cell metadata)
- Error handling and display with syntax highlighting
- Rich output rendering (detect MIME types)
- Progress indicator for long-running cells
- Cell execution history
- Kernel restart command

## Contributing

The plugin is designed to be extended. Key extension points:

1. **Language support**: Modify `kernel-manager.scm` to spawn Python/R kernels
2. **Output rendering**: Parse output for rich formats (SVG, LaTeX, HTML)
3. **UI improvements**: Add more picker features (search, filter)
4. **Async execution**: Leverage Steel's callback system

See the main [README](../README.md) for contribution guidelines.
