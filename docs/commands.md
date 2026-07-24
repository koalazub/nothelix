---
title: Commands and keys
nav_order: 7
---

# Commands and keys

This is the authoritative reference. Every command runs from command mode, so you
type `:command-name` and press Enter. The keybindings, shorthands, and autofill
expansions below are registered only for `.jl` and `.ipynb` files, so they stay
out of your way in ordinary buffers.

## Notebook lifecycle

| Command | Description |
|---|---|
| `:new-notebook [path]` | Create a new `.jl` notebook from a starter template and open it |
| `:convert-notebook` | Convert a raw `.ipynb` into the editable cell format |
| `:sync-to-ipynb` | Sync edits in the `.jl` file back to the source `.ipynb` |
| `:renumber-cells` | Renumber the cell markers to a contiguous `0, 1, 2, …` |
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
| `:copy-cell-output` | Copy the cell's rendered output to the system clipboard |
| `:cell-output-view` | Blow up the cell's output in a popup, live while it runs or stored otherwise |
| `:play-cell-audio` | Play the cell's wavplay audio through the system output (non-blocking) |
| `:stop-audio` | Stop the audio clip that is currently playing |
| `:scrub-audio` | Open scrub mode to seek the cell's audio by ear before resuming |

## Navigation and selection

| Command | Description |
|---|---|
| `:next-cell` | Jump to the next cell |
| `:previous-cell` | Jump to the previous cell |
| `:widget-walk-next` | Jump to the next widget and name its keys in the status line |
| `:widget-walk-prev` | Jump to the previous widget and name its keys in the status line |
| `:cell-picker` | Open the interactive cell navigator |
| `:new-cell` | Insert a new cell at the cursor and open the code or markdown picker |
| `:select-cell` | Select the whole cell, meaning the header, the code, and the output |
| `:select-cell-code` | Select only the code |
| `:select-output` | Select the output block |

## Plots and images

| Command | Description |
|---|---|
| `:view-chart` | Open the last-executed plot in the interactive chart viewer |
| `:insert-image` | Insert a `# @image <path>` marker and a blank canvas at the cursor |
| `:plot-grow` | Grow the plot block under the cursor and re-render |
| `:plot-shrink` | Shrink the plot block under the cursor and re-render |
| `:param-up` | Increase the numeric literal on a `# @param` line and re-run the cell |
| `:param-down` | Decrease the numeric literal on a `# @param` line and re-run the cell |
| `:select-next` | Cycle the `# @select` value forward to the next option and re-run the cell |
| `:select-prev` | Cycle the `# @select` value back to the previous option and re-run the cell |
| `:select-choice` | Open the `# @select` chooser modal at or above the cursor |
| `:toggle-flag` | Flip the boolean on a `# @toggle` line and re-run the cell |

A `# @param` annotation marks a numeric literal as a live knob, as in
`freq = 440   # @param 220:880 step 10`. The grammar has two parts. The clamp
range `<lo>:<hi>` is required, and the optional `step <s>` sets the increment. A
missing step defaults to `1` for integers and to `(hi - lo) / 100` for floats.
Nudging with `]p` or `[p` rewrites the literal instantly in the buffer, then
debounces a re-run of the same cell with no downstream side effects, and it flags
cells below as stale when they reference the changed variable. That staleness
check is a name-based heuristic that matches tokenized words and never traces
dataflow. Reassignment or shadowing can over-flag or under-flag, so read the
staleness marker as an alert rather than a guarantee. While the cursor sits in a
`@param` cell, or the widget walk lands on one, a one-row slider track is drawn
above the line showing the value's position in its range; it clears when you
leave the cell.

A `# @select` annotation marks an assignment as a closed set, as in
`wave = "sin"   # @select sin|cos|tan`. Its grammar mirrors `@param`: the name is
the assignment's left side and the trailing comment carries the pipe-delimited
options. `]s` and `[s` cycle the value with wrap-around, and `<space>nc` opens a
chooser where `h`/`l` move through the options and `Enter` applies. A quoted
current value is rewritten with quotes, a bare identifier without — the shape is
read from the literal, never guessed. A `# @toggle` annotation marks a boolean,
as in `loop = true   # @toggle`; `<space>nt` flips it in place. Both stage
downstream staleness and debounce a re-run exactly like `@param`.

