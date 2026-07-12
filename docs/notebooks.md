---
title: Notebooks
nav_order: 3
---

# Notebooks

```julia
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

Notebooks are plain `.jl` files with `@cell` markers: diffable, editable, runnable. See [Architecture](architecture.md) for why.

- `@cell` marks a code cell.
- `@markdown` marks a prose cell, written as comments.
- The number is the cell index.
- The markers are no-op macros, so the file still runs with `julia notebook.jl`.

## Opening an existing notebook

An `.ipynb` is JSON, so convert it first.

```
hx examples/simple.ipynb
```

| Command | What it does |
|---|---|
| `:convert-notebook` | Write a `.jl` companion and open it in place |
| `:sync-to-ipynb` | Push `.jl` edits back into the source `.ipynb` |

`:sync-to-ipynb` rewrites only the cell source, leaving the rest of the JSON untouched.

## Starting from scratch

```
:new-notebook maths.jl
```

Creates `maths.jl` from a one-cell template and opens it. Grow it with autofill:

| You type | You get |
|---|---|
| `@cell<space>` | Picker for code or markdown, then the marker stamped with the next index |
| `@md<space>` (or `@mark`, `@markdown`) | A markdown cell, no picker |
| `@<anything><space>` | The same picker, forgiving of typos like `@code` or `@c` |
| `<space>nn` | Opens the picker with nothing typed |

You never type a cell index or `:julia` by hand.

## Running cells

| Command | Key | What it runs |
|---|---|---|
| `:execute-cell` | `<space>nr` | The code cell under the cursor |
| `:execute-all-cells` | | Every code cell, top to bottom |
| `:execute-cells-above` | | Every cell from the top down to the cursor |
| `:cancel-cell` | | Interrupts a running execution |

- The first run is slow while Julia precompiles imports.
- Later runs reuse the warm kernel; state persists between cells like a REPL.
- Output appears inline below each cell as execution finishes.

See [Rendering](rendering.md) for how figures and math reach the buffer.

## Output and undo

Text output renders as virtual rows below the cell: it is not buffer text, never
enters undo, and is never written into the `.jl` file. Editing a cell and running
it is one `u` away from a clean slate — the run itself leaves nothing to undo.

Output persists per-cell in `~/.local/share/nothelix/`, keyed to the cell and a
hash of its source. Reopening a notebook shows each cell's last output unless
you've edited the source since, in which case it's treated as stale and left
blank until you run the cell again.

Plots still reserve real buffer lines for their height. On a fork build with
tagged-undo support, those reserve-line edits are skipped by undo too, same as
text; on an older build they still cost one undo step.

## Kernel persistence

One kernel runs per notebook, keyed to the file path, not the buffer.

- Close and reopen the file, or restart Helix: nothelix reattaches to the running kernel with all state intact.
- State is lost only on `:kernel-shutdown`, `:kernel-shutdown-all`, or quitting Helix.

## Resume position

Reopening a notebook returns your cursor to the last cell you worked in. The position is captured when you run a cell or save the file (`:w`), and restored when you open the notebook. It is stored per-user in `~/.local/share/nothelix/`, not in the project directory.

## Per-project settings

Drop a `.nothelix.conf` at a project root. Nothelix reads it when you open a notebook underneath.

| Key | Effect | Default |
|---|---|---|
| `conceal-on-open` | Auto-conceal on open | `true` |
| `math-font-pt`, `math-color` | Size and colour math images | |
| `table-font-pt` | Size table images | |
| `render-width` | Pin image width | |
| `plots-per-cell` | Cap on stacked plots rendered per cell (1..256) | `32` |
| `plot-mode` | Force `raster` or `braille` plot rendering, or `auto` to decide from the plotting backend | `auto` |
| `julia-bin`, `julia-project` | Pin the interpreter or environment for cells | PATH `julia` |

`julia-bin` and `julia-project` execute code, so they take effect only after you trust the directory.

| Command | What it does |
|---|---|
| `:nothelix-trust-project` | Trust the directory; enables its pinned Julia |
| `:nothelix-untrust-project` | Revoke trust |
| `:nothelix-project-trust-status` | Show current trust state |

## Moving around

| Command | Key | What it does |
|---|---|---|
| `:next-cell` | `]l` | Jump to the next cell |
| `:previous-cell` | `[l` | Jump to the previous cell |
| `:cell-picker` | `<space>nj` | Open an interactive cell navigator |
| `:select-cell` | `<space>na` | Select the whole cell (header, code, output) |
| `:select-cell-code` | `<space>ni` | Select only the code |
| `:select-output` | `<space>no` | Select the output block |

## Cell indices

On save, nothelix compacts cell indices to a contiguous `0, 1, 2, …`, cleaning holes from deleted or reordered cells. Run `:renumber-cells` to trigger it on demand.

The full command and keybinding reference lives on the [commands](commands.md) page.
