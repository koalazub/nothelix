# Performance + Cross-Cell Error Guidance Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Eliminate hot-path allocations across the Rust FFI layer, fix Julia kernel type instability and I/O waste, and add cross-cell error context so errors tell users *which cell to run* instead of just "not defined."

**Architecture:** Three independent tracks executed in parallel: (A) Rust perf — cache hints via OnceLock, replace format!() loops with write!(), eliminate per-call JSON reparsing; (B) Julia kernel perf — persistent log handle, const plot patterns, reverse dependency index, typed struct fields; (C) Cross-cell error guidance — extend structured_error with cell registry context (which cells define the missing variable, their execution status), render as `note:` section in the Rust formatter.

**Tech Stack:** Rust (std::sync::OnceLock, std::fmt::Write), Julia (typed structs, const globals, IOStream), Steel/Scheme (output-insert.scm passes new data through FFI)

---

## Track A: Rust Performance

### Task 1: Cache hints with OnceLock

`load_hints()` parses the embedded TOML on every `format_error()` call. Parse once, reuse forever.

**Files:**
- Modify: `libnothelix/src/error_format.rs:47-61`

- [ ] **Step 1: Write test that hints are loaded identically across calls**

```rust
// In error_format.rs tests
#[test]
fn hints_cached_identity() {
    let a = hints();
    let b = hints();
    assert!(std::ptr::eq(a.as_slice(), b.as_slice()));
}
```

- [ ] **Step 2: Run test, verify it fails**

Run: `nix shell nixpkgs#cargo-nextest --command cargo nextest run -E 'test(hints_cached)'`
Expected: FAIL — `hints()` doesn't exist yet.

- [ ] **Step 3: Replace load_hints with OnceLock**

```rust
use std::sync::OnceLock;

static HINTS: OnceLock<Vec<ErrorHint>> = OnceLock::new();

fn hints() -> &'static [ErrorHint] {
    HINTS.get_or_init(|| {
        let file: HintsFile = toml::from_str(HINTS_TOML).unwrap_or(HintsFile { hint: vec![] });
        file.hint
            .into_iter()
            .map(|h| ErrorHint {
                id: h.id,
                match_type: h.match_type,
                match_tokens: h.match_tokens,
                exclude_tokens: h.exclude_tokens,
                title: h.title,
                help: h.help,
                example: h.example,
            })
            .collect()
    })
}
```

Update `format_error`, `format_structured`, `format_raw` signatures to accept `&[ErrorHint]` from `hints()` instead of calling `load_hints()`. Remove the `pub fn load_hints()` entirely.

- [ ] **Step 4: Run all tests, verify pass**

Run: `nix shell nixpkgs#cargo-nextest --command cargo nextest run`
Expected: All pass.

- [ ] **Step 5: Run clippy, verify clean**

Run: `cargo clippy`
Expected: No warnings.

- [ ] **Step 6: Commit**

```
jj desc -m "perf: cache error hints with OnceLock — parse TOML once not per-error"
```

---

### Task 2: Replace format!() with write!() in Kitty chunk loops

Both `graphics.rs` and `kitty_placeholder.rs` build format strings per 4KB chunk inside loops. Use `write!()` directly into the pre-allocated `String`.

**Files:**
- Modify: `libnothelix/src/graphics.rs:39-50`
- Modify: `libnothelix/src/kitty_placeholder.rs:140-157`

- [ ] **Step 1: Run existing tests as baseline**

Run: `nix shell nixpkgs#cargo-nextest --command cargo nextest run -E 'test(kitty) | test(graphics)'`
Expected: All pass.

- [ ] **Step 2: Rewrite graphics.rs chunk loop**

```rust
use std::fmt::Write;

pub fn kitty_escape_for_b64_png(b64: &str, image_id: u32, rows: u32) -> String {
    let bytes = b64.as_bytes();
    let chunk_size = 4096;
    let total = (bytes.len() + chunk_size - 1) / chunk_size;
    let mut out = String::with_capacity(b64.len() + total * 64);

    for (i, chunk) in bytes.chunks(chunk_size).enumerate() {
        let s = std::str::from_utf8(chunk).unwrap_or("");
        let more = if i < total - 1 { 1 } else { 0 };
        if i == 0 {
            let _ = write!(out, "\x1b_Ga=T,f=100,t=d,q=2,I={image_id},r={rows},m={more};{s}\x1b\\");
        } else {
            let _ = write!(out, "\x1b_Gm={more};{s}\x1b\\");
        }
    }
    out
}
```

