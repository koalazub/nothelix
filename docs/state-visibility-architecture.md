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

| Phase | Scope | Status |
|---|---|---|
| Phase 1 | The ledger and the out-of-order note in cell output | Shipped |
| Phase 2 | The classifier, the header annotations, dimmed bars, the picker glyphs, and `:cell-state` | Shipped |
| Phase 3 | `:rebuild-inputs` and the `state-guard` knob | Planned |

Each phase is useful alone and none changes what a cell executes.

## Phase 2 wire format

The kernel classifies every executed cell in one pass over its own ledger
(`CellRegistry.classify_all`, kernel/cell_registry.jl) and emits the result
as a `cell_states` object on the execute response, keyed by cell index. Each
entry carries a `state` and the list of tracked `inputs`, every input a
`{name, writer, rel}` triple where `rel` is one of `below`, `stale`,
`orphan`, or `fresh`. The classifier reads a new per-cell `run_seq` (a
monotonic run counter) so it can tell that an upstream writer re-ran after
its reader, which is the `stale-input` case. `out-of-order` is a writer whose
index sits below the reader, and `orphan-input` is a writer whose current
code no longer assigns the variable.

The plugin reads the whole map with one FFI call, `json-get-cell-states`
(libnothelix/src/json_utils/fields.rs), which flattens the object to one
tab-separated line per cell, inputs joined by `;`. It caches the result
(plugin/nothelix/cell-state.scm) and folds `edited-since-run` on top, which
is a plugin-side comparison of the cell's current source hash against the
hash its stored output was keyed under. That one state is plugin-side because
only the plugin sees un-run edits, so the kernel classifier owns the four
runtime states and the plugin owns the edited state.

The classification is O(cells times reads) and runs off the execution hot
path, after a cell's result is already rendered, so it never delays a run.
Header tags are virtual annotations on the marker lines and cost no rope
edits, so re-tagging the whole notebook after each run stays cheap. A cell
classified non-fresh renders its output gutter bars with the dimmed
`ui.virtual.output.stale` scope instead of the cycling series colors.
