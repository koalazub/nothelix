# State visibility architecture

A persistent kernel is what makes notebooks fast, and it is also what makes
them lie. The session accumulates globals as cells run in whatever order the
researcher ran them, while the document on screen implies top-to-bottom
order. When the two diverge, a cell can read a value the document says it
should not have, and the output looks authoritative while being wrong. This
document defines how nothelix makes that divergence visible without changing
execution semantics.

Reactive notebooks solve this by re-running dependents automatically and
forbidding redefinition. That is a different product. Nothelix keeps Jupyter
semantics, a cell runs against the session as it is, and instead makes the
session state a rendered, inspectable thing.

## Layer 1, the state ledger

The kernel records, for every top-level global, the cell index that last
assigned it, the source hash of that cell at the time, and a timestamp. Every
execution also records the cell's read set and write set, derived from the
AST analysis the kernel already performs. Only top-level assignments count.
Locals, imports, and names a cell assigns before reading are excluded so the
ledger stays free of false alarms.

## Layer 2, the status classifier

The classifier joins the ledger against a static scan of the current
document and assigns each cell one state.

| State | Meaning |
|---|---|
| fresh | Every input was written by a cell above, unchanged since it ran |
| out-of-order | An input's last writer sits below this cell in the document |
| stale-input | A writer cell re-ran or was edited after this cell last ran |
| orphan-input | The session holds a value no cell in the document assigns anymore |
| edited-since-run | The cell's own source changed after its stored output |

## Layer 3, the surfaces

Nothing here asks the researcher to run a query. The stale-tags virtual
layer marks the cell header, for example a small annotation reading
`uses A from cell 76, below`. Output blocks whose state is anything other
than fresh render their gutter bars dimmed, so a stale result looks stale
from across the room. The cell picker shows a status glyph per row, which
makes it a whole-notebook freshness overview. A `:cell-state` command
prints the full provenance table for the cell under the cursor, each input
with its writer and freshness.

## Layer 4, the repair affordances

Every warning names its own exit. An out-of-order note points at
`execute-cells-above` and its binding. A `:rebuild-inputs` command runs the
transitive writer chain for the current cell, computed from recorded write
sets, which is the minimal deterministic rebuild. The `.nothelix.conf` key
`state-guard` takes `off`, `note`, or `block`. Under `block`, a single-cell
run with out-of-order inputs refuses and explains, for researchers who want
the guardrail.

## Delivery phases

Phase 1 ships the ledger and the out-of-order note in cell output. Phase 2
ships the classifier, the header annotations, dimmed bars, and the picker
glyphs. Phase 3 ships `:rebuild-inputs` and the `state-guard` knob. Each
phase is useful alone and none changes what a cell executes.
