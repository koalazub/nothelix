---
title: Notebooks
nav_order: 3
---

# Notebooks

A notebook in nothelix is a plain Julia file with explicit cell markers. This is
the central design decision, and most of the workflow follows from it.

## Why text, not JSON

A `.ipynb` file is JSON with cells, metadata, and base64-encoded outputs all
serialised together. The files are large, slow to parse, and hostile to version
control. Opening a big one through the editor's plugin layer can stall the UI for
seconds while the whole structure is parsed on the main thread.

Nothelix sidesteps the format entirely, following Marimo's lead: a notebook is a
decorated source file, with cell boundaries marked in the code itself. The cost
shifts from "parse everything, always" to "parse once, then edit text." You get a
file you can read, diff, and edit with ordinary modal motions, and the editor
stops burning cycles re-parsing a bloated structure on every keystroke.

## The cell format

A converted notebook looks like this.

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

`@cell` marks a code cell; `@markdown` marks a prose cell whose body is written as
comments. The number after each marker is the cell index. Because the markers are
defined as no-op macros at the top of the file, a converted notebook is still a
valid Julia program you can run with `julia notebook.jl` outside Helix entirely.

## Opening an existing notebook

A raw `.ipynb` is JSON and not directly editable, so convert it first.

```
hx examples/simple.ipynb
```

Run `:convert-notebook`. Nothelix writes a `.jl` companion file and opens it in
place. To push your edits back into the original `.ipynb` — for Jupyter, or for
collaborators who expect that format — run `:sync-to-ipynb`. The sync reads the
markers, extracts the updated source, and rewrites the JSON without disturbing the
parts you did not touch.

## Starting from scratch

You do not need an `.ipynb` to begin. The fastest path from nothing is one
command.

```
:new-notebook maths.jl
```

That creates `maths.jl` from a one-cell template and opens it. From there, the
notebook grows as you type.

- **Type `@cell` and press space.** A small popup asks whether you want a code
  cell or a markdown cell. Pick one, and the plugin stamps the next cell index,
  the file's language, and parks the cursor where you start typing. You never type
  a number or `:julia` by hand.
- **Type `@md` (or `@mark`, or `@markdown`) and press space.** Same idea, no
  popup — markdown is unambiguous, so it expands directly.
- **Press `<space>nn`** on an existing notebook to open that picker without typing
  anything. Handy when you are at the bottom of a cell and want another one.

Any `@<word>` followed by a space on an otherwise-blank line opens the picker, so
guesses and typos like `@code` or `@c` still give you something rather than
nothing. You never have to remember the exact marker syntax.

## Running cells

| Command | What it runs |
|---|---|
| `:execute-cell` (`<space>nr`) | The code cell under the cursor |
| `:execute-all-cells` | Every code cell, top to bottom |
| `:execute-cells-above` | Every code cell from the top down to the current one |
| `:cancel-cell` | Interrupts a running execution |

The first run is slow while Julia precompiles whatever you imported. Later runs
reuse the warm kernel, and state persists between cells exactly as it would in a
REPL — you do not re-import libraries or re-declare data on every run. As
execution finishes, output appears inline below each cell. A computation prints
its result; a plot renders in the terminal. See [Rendering](rendering.md) for how
figures and math get into the buffer, and [Architecture](architecture.md#the-kernel-protocol)
for how the kernel keeps state.

One kernel runs per open document, living until you call `:kernel-shutdown`
(`:kernel-shutdown-all` stops every running kernel). Quitting Helix stops them
for you.

## Moving around

| Command | Key | What it does |
|---|---|---|
| `:next-cell` | `]l` | Jump to the next cell |
| `:previous-cell` | `[l` | Jump to the previous cell |
| `:cell-picker` | `<space>nj` | Open an interactive cell navigator |
| `:select-cell` | `<space>na` | Select the whole cell (header, code, output) |
| `:select-cell-code` | `<space>ni` | Select only the code |
| `:select-output` | `<space>no` | Select the output block |

## Tidy indices, automatically

When you save, nothelix runs a quiet renumber pass that compacts cell indices to a
contiguous `0, 1, 2, …`. Holes left by deleting or rearranging cells get cleaned
up without you thinking about it. To trigger it yourself, `:renumber-cells` does
the same on demand.

The full command and keybinding reference lives on the
[commands](commands.md) page.
