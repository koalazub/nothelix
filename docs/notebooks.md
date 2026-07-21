---
title: Notebooks
nav_order: 3
---

# Notebooks

A nothelix notebook is ordinary Julia source with cell markers laid over it.
Here is one, cells and all.

```julia
using Plots

@cell 0 :julia
x = 1:10
y = x.^2

@markdown 1 # Results

@cell 2 :julia
plot(x, y)
```

Because the markers are comments, the file stays diffable, editable, and
runnable like any other Julia source. Read [Architecture](architecture.md) for
why the format is a script rather than JSON.

## The cell markers

A cell marker is the line that starts a cell. There are four kinds, and each one
opens a cell that runs until the next marker or the end of the file.

| Marker | Cell kind | What it holds |
|---|---|---|
| `@cell` | code cell | Julia that the kernel runs |
| `@markdown` | markdown cell | Prose written as `#`-prefixed comment lines |
| `@raw` | raw cell | Content that passes through conversion verbatim, never executed and never rendered |
| `@typst` | Typst cell | A Typst source block |

The number after the marker is the cell index. Every marker is a no-op macro
that the kernel defines, so the whole file still runs under `julia notebook.jl`
with the markers doing nothing.

If a file has real code above the first marker, nothelix lifts it into an
implicit preamble cell at index `-1` that runs before everything else. This is
where top-of-file `using` lines live, and it runs first so later cells see the
packages they load. A lone `using NothelixMacros` line is the one exception. It
is dropped rather than run, because that package no longer exists and the kernel
defines the markers itself.

## Marker labels

A marker line can carry a trailing comment, and that comment becomes a
persistent label for the cell. The grammar is a space, then `#`, then the label
text.

```julia
@cell 3 :julia # Day E
@markdown 5 # Q1
```

The `#` is required. `extract_marker_comment` parses the trailing comment off
the marker line, and on export the label is prepended to the cell source so it
lands inside the `.ipynb`. A code cell keeps the label as a leading comment
line, while a markdown, raw, or Typst cell gets it as a `#`-prefixed heading.
Because the label is written into the exported cell source, it survives a
round-trip to `.ipynb` and back.

The label is also the first and highest-priority source for a cell's row label
in the navigator, ahead of the on-device model label and the cell's first
meaningful line. Give a cell a marker label and that is exactly what you see
when you jump between cells.

## Opening an existing notebook

An `.ipynb` file is JSON, so convert it to a `.jl` first.

```
hx examples/simple.ipynb
```

| Command | What it does |
|---|---|
| `:convert-notebook` | Write a `.jl` companion and open it in place |
| `:sync-to-ipynb` | Push `.jl` edits back into the source `.ipynb` |

`:sync-to-ipynb` rewrites only the cell source and leaves the rest of the JSON
untouched.

## Starting from scratch

```
:new-notebook maths.jl
```

This creates `maths.jl` from a one-cell template and opens it. Grow the file
with autofill instead of typing markers by hand.

| You type | You get |
|---|---|
| `@cell<space>` | A picker for code or markdown, then the marker stamped with the next index |
| `@md<space>` (or `@mark`, `@markdown`) | A markdown cell, no picker |
| `@<anything><space>` | The same picker, forgiving of typos like `@code` or `@c` |
| `<space>nn` | The picker with nothing typed |

You never type a cell index or `:julia` by hand.

## Running cells

| Command | Key | What it runs |
|---|---|---|
| `:execute-cell` | `<space>nr` | The code cell under the cursor |
| `:execute-all-cells` | | Every code cell, top to bottom |
| `:execute-cells-above` | | Every cell from the top down to the cursor |
| `:cancel-cell` | | Interrupts a running execution |

The first run is slow while Julia precompiles imports. Later runs reuse the warm
kernel, and state persists between cells the way it does in a REPL. Output
appears inline below each cell as execution finishes.

See [Rendering](rendering.md) for how figures and math reach the buffer.

## Output and undo

Text output renders as virtual rows below the cell. It is not buffer text, so it
never enters undo and is never written into the `.jl` file. Editing a cell and
running it is one `u` away from a clean slate, because the run itself leaves
nothing to undo.