Also remove the intermediate `chunks.collect::<Vec<_>>()` — iterate directly.

- [ ] **Step 3: Apply same pattern to kitty_placeholder.rs build_virtual_transmission**

Same change: replace `format!()` + `push_str` with `write!()` directly. Remove the `chunks.collect::<Vec<_>>()`.

- [ ] **Step 4: Run tests, verify pass**

Run: `nix shell nixpkgs#cargo-nextest --command cargo nextest run -E 'test(kitty) | test(graphics)'`
Expected: All pass.

- [ ] **Step 5: Run clippy, commit**

```
jj desc -m "perf: use write!() instead of format!() in Kitty chunk loops"
```

---

### Task 3: Eliminate per-field JSON reparsing in output-insert.scm

The Steel layer calls `json-get` 5-6 times on the same result JSON string. Each call re-parses the entire JSON via serde. Add a `json-get-many` FFI function that parses once and extracts multiple fields.

**Files:**
- Modify: `libnothelix/src/json_utils.rs` — add `json_get_many`
- Modify: `libnothelix/src/lib.rs` — register FFI
- Modify: `plugin/nothelix/output-insert.scm:146-162` — use `json-get-many`

- [ ] **Step 1: Write test for json_get_many**

```rust
#[test]
fn json_get_many_extracts_multiple() {
    let json = r#"{"error": "boom", "stdout": "hi", "stderr": "", "has_error": true}"#;
    let result = json_get_many(json.into(), "error,stdout,stderr,has_error".into());
    // Returns tab-separated values
    assert_eq!(result, "boom\thi\t\ttrue");
}
```

- [ ] **Step 2: Run test, verify fail**

- [ ] **Step 3: Implement json_get_many**

```rust
pub fn json_get_many(json_str: String, keys_csv: String) -> String {
    let keys: Vec<&str> = keys_csv.split(',').collect();
    let parsed = match serde_json::from_str::<Value>(&json_str) {
        Ok(v) => v,
        Err(_) => return "\t".repeat(keys.len().saturating_sub(1)),
    };
    keys.iter()
        .map(|key| {
            parsed.get(key.trim()).map_or(String::new(), |val| match val {
                Value::String(s) => s.clone(),
                Value::Bool(b) => b.to_string(),
                Value::Number(n) => n.to_string(),
                Value::Null => String::new(),
                other => other.to_string(),
            })
        })
        .collect::<Vec<_>>()
        .join("\t")
}
```

Register in `lib.rs`:
```rust
m.register_fn("json-get-many", json_utils::json_get_many);
```

- [ ] **Step 4: Update output-insert.scm to use json-get-many**

Replace the 5 separate `json-get` calls (lines 146, 151, 159-162) with a single `json-get-many` call + tab-split:

```scheme
(define fields (string-split (json-get-many result-json "error,structured_error,output_repr,stdout,stderr,has_error") "\t"))
(define err (list-ref fields 0))
(define structured (list-ref fields 1))
(define output-repr (list-ref fields 2))
(define stdout-text (list-ref fields 3))
(define stderr-text (list-ref fields 4))
(define has-error (equal? (list-ref fields 5) "true"))
```

- [ ] **Step 5: Run tests + manual smoke test, commit**

```
jj desc -m "perf: json-get-many parses JSON once instead of 6 times per cell result"
```

---

## Track B: Julia Kernel Performance

### Task 4: Persistent log file handle

`log_msg()` opens and closes the log file on every call (27+ per command). Keep a persistent `IOStream`.

**Files:**
- Modify: `kernel/runner.jl:14-25`

- [ ] **Step 1: Replace open/close with persistent handle**

