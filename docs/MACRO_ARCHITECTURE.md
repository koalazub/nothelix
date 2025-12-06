# Nothelix Macro Architecture (IMPLEMENTED)

> **Status**: ✅ This design has been fully implemented. This document serves as reference.

## Overview

Nothelix leverages Julia's macro system for cell management, dependency tracking, and reactive execution - similar to Marimo for Python.

## Current Architecture (Problems)

```
Helix/Steel                    File I/O                     Julia Kernel
    |                             |                              |
    |-- write code to input.jl -->|                              |
    |                             |-- kernel polls input.jl ---->|
    |                             |                              |-- execute raw code
    |                             |<-- write to output.txt ------|
    |<-- poll for output.txt.done |                              |
```

**Issues:**
- `@cell` and `@markdown` are no-op macros (just parsing markers)
- No dependency tracking between cells
- No automatic reactivity
- File-based IPC is slow and fragile
- Kernel has no awareness of cell structure

## New Architecture

```
Helix/Steel                    Commands                      Julia Kernel
    |                             |                              |
    |-- "register_cell(1, code)" ->                              |
    |                             |-- @cell macro executes ----->|
    |                             |                              |-- registers in CELL_REGISTRY
    |                             |                              |-- extracts dependencies
    |                             |                              |-- executes code
    |                             |                              |-- captures output
    |<-- JSON result with deps ---|<-- notify completion --------|
    |                             |                              |
    |-- "execute_cell(1)" ------->|                              |
    |                             |-- triggers dependent cells -->|
```

## Components

### 1. Julia Cell Registry (kernel/cell_registry.jl)

```julia
module CellRegistry

using Base: Meta

# Cell state structure
mutable struct Cell
    index::Int
    exec_count::Union{Int, Nothing}
    code::Expr
    defines::Set{Symbol}      # Variables this cell defines
    uses::Set{Symbol}         # Variables this cell uses
    outputs::Any
    stdout::String
    stderr::String
    error::Union{Exception, Nothing}
    status::Symbol            # :pending, :running, :done, :error
end

# Global registry
const CELLS = Dict{Int, Cell}()
const VARIABLE_SOURCES = Dict{Symbol, Int}()  # Which cell defines each variable

# Dependency graph
function get_dependencies(cell_idx::Int)::Vector{Int}
    cell = CELLS[cell_idx]
    deps = Int[]
    for var in cell.uses
        if haskey(VARIABLE_SOURCES, var) && VARIABLE_SOURCES[var] != cell_idx
            push!(deps, VARIABLE_SOURCES[var])
        end
    end
    sort!(unique!(deps))
end

# Get cells that depend on this cell
function get_dependents(cell_idx::Int)::Vector{Int}
    cell = CELLS[cell_idx]
    dependents = Int[]
    for (var, source_idx) in VARIABLE_SOURCES
        if source_idx == cell_idx
            for (idx, other_cell) in CELLS
                if idx != cell_idx && var in other_cell.uses
                    push!(dependents, idx)
                end
            end
        end
    end
    sort!(unique!(dependents))
end

export Cell, CELLS, VARIABLE_SOURCES, get_dependencies, get_dependents

end
```

### 2. AST Analysis (kernel/ast_analysis.jl)

```julia
module ASTAnalysis

# Extract variables defined by an expression
function extract_defines(expr::Expr)::Set{Symbol}
    defines = Set{Symbol}()
    _extract_defines!(defines, expr)
    defines
end

function _extract_defines!(defines::Set{Symbol}, expr)
    if expr isa Expr
        if expr.head == :(=) && expr.args[1] isa Symbol
            push!(defines, expr.args[1])
        elseif expr.head == :function && expr.args[1] isa Expr
            # Function definition
            func_name = expr.args[1].args[1]
            if func_name isa Symbol
                push!(defines, func_name)
            end
        elseif expr.head in (:local, :global)
            for arg in expr.args
                if arg isa Symbol
                    push!(defines, arg)
                elseif arg isa Expr && arg.head == :(=)
                    push!(defines, arg.args[1])
                end
            end
        end
        for arg in expr.args
            _extract_defines!(defines, arg)
        end
    end
end

# Extract variables used by an expression
function extract_uses(expr::Expr)::Set{Symbol}
    uses = Set{Symbol}()
    defines = Set{Symbol}()  # Track local defines to exclude
    _extract_uses!(uses, defines, expr)
    setdiff(uses, defines)  # Remove locally defined variables
end

function _extract_uses!(uses::Set{Symbol}, defines::Set{Symbol}, expr)
    if expr isa Symbol
        # Check if it's a variable reference (not a built-in)
        if !isdefined(Base, expr) && !isdefined(Core, expr)
            push!(uses, expr)
        end
    elseif expr isa Expr
        if expr.head == :(=) && expr.args[1] isa Symbol
            push!(defines, expr.args[1])
            _extract_uses!(uses, defines, expr.args[2])
        else
            for arg in expr.args
                _extract_uses!(uses, defines, arg)
            end
        end
    end
end

export extract_defines, extract_uses

end
```