A run can also declare a widget from the kernel with `nothelix_slider` or
`nothelix_choice`. These render as a row under the cell and reuse the same keys,
so `]p` and `[p` nudge a kernel slider and `]s` and `[s` cycle a kernel choice on
the cell under the cursor, and `<space>nc` opens its modal. A nudge assigns the
variable in the kernel and flags the cells that read it as stale, with no re-run
of its own, and it does nothing but say so when the kernel is not running. See
[Notebooks](notebooks.md) for a worked example.

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

## Kernel, status, and project trust

| Command | Description |
|---|---|
| `:kernel-shutdown` | Stop the kernel for the current document |
| `:kernel-shutdown-all` | Stop every running kernel |
| `:graphics-protocol` | Show which graphics protocol was detected |
| `:graphics-check` | Run a quick graphics diagnostic |
| `:nothelix-status` | Show full status, covering kernels, graphics, the language server, and more |
| `:nothelix-trust-project` | Trust this project's `.nothelix.conf` to launch a custom Julia binary or environment, then restart the kernel |
| `:nothelix-untrust-project` | Revoke that trust and revert to PATH Julia, then restart the kernel |
| `:nothelix-project-trust-status` | Report whether the current project is trusted and the runtime it would use |

Trust gates only the executable settings in a project's `.nothelix.conf`, which
are `julia-bin` and `julia-project`. The display settings apply without trust.

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

These are registered in normal mode for `.jl` and `.ipynb` files.

| Key | Command |
|---|---|
| `]l` | `:next-cell` |
| `[l` | `:previous-cell` |
| `]p` | `:param-up` |
| `[p` | `:param-down` |
| `]s` | `:select-next` |
| `[s` | `:select-prev` |
| `]a` | `:audio-seek-forward` |
| `[a` | `:audio-seek-back` |
| `]w` | `:widget-walk-next` |
| `[w` | `:widget-walk-prev` |
| `<space>nr` | `:execute-cell` |
| `<space>nn` | `:new-cell` |
| `<space>nj` | `:cell-picker` |
| `<space>na` | `:select-cell` |
| `<space>ni` | `:select-cell-code` |
| `<space>no` | `:cell-output-view` |
| `<space>ny` | `:copy-cell-output` |
| `<space>ns` | `:play-cell-audio` |
| `<space>nx` | `:stop-audio` |
| `<space>nc` | `:select-choice` |
| `<space>nt` | `:toggle-flag` |
| `<space>n=` | `:plot-grow` |
| `<space>n-` | `:plot-shrink` |
| `<space>p` | `:animation-toggle-at-cursor` |
| `Tab` (insert mode, `.jl` only) | `:julia-tab-complete` |

### Scrub mode

`:scrub-audio` opens a small waveform popup over the cell. Press `h` and `l` to
move the playhead back and forward by the current step, and `j` and `k` to make
that step coarser or finer along the ladder. A dimmed bracket shows how far one
step reaches, and the playhead sweeps to its new column on each move. Press
`Enter` to resume playback from the playhead, or `Esc` (or `q`) to leave and
keep playback as it was. Pressing `<space>ns` on the cell that is already
playing opens the same popup. `]a` and `[a` seek without the popup, and pressing
them in quick succession accelerates the step through the ladder.

### Widget walk

`]w` and `[w` walk the cursor between every widget in the notebook, wrapping at
the ends. A widget is any interactive surface: a `# @param` knob, a `# @select`
choice, a `# @toggle` flag, a `nothelix_slider` or `nothelix_choice` a run
declared, a cell's audio scrub, an `@image` plot's size, or an animation. Each jump names the widget and the keys that act on it in the status
line, so the surface teaches itself; landing on a `@param` also draws its slider
track. The walk and the shared modal are gated by the `widgets` setting in
`.nothelix.conf`; when it is `false` both no-op and the direct feature keys
(`]p`/`[p`, `]s`/`[s`, `]a`/`[a`, `<space>nt`, the plot and animation commands)
still work.

## Shorthands

A few aliases stand in for the commands you reach for most.

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
| `@cell<space>` | `@cell N :julia` stamped with the next index |
| `@md<space>`, `@mark<space>`, `@markdown<space>` | `@markdown N`, with the cursor parked after `# ` on the next line |
| `@typst<space>` | `@typst N`, with the cursor parked after `# ` on the next line |

Only these exact words expand. Any other `@word` is Julia code (`@show`,
`@time`, …) and is left alone; `:new-cell` opens the cell-type picker when you
want one.

For the source-level anatomy of these markers, including the persistent label you
can pin to a marker line, see [Notebooks](notebooks.md). When a command misbehaves,
[Troubleshooting](troubleshooting.md) covers the debug log and the common breakages.