```julia
const LOG_IO = Ref{Union{IOStream, Nothing}}(nothing)

function ensure_log_io()
    if LOG_IO[] === nothing || !isopen(LOG_IO[])
        LOG_IO[] = open(LOG_FILE, "a")
    end
    LOG_IO[]
end

function log_msg(level::Symbol, msg::String)
    io = ensure_log_io()
    timestamp = Dates.format(now(), "yyyy-mm-dd HH:MM:SS.sss")
    println(io, "[$timestamp] [$level] $msg")
    flush(io)
end
```

- [ ] **Step 2: Add cleanup on kernel shutdown**

In the main loop's `finally` block (or atexit), add:
```julia
atexit() do
    io = LOG_IO[]
    io !== nothing && isopen(io) && close(io)
end
```

- [ ] **Step 3: Test by starting kernel and verifying log output**

- [ ] **Step 4: Commit**

```
jj desc -m "perf: persistent log file handle — eliminate 27+ open/close per command"
```

---

### Task 5: Const plot detection patterns + type-based checks

`is_displayable_plot()` allocates a fresh array and does string(typeof) on every call.

**Files:**
- Modify: `kernel/output_capture.jl:175-189`

- [ ] **Step 1: Extract const patterns, use type-name caching**

```julia
const PLOT_TYPE_PATTERNS = ("Plot", "Figure", "Scene", "FigureAxis", "Chart", "Canvas", "Drawing", "GtkCanvas")

function is_displayable_plot(x)
    x === nothing && return false
    T = typeof(x)
    # Fast path: check common concrete types by module
    mod = parentmodule(T)
    mod_name = nameof(mod)
    mod_name === :Plots && return true
    mod_name === :Makie && return true
    mod_name === :CairoMakie && return true
    mod_name === :GLMakie && return true
    # Fallback: string check for less common types
    t = string(nameof(T))
    any(p -> occursin(p, t), PLOT_TYPE_PATTERNS)
end
```

- [ ] **Step 2: Apply same fix to get_output_type in cell_macros.jl:170-187**

Replace `occursin("DataFrame", string(typeof(x)))` with:
```julia
hasproperty(x, :columns) && hasproperty(x, :colindex) && return "dataframe"
```

- [ ] **Step 3: Commit**

```
jj desc -m "perf: const plot patterns + module-based type dispatch"
```

---

### Task 6: Reverse dependency index for get_dependents

Replace the O(n×m) nested loop with an O(1) reverse lookup.

**Files:**
- Modify: `kernel/cell_registry.jl:28,50-65`

- [ ] **Step 1: Add reverse index**

```julia
# Add alongside existing globals
const VARIABLE_USERS = Dict{Symbol, Set{Int}}()  # var → set of cell indices that use it
```

- [ ] **Step 2: Update cell registration to maintain reverse index**

In whatever function registers cell uses (likely in `cell_macros.jl` where `cell.uses` is set), add:
```julia
for var in cell.uses
    users = get!(Set{Int}, CellRegistry.VARIABLE_USERS, var)
    push!(users, cell_idx)
end
```

- [ ] **Step 3: Rewrite get_dependents using reverse index**

```julia
function get_dependents(cell_idx::Int)::Vector{Int}
    !haskey(CELLS, cell_idx) && return Int[]
    cell = CELLS[cell_idx]
    dependents = Set{Int}()
    for var in cell.defines
        if haskey(VARIABLE_USERS, var)
            union!(dependents, VARIABLE_USERS[var])
        end
    end
    delete!(dependents, cell_idx)
    sort!(collect(dependents))
end
```

- [ ] **Step 4: Update clear_registry to clear VARIABLE_USERS**

- [ ] **Step 5: Commit**

```
jj desc -m "perf: O(1) reverse dependency lookup via VARIABLE_USERS index"
```

---

## Track C: Cross-Cell Error Guidance

The key insight: when a user gets `UndefVarError: X not defined`, the kernel already knows which cell defines `X` and whether that cell has been executed. We should surface this as actionable guidance: *"variable X is defined in @cell 4 (status: pending — run it first)"*.