### 3. Output Capture (kernel/output_capture.jl)

```julia
module OutputCapture

using Base64

mutable struct CapturedOutput
    return_value::Any
    stdout::String
    stderr::String
    images::Vector{Tuple{String, Vector{UInt8}}}  # (format, data)
end

function capture_execution(f)
    # Capture stdout/stderr
    stdout_io = IOBuffer()
    stderr_io = IOBuffer()

    result = CapturedOutput(nothing, "", "", [])

    try
        result.return_value = redirect_stdio(stdout=stdout_io, stderr=stderr_io) do
            f()
        end
    catch e
        result.return_value = e
        rethrow(e)
    finally
        result.stdout = String(take!(stdout_io))
        result.stderr = String(take!(stderr_io))
    end

    # Check if result is a plot and capture it
    if is_displayable_plot(result.return_value)
        img_data = capture_plot_as_png(result.return_value)
        if img_data !== nothing
            push!(result.images, ("png", img_data))
        end
    end

    result
end

function is_displayable_plot(x)
    x === nothing && return false
    t = string(typeof(x))
    any(pattern -> occursin(pattern, t), ["Plot", "Figure", "Scene", "Chart"])
end

function capture_plot_as_png(p)
    io = IOBuffer()
    try
        show(io, MIME("image/png"), p)
        return take!(io)
    catch
        return nothing
    end
end

export CapturedOutput, capture_execution

end
```

### 4. Cell Macros (kernel/cell_macros.jl)

```julia
module CellMacros

using ..CellRegistry
using ..ASTAnalysis
using ..OutputCapture
using JSON3

# The main @cell macro
macro cell(index, exec_count, body)
    idx = index isa Int ? index : eval(index)
    ec = exec_count === :nothing ? nothing : (exec_count isa Int ? exec_count : eval(exec_count))

    quote
        local cell_idx = $idx
        local cell_ec = $ec
        local cell_expr = $(QuoteNode(body))

        # Analyze dependencies
        local defines = ASTAnalysis.extract_defines(cell_expr)
        local uses = ASTAnalysis.extract_uses(cell_expr)

        # Create/update cell in registry
        cell = Cell(
            cell_idx,
            cell_ec,
            cell_expr,
            defines,
            uses,
            nothing,
            "",
            "",
            nothing,
            :running
        )
        CellRegistry.CELLS[cell_idx] = cell

        # Update variable sources
        for var in defines
            CellRegistry.VARIABLE_SOURCES[var] = cell_idx
        end

        # Execute with output capture
        local result = try
            captured = OutputCapture.capture_execution() do
                $(esc(body))
            end
            cell.outputs = captured.return_value
            cell.stdout = captured.stdout
            cell.stderr = captured.stderr
            cell.status = :done
            captured
        catch e
            cell.error = e
            cell.status = :error
            rethrow(e)
        end

        # Return JSON result for Helix
        _cell_result_json(cell_idx, cell)
    end
end

macro markdown(index, body)
    # Markdown cells just store content, don't execute
    quote
        local cell_idx = $index
        CellRegistry.CELLS[cell_idx] = Cell(
            cell_idx,
            nothing,
            $(QuoteNode(body)),
            Set{Symbol}(),
            Set{Symbol}(),
            string($(esc(body))),
            "",
            "",
            nothing,
            :done
        )
        nothing
    end
end

function _cell_result_json(idx, cell)
    deps = CellRegistry.get_dependencies(idx)
    dependents = CellRegistry.get_dependents(idx)

    result = Dict(
        "cell_index" => idx,
        "status" => string(cell.status),
        "defines" => collect(cell.defines),
        "uses" => collect(cell.uses),
        "dependencies" => deps,
        "dependents" => dependents,
        "stdout" => cell.stdout,
        "stderr" => cell.stderr,
        "has_error" => cell.error !== nothing,
        "error" => cell.error !== nothing ? sprint(showerror, cell.error) : nothing,
        "output_type" => _get_output_type(cell.outputs),
        "output_repr" => _repr_output(cell.outputs)
    )

    JSON3.write(result)
end

function _get_output_type(x)
    x === nothing && return "nothing"
    x isa Exception && return "error"
    t = string(typeof(x))
    if any(pattern -> occursin(pattern, t), ["Plot", "Figure", "Scene"])
        return "plot"
    elseif x isa AbstractArray
        return "array"
    elseif x isa DataFrame
        return "dataframe"
    else
        return "value"
    end
end

function _repr_output(x)
    x === nothing && return ""
    try
        repr(MIME("text/plain"), x)
    catch
        repr(x)
    end
end

export @cell, @markdown

end
```

