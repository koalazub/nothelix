---
title: Widget architecture
nav_order: 11
---

# Widget architecture

Widgets are how nothelix manipulates data from inside the buffer. A widget is
one contract with five parts, and everything interactive in the plugin is an
instance of it.

- **Declaration.** Where the widget comes from. Source widgets are comment
  annotations in the notebook (`# @param 220:880 step 10`), so they diff,
  grep, and survive like any other line of the file. Output widgets attach to
  something a run produced (a clip's scrub surface, a plot's size) and live
  only as long as the output does.
- **State.** The current value, the step ladder, and the anchor that ties the
  widget to its cell. Held in one per-document registry rather than scattered
  module boxes.
- **Rendering.** What the widget draws, through the same virtual-row and
  overlay machinery as every other surface. A widget's row always names the
  key that acts on it in the current state, the same self-teaching rule the
  audio header follows.
- **Motions.** One grammar everywhere. `]w` and `[w` walk between widgets.
  Direct nudges act on the widget at or above the cursor. A widget whose
  manipulation needs more than a nudge opens a modal where `h`/`l` move the
  value, `j`/`k` change the granularity, `Enter` applies, `Esc` leaves. The
  scrub modal defined this grammar; every widget speaks it.
- **Effect.** What a change does. Source widgets rewrite their literal in the
  buffer and debounce a re-run of the owning cell, and downstream staleness
  flows from the provenance ledger exactly as if you had edited the cell by
  hand. Output widgets call their subsystem directly. There is no third path.

## Why this shape

The notebook stays a plain Julia file. Widgets never inject state the file
does not carry, a source widget IS its annotation line, so checking out a
commit reproduces the knobs exactly. The kernel stays inert, annotations are
comments and the marker macros ignore them. And the interaction stays
motion-first, no widget requires the mouse, the palette, or documentation to
operate, because its surface names its keys.

## The kinds

| Kind | Declaration | Nudge | Modal |
|---|---|---|---|
| number | `# @param <lo>:<hi> [step <s>]` | `]p` / `[p` | slider track above the line |
| choice | `# @select a\|b\|c` | `]s` / `[s` | option chooser (`h`/`l`), `<space>nc` |
| flag | `# @toggle` | `<space>nt` | none |
| scrub | a cell's audio artifact | `]a` / `[a` | playhead, bracket, seek ladder |
| size | an `@image` plot block | `:plot-grow` / `:plot-shrink` | none |
| toggle | an animation at the cursor | `<space>p` | none |
| kernel slider | a `nothelix_slider` call in a run | `]p` / `[p` | slider track popup (`h`/`l`), `<space>nc` |
| kernel choice | a `nothelix_choice` call in a run | `]s` / `[s` | option chooser (`h`/`l`), `<space>nc` |

Every kind is a leaf module that supplies parse, render, nudge, and apply; the
registry, motions, modal shell, and re-run pipeline are shared. `choice` rewrites
an assignment from a closed set, inferring quoting from the current literal, and
`flag` flips a boolean. Both reuse `@param`'s trailing-comment grammar and line
targeting: the name is the assignment's left side, and the comment carries only
the option set. The `number` row's modal is the one spec adjustment from the
original plan — instead of a ladder scrub, the nudged param shows a one-row
slider track (value position in range plus its keys), rendered on demand above
the line while its cell holds the cursor and cleared on leave.

## Projection

The kinds above are a closed vocabulary, but the producers are open. Any Julia
library can make its own objects appear as widgets by defining one method of
`nothelix_towidget`. The base method returns nothing for every value, so an
ordinary result stays ordinary. A library adds a method for its own type that
returns a NamedTuple or Dict naming a kind and a name, plus the fields that kind
needs (`lo` and `hi` and an optional `step` for a slider, `options` for a
choice), and any run whose cell returns that object grows the matching row.

The kernel treats a projection exactly as it treats a `nothelix_slider` call. It
validates that the kind is one it knows, the name is a plain identifier, and the
params are well formed, then registers the spec on the cell through the same
machinery. A projection it cannot recognise degrades to plain output; the kernel
writes one warning line into the cell's stderr and the value still displays as it
always would, so an unknown kind never eats a cell's real result. A library that
would rather register a spec by hand than define a method calls `nothelix_widget`
with the same NamedTuple or Dict and takes the same validation path.

The vocabulary is the compatibility contract. A projection speaks in the kinds
the plugin already renders, so a new library lights up a widget with no plugin
change and no FFI change. The spec pipeline carries kind, name, params, and
current, and it now takes three kinds of producer, the source annotation, the
kernel call, and the library projection.

## Phases

**Phase 1 (shipped with this document).** The contract extracted into
`plugin/nothelix/widgets.scm`: the registry, the widget walk, the shared modal
shell with the h/l/j/k grammar, and the debounced re-run effect lifted out of
param-tweak. `@param` and audio scrub are re-seated as instances with their
existing keys and behaviour unchanged. A `widgets` knob in `.nothelix.conf`
(default on) gates the unified surfaces — the walk and the shared modal — while
the pre-widget feature keys keep working when it is off.

**Phase 2 (shipped).** The `choice` (`# @select`) and `flag` (`# @toggle`) kinds,
each a leaf module reusing the shared registry, modal shell, and re-run pipeline;
the number slider track rendered as a virtual row above the param line via the
stale-tag above surface (which reserves rows on any line, not just markers), on
demand and cleared on leave; and widget-bearing cells marked with `⊞` in the
picker. One spec adjustment: the number modal became the slider track described
above rather than a ladder scrub, and the flag flip is bound to `<space>nt` (a
flip is non-directional, so no `]`/`[` pair) rather than a bracket nudge.

**Phase 3 (shipped).** Kernel-declared widgets. A cell's run can call
`nothelix_slider` or `nothelix_choice`, which records a spec on that cell in the
registry and returns nothing. The plugin materialises each spec as one virtual
row anchored below the cell, rendered through the same output-row composition as
the waveform group, and the specs persist alongside audio in the output store so
a reopen restores them. Manipulating a kernel widget sends the new value straight
to the kernel through a `set_var` command, which assigns the variable and records
the declaring cell as its fresh writer, so the existing provenance ledger flags
dependent cells stale with no auto-rerun. The row reuses the number and choice
grammar, so `]p` and `[p` nudge a kernel slider and `]s` and `[s` cycle a kernel
choice on the cell under the cursor, `<space>nc` opens the modal, and `]w` and
`[w` walk onto them like any other widget. When the kernel is not running a nudge
says so and asks you to run the cell first, and it never queues. This is
Jupyter-style interactive output without leaving the source-file model. These two
calls are not the only way onto this pipeline; the Projection section above shows
how any library reaches the same specs by returning its own object from a cell.
