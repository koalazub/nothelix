# Draft, WIP. Plugin decoration, a perspective from building Nothelix

This is meant as a perspective on where the plugin environment could go next,
not a hard feature request. It comes out of building Nothelix, a notebook setup
for scientific computing inside Helix. The aim there is simple. Someone should
be able to open a buffer and work on the computation itself, with plots and math
rendered where they belong, without thinking about the editor underneath or
running a patched build to get there. The note below describes the one thing
that currently sits between that experience and a plugin that runs on stock
Helix. It is offered as useful input for whoever shapes the plugin API next, and
the intent throughout is to fit into the direction already being taken rather
than push against it.

Tracking against helix-editor/helix#8675, "Add Steel as an optional plugin
system". The findings below were checked against the head of that PR, commit
`0522d519` on mattwparas's `steel-event-system` branch, which is the exact
commit the PR proposes to merge into Helix master. Each point was verified
against the four things that make up the plugin surface, namely the Scheme
cogs, the Rust `register_fn` builtins, the generated `steel-docs.md` reference,
and `STEEL.md`. The experimental `mwp-*` branches were checked too and add
nothing relevant.

## What works today

A plugin can build its own interface. It can add statusline elements, open
popups and pickers, and render custom components, and it can drive the editor
by chaining the same typed and static commands a person would run. That is
enough for tools that sit alongside the buffer rather than inside it.

## What is missing

A plugin cannot change how the editor draws the buffer the user is editing.
Nothing in the API lets it anchor an image or a widget to a line, reserve blank
rows inside the text flow for output or stacked math, or restyle and hide a span
of buffer text the way concealment does.

This is not the feature hiding under another name. None of the four surfaces
above defines a function that attaches content to a buffer. The few names that
look related are configuration for Helix's own built in behaviour. `gutters`
chooses which built in gutters appear, and the `inline-diagnostics` options tune
how Helix shows its own diagnostics. Neither one lets a plugin supply content of
its own.

The capability already exists inside the editor. Inlay hints are built as inline
annotations and diagnostics are drawn by a dedicated decoration renderer, both
running on the `text_annotations` machinery in helix-core. Helix also shifts
those annotations as the buffer changes so they stay attached to the right
place. The machinery is present and it runs on every frame. It is simply not
reachable from a plugin.

## Why it matters

A notebook shows its results in place. The plot sits under the cell that produced
it, LaTeX becomes stacked math, and a token like `\alpha` appears as α. Each of
those needs in-buffer decoration, so the only way to ship them today is to patch
the editor itself, which is the fork the plugin system is meant to retire.

## What would close it

A decoration API that a plugin can attach to a view or a document, covering
inline annotations, virtual rows in the text flow, and style overlays over a
range of buffer text. The one firm requirement is that decorations move with
edits. Helix already does this for its own annotations, so a plugin's should
behave the same way. Since the underlying `text_annotations` support is already
there, this is a question of exposing it rather than building something new.

## Still open

Inline images rely on terminal graphics and are probably a larger, separate
request than text decoration, so they likely want their own thread. The concrete
shape of the API is also undecided, including whether decorations are addressed
by handle, whether they live per view or per document, and how a plugin clears
them.