### 5. Kernel Runner (kernel/runner.jl)

```julia
# Main kernel runner that loads all modules and handles commands

include("cell_registry.jl")
include("ast_analysis.jl")
include("output_capture.jl")
include("cell_macros.jl")

using .CellRegistry
using .ASTAnalysis
using .OutputCapture
using .CellMacros
using JSON3

const KERNEL_DIR = ARGS[1]
const INPUT_FILE = joinpath(KERNEL_DIR, "input.json")
const OUTPUT_FILE = joinpath(KERNEL_DIR, "output.json")
const LOG_FILE = joinpath(KERNEL_DIR, "kernel.log")

function log_msg(msg)
    open(LOG_FILE, "a") do io
        println(io, "[$(now())] $msg")
    end
end

function write_output(data)
    open(OUTPUT_FILE, "w") do io
        JSON3.write(io, data)
    end
    touch(OUTPUT_FILE * ".done")
end

function handle_command(cmd::Dict)
    cmd_type = get(cmd, "type", "")

    if cmd_type == "execute_cell"
        cell_idx = cmd["cell_index"]
        code = cmd["code"]

        log_msg("Executing cell $cell_idx")

        # Parse and execute
        expr = Meta.parse("begin\n$code\nend")
        wrapped = :(@cell $cell_idx nothing $expr)

        try
            result = eval(wrapped)
            write_output(Dict("status" => "ok", "result" => result))
        catch e
            log_msg("Error in cell $cell_idx: $e")
            write_output(Dict(
                "status" => "error",
                "error" => sprint(showerror, e),
                "stacktrace" => sprint(showerror, e, catch_backtrace())
            ))
        end

    elseif cmd_type == "get_dependencies"
        cell_idx = cmd["cell_index"]
        deps = CellRegistry.get_dependencies(cell_idx)
        dependents = CellRegistry.get_dependents(cell_idx)
        write_output(Dict(
            "dependencies" => deps,
            "dependents" => dependents
        ))

    elseif cmd_type == "execute_reactive"
        # Execute a cell and all its dependents
        cell_idx = cmd["cell_index"]
        code = cmd["code"]

        # First execute the target cell
        expr = Meta.parse("begin\n$code\nend")
        wrapped = :(@cell $cell_idx nothing $expr)
        eval(wrapped)

        # Then execute all dependents in order
        dependents = CellRegistry.get_dependents(cell_idx)
        for dep_idx in dependents
            if haskey(CellRegistry.CELLS, dep_idx)
                dep_cell = CellRegistry.CELLS[dep_idx]
                eval(:(@cell $dep_idx nothing $(dep_cell.code)))
            end
        end

        write_output(Dict(
            "status" => "ok",
            "executed" => [cell_idx; dependents]
        ))

    elseif cmd_type == "list_cells"
        cells = Dict(
            idx => Dict(
                "defines" => collect(cell.defines),
                "uses" => collect(cell.uses),
                "status" => string(cell.status)
            )
            for (idx, cell) in CellRegistry.CELLS
        )
        write_output(cells)

    else
        write_output(Dict("status" => "error", "error" => "Unknown command: $cmd_type"))
    end
end

# Main loop
log_msg("Kernel started")
write_output(Dict("status" => "ready"))

while true
    if isfile(INPUT_FILE)
        try
            cmd = JSON3.read(read(INPUT_FILE, String))
            rm(INPUT_FILE)  # Remove to prevent re-execution
            handle_command(cmd)
        catch e
            log_msg("Error reading command: $e")
            write_output(Dict("status" => "error", "error" => sprint(showerror, e)))
        end
    end
    sleep(0.05)  # 50ms polling
end
```