<!-- SCREENSHOT NEEDED: virtual-row text output sitting below a cell, showing output that is not buffer text -->

Output persists per-cell in `~/.local/share/nothelix/`, keyed to the cell and a
hash of its source. Reopening a notebook shows each cell's last output. If you
have edited the source since the last run, that output is stale, so nothelix
leaves it blank until you run the cell again.

Plots still reserve real buffer lines for their height. On a fork build with
tagged-undo support, those reserve-line edits are skipped by undo too, the same
as text. On an older build they still cost one undo step.

## Kernel persistence

One kernel runs per notebook, keyed to the file path and not to the buffer.
Close and reopen the file, or restart Helix, and nothelix reattaches to the
running kernel with all state intact. State is lost only on `:kernel-shutdown`,
`:kernel-shutdown-all`, or quitting Helix.

## Resume position

Reopening a notebook returns your cursor to the last cell you worked in. The
position is captured when you run a cell or save the file with `:w`, and it is
restored when you open the notebook. It is stored per-user in
`~/.local/share/nothelix/`, not in the project directory.

## Per-project settings

Drop a `.nothelix.conf` file at a project root. Nothelix reads it when you open
a notebook underneath.

| Key | Effect | Default |
|---|---|---|
| `conceal-on-open` | Auto-conceal on open | `true` |
| `math-font-pt`, `math-color` | Size and colour math images | |
| `table-font-pt` | Size table images | |
| `render-width` | Pin image width | |
| `plots-per-cell` | Cap on stacked plots rendered per cell, from `1` to `256` | `32` |
| `plot-mode` | Force `raster` or `braille` plot rendering, or `auto` to decide from the plotting backend | `auto` |
| `slm-summaries` | Label picker rows with Apple's on-device model (macOS 26+, needs Apple Intelligence, falls back to first-line heuristics) | `false` |
| `julia-bin`, `julia-project` | Pin the Julia binary or environment for cells | PATH `julia` |

`julia-bin` and `julia-project` execute code, so they take effect only after you
trust the directory.

| Command | What it does |
|---|---|
| `:nothelix-trust-project` | Trust the directory and enable its pinned Julia |
| `:nothelix-untrust-project` | Revoke trust |
| `:nothelix-project-trust-status` | Show the current trust state |

## Moving around

| Command | Key | What it does |
|---|---|---|
| `:next-cell` | `]l` | Jump to the next cell |
| `:previous-cell` | `[l` | Jump to the previous cell |
| `:cell-picker` | `<space>nj` | Open an interactive cell navigator |
| `:select-cell` | `<space>na` | Select the whole cell, header, code, and output |
| `:select-cell-code` | `<space>ni` | Select only the code |
| `:select-output` | `<space>no` | Select the output block |

## The cell navigator

`<space>nj` opens a two-pane picker. Every cell shows as `index · type · label`,
with a live preview of the selected cell.

![Cell navigator with on-device labels](assets/img/cell-picker-slm.png)

Type a cell number to jump straight to it. Press `/` and fuzzy-search the labels,
where typing `pseudo` narrows to the pseudoinverse cells. Use `j` or `k` or the
arrows to scroll one row, and `h` or `l` to jump several rows at once. Press
`Enter` to go there. The jump distance defaults to ten rows and is set with
`picker-jump` in `.nothelix.conf`.

Row labels come from three sources in order. A marker label wins first, then an
on-device model label, and finally the cell's first meaningful line.

With `slm-summaries = true` in `.nothelix.conf`, rows are labelled by Apple's
on-device model on macOS 26 or newer, which requires Apple Intelligence. Labels
like `section 7: pseudoinverse` or `verification 1: residuals` are generated in
the background the first time a picker opens on a notebook. They are cached by
cell content and recomputed only for cells that changed, so there is no network,
no bundled model, and near-zero steady-state cost. Without the config, or on a
machine without the model, the picker falls back to first-line labels.

## Cell indices

On save, nothelix compacts cell indices to a contiguous `0, 1, 2, …`, cleaning
holes left by deleted or reordered cells. Run `:renumber-cells` to trigger it on
demand.

The full command and keybinding reference lives on the [commands](commands.md)
page.
