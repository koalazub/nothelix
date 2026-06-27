---
title: Commands and keys
nav_order: 7
---

# Commands and keys

The authoritative reference. Every command is invoked from command mode
(`:command-name`); the keybindings and autofill shortcuts below are registered
only for `.jl` and `.ipynb` files.

## Notebook lifecycle

| Command | Description |
|---|---|
| `:new-notebook [path]` | Create a new `.jl` notebook from a starter template and open it |
| `:convert-notebook` | Convert a raw `.ipynb` into the editable cell format |
| `:sync-to-ipynb` | Sync edits in the `.jl` file back to the source `.ipynb` |
| `:renumber-cells` | Renumber `@cell` and `@markdown` markers to a contiguous `0, 1, 2, …` |
| `:export-markdown` | Export the notebook to Markdown (`.md`) |
| `:export-typst` | Export the notebook to Typst (`.typ`) |
| `:export-pdf` | Export the notebook to a typeset PDF (`.pdf`) |

## Cell execution

| Command | Description |
|---|---|
| `:execute-cell` | Run the code cell under the cursor |
| `:execute-all-cells` | Run every code cell, top to bottom |
| `:execute-cells-above` | Run every code cell from the top down to the current one |
| `:cancel-cell` | Interrupt the currently running cell |

## Navigation and selection

| Command | Description |
|---|---|
| `:next-cell` | Jump to the next cell |
| `:previous-cell` | Jump to the previous cell |
| `:cell-picker` | Open the interactive cell navigator |
| `:new-cell` | Insert a new cell at the cursor (opens the code/markdown picker) |
| `:select-cell` | Select the whole cell (header, code, output) |
| `:select-cell-code` | Select only the code |
| `:select-output` | Select the output block |

## Plots and images

| Command | Description |
|---|---|
| `:view-chart` | Open the last-executed plot in the interactive chart viewer |
| `:insert-image` | Insert a `# @image <path>` marker and a blank canvas at the cursor |
| `:plot-grow` | Grow the plot block under the cursor and re-render |
| `:plot-shrink` | Shrink the plot block under the cursor and re-render |

## Mathematics and tables

| Command | Description |
|---|---|
| `:render-math-at-cursor` | Render the `$$ … $$` block under the cursor as a typeset image |
| `:render-all-display-math` | Render every display-math block in the buffer |
| `:render-all-tables` | Render every Markdown pipe table as a typeset image |
| `:clear-math-images` | Remove inline math and table images, restoring the source |
| `:format-math-buffer` | Expand single-line environments into multi-line `$$` blocks |
| `:math-render-buffer` | Stack big-operator limits above and below their glyph |
| `:math-render-clear` | Remove the stacked-limit annotations |
| `:conceal-math` | Apply LaTeX-to-Unicode concealment to the buffer |
| `:clear-conceal` | Remove the concealment overlays |
| `:julia-tab-complete` | Expand a `\<name>` Julia LaTeX shortcut at the cursor |

## Kernel and status

| Command | Description |
|---|---|
| `:kernel-shutdown` | Stop the kernel for the current document |
| `:kernel-shutdown-all` | Stop every running kernel |
| `:graphics-protocol` | Show which graphics protocol was detected |
| `:graphics-check` | Run a quick graphics diagnostic |
| `:nothelix-status` | Show full status (kernels, graphics, language server, and more) |

## Animation

| Command | Description |
|---|---|
| `:animation-toggle-at-cursor` | Play or pause the animation at the cursor |
| `:animation-pause-all` | Pause every animation |
| `:animation-resume-all` | Resume every animation |

## Debugging and tests

| Command | Description |
|---|---|
| `:nothelix-debug-on` / `:nothelix-debug-off` / `:nothelix-debug-toggle` | Control debug logging |
| `:run-all-tests` | Run every Steel test suite |
| `:run-cell-tests` | Run the cell-extraction tests |
| `:run-kernel-tests` | Run the kernel-persistence tests |
| `:run-execution-tests` | Run the execution-flow tests |

See [Troubleshooting](troubleshooting.md) for what to do with the debug log.

## Keybindings

Registered in normal mode for `.jl` and `.ipynb` files.

| Key | Command |
|---|---|
| `]l` | `:next-cell` |
| `[l` | `:previous-cell` |
| `<space>nr` | `:execute-cell` |
| `<space>nn` | `:new-cell` |
| `<space>nj` | `:cell-picker` |
| `<space>na` | `:select-cell` |
| `<space>ni` | `:select-cell-code` |
| `<space>no` | `:select-output` |
| `<space>n=` | `:plot-grow` |
| `<space>n-` | `:plot-shrink` |
| `<space>p` | `:animation-toggle-at-cursor` |
| `Tab` (insert mode, `.jl` only) | `:julia-tab-complete` |

## Shorthands

A few aliases for the commands you reach for most.

| Shorthand | Expands to |
|---|---|
| `:xc` | `:execute-cell` |
| `:xca` | `:execute-all-cells` |
| `:nc` | `:next-cell` |

## Autofill

At the start of an otherwise-blank line in a notebook file, these expand when you
press space.

| You type | You get |
|---|---|
| `@cell<space>` | The code/markdown picker, then `@cell N :julia` stamped with the next index |
| `@md<space>`, `@mark<space>`, `@markdown<space>` | `@markdown N`, cursor parked after `# ` on the next line |
| `@<anything><space>` | The same picker as `@cell`, forgiving of typos like `@code` or `@c` |