### 6. Rust FFI Updates (libnothelix/src/kernel.rs)

```rust
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

#[derive(Serialize)]
pub struct KernelCommand {
    pub r#type: String,
    pub cell_index: Option<usize>,
    pub code: Option<String>,
}

#[derive(Deserialize)]
pub struct KernelResponse {
    pub status: String,
    pub result: Option<String>,
    pub error: Option<String>,
    pub dependencies: Option<Vec<usize>>,
    pub dependents: Option<Vec<usize>>,
    pub executed: Option<Vec<usize>>,
}

pub fn send_command(kernel_dir: &str, cmd: &KernelCommand) -> Result<(), String> {
    let input_file = format!("{}/input.json", kernel_dir);
    let json = serde_json::to_string(cmd).map_err(|e| e.to_string())?;
    fs::write(&input_file, json).map_err(|e| e.to_string())?;
    Ok(())
}

pub fn execute_cell(kernel_dir: &str, cell_index: usize, code: &str) -> Result<(), String> {
    send_command(kernel_dir, &KernelCommand {
        r#type: "execute_cell".to_string(),
        cell_index: Some(cell_index),
        code: Some(code.to_string()),
    })
}

pub fn execute_reactive(kernel_dir: &str, cell_index: usize, code: &str) -> Result<(), String> {
    send_command(kernel_dir, &KernelCommand {
        r#type: "execute_reactive".to_string(),
        cell_index: Some(cell_index),
        code: Some(code.to_string()),
    })
}

pub fn get_dependencies(kernel_dir: &str, cell_index: usize) -> Result<(), String> {
    send_command(kernel_dir, &KernelCommand {
        r#type: "get_dependencies".to_string(),
        cell_index: Some(cell_index),
        code: None,
    })
}
```

### 7. Steel Plugin Updates

```scheme
;; Execute cell with reactive updates
(define (execute-cell-reactive)
  (define bounds-json (get-cell-bounds content current-line))
  (define cell-index (json-get-string bounds-json "cell_index"))
  (define code (json-get-string bounds-json "code"))

  ;; Send reactive execute command
  (kernel-execute-reactive kernel-dir cell-index code)

  ;; Wait for result and update all affected cells
  (spawn-native-thread
    (lambda ()
      (define result (kernel-execute-wait kernel-dir))
      (define executed-cells (json-get-array result "executed"))
      (hx.with-context
        (lambda ()
          ;; Update output for each executed cell
          (for-each update-cell-output executed-cells))))))
```

## Migration Path

1. **Phase 1**: Implement kernel modules without breaking existing functionality
2. **Phase 2**: Add JSON command protocol alongside file-based IPC
3. **Phase 3**: Update Steel plugin to use new protocol
4. **Phase 4**: Add reactive execution
5. **Phase 5**: Remove legacy file-based IPC

## File Structure

```
nothelix/
├── kernel/
│   ├── runner.jl           # Main kernel entry point
│   ├── cell_registry.jl    # Cell state management
│   ├── ast_analysis.jl     # Dependency extraction
│   ├── output_capture.jl   # stdout/stderr/plot capture
│   └── cell_macros.jl      # @cell and @markdown macros
├── libnothelix/
│   └── src/
│       ├── lib.rs          # Existing FFI
│       └── kernel.rs       # New kernel communication
└── plugins/
    └── nothelix.scm        # Updated Steel plugin
```