This is not about any specific variable. It's about the error formatter understanding the notebook's execution context — the dependency graph, the cell execution states — and using that to produce *guidance* that respects the structure of the codebase being edited.

### Task 7: Extend structured_error with cell context from the registry

When `extract_structured_error` runs, it already has access to the cell registry. Add a `cell_context` field that maps undefined variables to their defining cells and execution status.

**Files:**
- Modify: `kernel/output_capture.jl:43-84` — `extract_structured_error()`
- Modify: `kernel/cell_registry.jl` — add `lookup_variable_context()`

- [ ] **Step 1: Add context lookup to cell_registry.jl**

```julia
"""
For a set of variable names, return context about where they're defined
and whether those cells have been executed.
Returns: Dict{String, Dict{String,Any}} mapping var_name → context
"""
function lookup_variable_context(var_names::Set{Symbol})::Dict{String, Any}
    context = Dict{String, Any}()
    for var in var_names
        if haskey(VARIABLE_SOURCES, var)
            src_idx = VARIABLE_SOURCES[var]
            if haskey(CELLS, src_idx)
                src_cell = CELLS[src_idx]
                context[string(var)] = Dict{String, Any}(
                    "defined_in_cell" => src_idx,
                    "cell_status" => string(src_cell.status),
                    "was_executed" => src_cell.status in (:done, :error),
                )
            end
        end
    end
    context
end
```

- [ ] **Step 2: Wire into extract_structured_error**

After building the basic structured error dict, add cell context for `UndefVarError`:

```julia
# In extract_structured_error(), after line 82:
# Add cross-cell context for errors that reference variables
if error isa UndefVarError
    var_sym = error.var
    cell_context = CellRegistry.lookup_variable_context(Set([var_sym]))
    if !isempty(cell_context)
        result["cell_context"] = cell_context
    end
end

# For MethodError, check if any argument types suggest unexecuted deps
if error isa MethodError
    # Check if the current cell has unexecuted dependencies
    if haskey(CellRegistry.CELLS, cell_index)
        deps = CellRegistry.get_dependencies(cell_index)
        unexecuted = filter(d -> haskey(CellRegistry.CELLS, d) &&
            CellRegistry.CELLS[d].status ∉ (:done,), deps)
        if !isempty(unexecuted)
            result["unexecuted_deps"] = unexecuted
        end
    end
end
```

- [ ] **Step 3: Commit**

```
jj desc -m "feat: structured_error includes cell registry context for cross-cell guidance"
```

---

### Task 8: Extend Rust StructuredError to accept cell context

**Files:**
- Modify: `libnothelix/src/error_format.rs:65-91` — StructuredError + new types
- Modify: `libnothelix/src/error_format.rs` — format_structured rendering

- [ ] **Step 1: Add context types to StructuredError**

```rust
#[derive(Deserialize, Default)]
pub struct StructuredError {
    #[serde(default)]
    pub error_type: String,
    #[serde(default)]
    pub message: String,
    #[serde(default)]
    pub frames: Vec<ErrorFrame>,
    #[serde(default)]
    pub source_line: String,
    #[serde(default)]
    pub cell_index: i64,
    #[serde(default)]
    pub cell_line: i64,
    // NEW: cross-cell context
    #[serde(default)]
    pub cell_context: std::collections::HashMap<String, VarContext>,
    #[serde(default)]
    pub unexecuted_deps: Vec<i64>,
}

#[derive(Deserialize, Default)]
pub struct VarContext {
    #[serde(default)]
    pub defined_in_cell: i64,
    #[serde(default)]
    pub cell_status: String,
    #[serde(default)]
    pub was_executed: bool,
}
```

- [ ] **Step 2: Write test for cross-cell rendering**

```rust
#[test]
fn structured_error_with_cell_context() {
    let json = r#"{
        "error_type": "UndefVarError",
        "message": "`data` not defined",
        "frames": [],
        "source_line": "result = norm(data)",
        "cell_index": 5,
        "cell_line": 3,
        "cell_context": {
            "data": {
                "defined_in_cell": 2,
                "cell_status": "pending",
                "was_executed": false
            }
        }
    }"#;
    let out = format_error(json, "");
    assert!(out.contains("@cell 2"), "got:\n{out}");
    assert!(out.contains("pending"), "got:\n{out}");
}
```

