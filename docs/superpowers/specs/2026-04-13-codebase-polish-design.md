# nothelix codebase polish: restructuring + ergonomic errors

**Status:** design approved, executing
**Date:** 2026-04-13
**Author:** koalazub + Claude

## Context

nothelix is ~8600 lines across Steel (3800), Rust (2500), Julia (1300),
and bash (1000). The code works but grew organically: `execution.scm` is
1314 lines with 11 responsibilities, cells are ad-hoc lists instead of
structs, and Julia's raw error output (`BoundsError: attempt to access
3-element Vector{Int64} at index [0]` + 15-frame stacktrace) is dumped
verbatim into the output block.

The goal is twofold: make the codebase inviting to developers who will
modify it (phase 1), and make the error output inviting to researchers
who will use it (phase 2).

## Audience

- **Primary**: developers who read and modify the source — contributors,
  forks, maintainers. They need clean module boundaries, typed data
  structures, composable helpers, and docstrings that explain "why".
- **Secondary**: researchers who execute cells and see errors. They need
  Rust-quality error messages that guide them to fix their code, not
  Julia stack traces that require a CS degree to parse.

## Phase 1 — Steel plugin restructuring

### 1.1 Split execution.scm

Current: 1314 lines, 11 responsibilities. Target: 5 focused modules.

| New module | Extracted from | Responsibility |
|---|---|---|
| `cell-boundaries.scm` | execution.scm:86-214 | `find-cell-start-line`, `find-cell-code-end`, `find-output-start`, `find-output-end-line`, `extract-cell-code`, `line-blank?`, `expand-delete-*`, `find-last-non-blank-line-before`, `delete-line-range` |
| `cursor-restore.scm` | execution.scm:409-464 | `save-cursor-for-restore!`, `restore-cursor-for!`, `clear-cursor-restore!`, `*pending-cursor-restore*` |
| `image-cache.scm` | execution.scm:252-320 + 1183-1313 | `cell-index->image-id`, `maybe-clear-raw-content!`, `sync-images-to-markers!`, `sync-images-if-markers-changed!`, `render-cached-images`, `*image-marker-counts*`, `*rendered-image-docs*` |
| `output-insert.scm` | execution.scm:496-720 | `update-cell-output`, `commentify`, `schedule-reconceal` call — the output-insertion pipeline |
| `execution.scm` (slim) | remains | `execute-cell`, `execute-all-cells`, `execute-cell-list`, `execute-single-cell-async`, `cancel-cell`, `poll-for-result`, `handle-execution-error`, `*executing-kernel-dir*` — pure orchestration |

Each module `provide`s its public API and `require`s only what it
needs. The execution core imports cell-boundaries, cursor-restore,
image-cache, and output-insert. No circular dependencies.

### 1.2 Introduce Cell struct

Replace ad-hoc `(list line-idx kind-label cell-index header-text
user-label)` tuples with a proper Steel struct:

```scheme
(struct Cell
  (line-idx    ; 0-based line number in the rope
   kind        ; 'code | 'markdown
   index       ; integer from @cell N / @markdown N
   lang        ; "julia" | "python" | ...
   label       ; optional user label from "..." in marker
   header-text ; raw marker line text
  ) #:transparent)
```

All code that currently destructures cells via `(list-ref cell 2)`
switches to `(Cell-index cell)`. Mutable where needed via
`#:mutable` on specific fields.

Picker, scaffold, execution, and selection all consume `Cell`
through the struct interface. The parser (`parse-cell-header` in
picker.scm) returns `Cell` instances.

### 1.3 Consolidate mutable state

Current: 7 mutable globals scattered across files. Target: each
module owns its own state, no cross-module `set!` on another
module's globals.

| Global | Current location | Target module |
|---|---|---|
| `*pending-cursor-restore*` | execution.scm | cursor-restore.scm |
| `*image-marker-counts*` | execution.scm | image-cache.scm |
| `*rendered-image-docs*` | execution.scm | image-cache.scm |
| `*executing-kernel-dir*` | execution.scm | execution.scm (stays) |
| `*kernels*` | kernel.scm | kernel.scm (stays) |
| `*conceal-cache*` | conceal-state.scm | conceal-state.scm (stays) |
| `*spinner-frame*` | spinner.scm | spinner.scm (stays) |
| `*last-plot-data*` | chart-viewer.scm | chart-viewer.scm (stays) |

Each module exposes mutator functions (e.g.
`image-cache-invalidate!`) rather than exposing the raw hash.
Consumers never `set!` another module's global directly.

### 1.4 Deduplicate cell scanning

Current: 4 independent implementations of "walk lines looking for
cell markers" (execution.scm, picker.scm, scaffold.scm, selection.scm).

Target: one `scan-cells` function in `cell-boundaries.scm` that
returns a list of `Cell` structs. All consumers call it.

```scheme
;;@doc
;; Scan the document for all @cell and @markdown markers.
;; Returns a list of Cell structs in document order.
;; (-> (listof Cell))
(define (scan-cells)
  ...)
```

The picker's `get-all-cells`, scaffold's marker loop, and
execution's `find-cell-marker-by-index` all become thin wrappers
over `scan-cells` + `filter`.

### 1.5 Deduplicate polling loops

Current: two nearly identical polling chains
(`poll-for-result-with-delay` for single cells,
`poll-cell-list-result-with-delay` for cell lists).

Target: one `poll-kernel` helper that takes a continuation:

```scheme
;;@doc
;; Poll the kernel for a result with exponential backoff.
;; Calls (on-result result-json) when status != "pending".
;; (-> string? string? integer? (-> string? void) void)
(define (poll-kernel kernel-dir jl-path cell-index on-result)
  ...)
```

Both `execute-cell` and `execute-cell-list` pass different
continuations.

### 1.6 Docstrings on every public function

Target: 100% `;;@doc` coverage on all `provide`d functions.
`string-utils.scm` (currently 0%) gets full coverage. Each
docstring includes:
- One-line summary
- Parameter types (informal, `(-> input output)` style)
- Example where non-obvious

### 1.7 Architecture document

A `docs/ARCHITECTURE.md` that maps the module graph, explains the
execution flow (cell run → kernel → output insertion → image
registration → conceal refresh), and describes the data flow
between Steel, Rust, and Julia. Target audience: a developer who
just cloned the repo and wants to know where to look.

## Phase 2 — Ergonomic error reformatter

### 2.1 Architecture

Three layers, each doing what it's best at:

```
Julia kernel          Rust FFI              Steel plugin
─────────────        ──────────            ─────────────
catch exception  →   pattern-match    →   insert formatted
extract metadata     against hint         error into buffer
(type, message,      registry, render
frames, source)      Rust-style output
```

**Julia** (output_capture.jl): catches the exception object, extracts
structured metadata:

```json
{
  "error_type": "BoundsError",
  "message": "attempt to access 3-element Vector{Int64} at index [0]",
  "frames": [
    {"file": "<cell>", "line": 3, "func": "top-level scope"},
    {"file": "array.jl", "line": 861, "func": "getindex"}
  ],
  "source_line": "v[0]",
  "source_col": 2,
  "cell_index": 2,
  "cell_line": 3
}
```

**Rust** (new `error_format.rs`): reads the structured JSON, matches
against a hint registry, and produces formatted output:

```
error[E001]: index out of bounds
  --> cell 2, line 3
   |
 3 | v[0]
   |   ^ index 0 is out of range for a 3-element array
   |
   = help: Julia arrays are 1-indexed. Use v[1] for the first element.
```

**Steel** (output-insert.scm): receives the formatted string from
Rust via FFI and inserts it into the buffer as commented lines,
replacing the raw Julia error dump.

### 2.2 Structured error extraction (Julia)

Modify `capture_toplevel` in `output_capture.jl` to produce
structured error metadata instead of (or alongside) the raw
`sprint(showerror, e)` string.

```julia
struct StructuredError
    error_type::String      # "BoundsError", "UndefVarError", etc.
    message::String         # human-readable message
    frames::Vector{ErrorFrame}
    source_line::String     # the line of user code that errored
    source_col::Int         # column (0-based) if available
    cell_index::Int         # which @cell N
    cell_line::Int          # line within the cell (1-based)
end

struct ErrorFrame
    file::String
    line::Int
    func::String
    is_user_code::Bool      # true if from <cell>, false if from Base/stdlib
end
```

The kernel returns this as a `"structured_error"` field in the JSON
response. The existing `"error"` field keeps the raw string as a
fallback for errors the Rust formatter doesn't handle.

### 2.3 Hint registry (Rust)

A `Vec<ErrorHint>` loaded at startup from a TOML file
(`error_hints.toml`) bundled with libnothelix:

```toml
[[hint]]
id = "E001"
pattern = "BoundsError.*index \\[0\\]"
title = "index out of bounds"
help = "Julia arrays are 1-indexed. Use v[1] for the first element."
severity = "error"

[[hint]]
id = "E002"
pattern = "UndefVarError: (\\w+) not defined"
title = "undefined variable: {1}"
help = "Check spelling, or make sure the variable is defined in an earlier cell."
severity = "error"

[[hint]]
id = "E003"
pattern = "MethodError: no method matching \\+(::String"
title = "cannot add string and number"
help = "Use string() to convert, or * to concatenate strings."
severity = "error"

[[hint]]
id = "E010"
pattern = "Pkg\\.add.*Resolving package versions"
title = "package installation noise"
help = ""
severity = "suppress"
```

The `severity = "suppress"` hint replaces the current ad-hoc
Pkg-noise filtering in execution.scm with a data-driven approach.

Contributors add hints by editing the TOML file — no Rust or Steel
knowledge required. The Rust side compiles the regex patterns at
startup and matches against incoming errors.

### 2.4 Rust formatter

New `error_format.rs` module in libnothelix:

```rust
pub struct ErrorHint {
    pub id: String,
    pub pattern: Regex,
    pub title: String,      // supports {N} capture group refs
    pub help: String,
    pub severity: Severity, // Error, Warning, Suppress
}

pub fn format_error(
    structured: &StructuredError,
    hints: &[ErrorHint],
) -> String {
    // 1. Match error against hints
    // 2. Render Rust-style output with cell+line attribution
    // 3. Filter stack frames to user code only
    // 4. Fallback to cleaned-up raw error if no hint matches
}
```

Exposed to Steel via FFI as `format-julia-error`.

### 2.5 Stack trace filtering

Only show frames where `is_user_code == true` (file is `<cell>`
or matches the notebook path). Internal Julia frames
(`Base`, `Core`, `stdlib`) are collapsed into `... N internal
frames ...`.

### 2.6 Fallback for unrecognized errors

If no hint matches and the error doesn't have structured metadata:

```
error: execution failed
  --> cell 2
   |
   | BoundsError: attempt to access 3-element Vector{Int64} at index [0]
   |
   = note: raw Julia error (no specific hint available)
   = note: consider filing an issue to add a hint for this error type
```

Still better than the raw dump: cell-attributed, no internal
frames, clear labelling of what happened.

### 2.7 Initial hint set (~15 patterns)

| ID | Pattern | Title | Help |
|---|---|---|---|
| E001 | `BoundsError.*index \[0\]` | index out of bounds (0-indexed) | Julia is 1-indexed. Use v[1]. |
| E002 | `BoundsError` (generic) | index out of bounds | Check array length with `length()`. |
| E003 | `UndefVarError: (\w+)` | undefined variable: {1} | Check spelling or define in earlier cell. |
| E004 | `MethodError: no method matching (\w+)\(` | no method `{1}` for these types | Check argument types with `typeof()`. |
| E005 | `MethodError.*\+(::String` | can't add string and number | Use `string()` or `*` to concatenate. |
| E006 | `DimensionMismatch` | matrix dimension mismatch | Check sizes with `size()`. |
| E007 | `SingularException` | matrix is singular | Matrix is not invertible. Check with `det()`. |
| E008 | `DomainError` | math domain error | Argument outside valid domain (e.g. sqrt of negative). |
| E009 | `ArgumentError: Package (\w+) not found` | package {1} not installed | Run `using Pkg; Pkg.add("{1}")`. |
| E010 | `StackOverflowError` | infinite recursion | Check base case in recursive function. |
| E011 | `TypeError: non-boolean.*if` | non-boolean in if condition | Condition must be `true`/`false`, not a number. |
| E012 | `LoadError` | file load error | Check file path and syntax. |
| E013 | `Resolving package versions` | (suppress) | Pkg noise. |
| E014 | `No packages added` | (suppress) | Pkg noise. |
| E015 | `Precompiling` (no error) | (suppress) | Pkg noise. |

## Breakage tolerance

Full. Atomic commits that temporarily break imports are acceptable.
The test suite validates correctness after each phase lands. No
incremental migration — split execution.scm in one commit, update
all imports in the same commit.

## Constraints

All Steel code MUST conform to the Steel language specification at
https://mattwparas.github.io/steel/book/. Specifically:
- Structs via `(struct Name (fields) #:transparent)` or `#:mutable`
- Hash maps via `(hash key val ...)`, accessed with `hash-get`,
  `hash-insert`, `hash-remove`, `hash-contains?`
- Modules via `(require "path")` and `(provide name ...)`
- Contracts via `define/contract` where interfaces are public
- Transducers for collection transforms (`mapping`, `filtering`,
  `transduce`, `into-list`)
- No `string-index-of` (doesn't exist — use `string-split` or
  manual scan)
- Named let loops `(let name ([var init] ...) body)` for iteration
- `set!` for mutation (with `!` suffix on the function name)

## Success criteria

**Phase 1:**
- execution.scm < 300 lines
- Every public function has `;;@doc`
- Zero ad-hoc list destructuring for cells (all via Cell struct)
- All existing bats + cargo tests still pass
- `docs/ARCHITECTURE.md` exists and covers the module graph

**Phase 2:**
- `BoundsError` at index 0 renders as a Rust-style guided message
- 15 initial hints in `error_hints.toml`
- Hint contributors edit TOML, not code
- Stack traces show only user frames
- Unrecognized errors still render cleanly (cell-attributed, no
  internal frames)
- All existing tests still pass + new tests for each hint

## Dependencies

- Phase 1 has no external dependencies (pure refactor)
- Phase 2 requires:
  - `regex` crate in libnothelix (for hint pattern matching)
  - `toml` crate in libnothelix (for loading hint registry)
  - Changes to `output_capture.jl` (structured error extraction)
  - New `error_format.rs` in libnothelix
  - New FFI function `format-julia-error` exposed to Steel