- [ ] **Step 3: Run test, verify fail**

- [ ] **Step 4: Add rendering in format_structured**

After the help/example section, before the call chain, insert:

```rust
// ── Cross-cell context (note: section) ──
if !err.cell_context.is_empty() {
    out.push_str("   |\n");
    for (var, ctx) in &err.cell_context {
        if ctx.was_executed {
            out.push_str(&format!(
                "   = note: `{}` is defined in @cell {} (executed, status: {})\n",
                var, ctx.defined_in_cell, ctx.cell_status
            ));
            out.push_str("   = note: the cell ran but the variable may have been overwritten or errored\n");
        } else {
            out.push_str(&format!(
                "   = note: `{}` is defined in @cell {} — but it hasn't been executed yet\n",
                var, ctx.defined_in_cell
            ));
            out.push_str("   = help: run @cell ");
            out.push_str(&ctx.defined_in_cell.to_string());
            out.push_str(" first, or use :execute-cells-above to run all preceding cells\n");
        }
    }
}

if !err.unexecuted_deps.is_empty() {
    out.push_str("   |\n");
    let cells: Vec<String> = err.unexecuted_deps.iter().map(|c| format!("@cell {c}")).collect();
    out.push_str(&format!(
        "   = note: this cell depends on {} which haven't been executed\n",
        cells.join(", ")
    ));
}
```

- [ ] **Step 5: Run tests, verify pass**

- [ ] **Step 6: Commit**

```
jj desc -m "feat: render cross-cell guidance — show which cell defines a variable and its status"
```

---

### Task 9: Handle generic context — unexecuted dependencies, stale cells

Beyond `UndefVarError`, extend guidance to any error where the current cell has unexecuted dependencies. This catches scenarios like: cell 6 uses a function defined in cell 3, but cell 3 hasn't been run.

**Files:**
- Modify: `kernel/output_capture.jl` — extend to all error types
- Modify: `kernel/cell_registry.jl` — add `stale_dependencies()` check

- [ ] **Step 1: Add stale dependency detection**

In `cell_registry.jl`:

```julia
"""
Return cell indices that this cell depends on but haven't been executed
(or were executed before their own dependencies changed).
"""
function unexecuted_dependencies(cell_idx::Int)::Vector{Int}
    deps = get_dependencies(cell_idx)
    filter(d -> haskey(CELLS, d) && CELLS[d].status ∉ (:done,), deps)
end
```

- [ ] **Step 2: Always include unexecuted deps in structured_error**

In `extract_structured_error`, regardless of error type:

```julia
# Always check for unexecuted dependencies — this is useful context for any error
unexec = CellRegistry.unexecuted_dependencies(cell_index)
if !isempty(unexec)
    result["unexecuted_deps"] = unexec
end
```

- [ ] **Step 3: Commit**

```
jj desc -m "feat: surface unexecuted dependencies for any error, not just UndefVarError"
```

---

## Squash & Push

### Task 10: Final verification and push

- [ ] **Step 1: Run full test suite**

```
nix shell nixpkgs#cargo-nextest --command cargo nextest run
```

- [ ] **Step 2: Run clippy**

```
cargo clippy
```

- [ ] **Step 3: Squash track commits and push**

```
jj squash
jj desc -m "feat: perf overhaul + cross-cell error guidance

Performance:
- Cache error hints with OnceLock (parse TOML once, not per-error)
- Replace format!() with write!() in Kitty chunk loops
- json-get-many parses JSON once instead of 6× per cell result
- Persistent log file handle (eliminate 27+ open/close per command)
- Const plot patterns + module-based type dispatch
- O(1) reverse dependency lookup via VARIABLE_USERS index

Cross-cell error guidance:
- Structured errors include cell registry context
- UndefVarError shows which cell defines the variable and its status
- Any error shows unexecuted dependency cells
- Rendered as = note: / = help: sections in Rust-compiler-style output"
jj bookmark set error-format-overhaul
jj git push --bookmark error-format-overhaul
```
