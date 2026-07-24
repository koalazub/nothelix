module CellRegistry

export Cell, CELLS, VARIABLE_SOURCES, VARIABLE_USERS, VARIABLE_TYPES, get_dependencies, get_dependents, clear_registry, lookup_variable_context, unexecuted_dependencies, in_scope_variables_by_type, provenance_notes, next_run_seq!, classify_all, set_current_cell!, current_cell

# Cell state structure
mutable struct Cell
    index::Int
    exec_count::Union{Int, Nothing}
    code::Union{Expr, Nothing}
    code_string::String
    defines::Set{Symbol}
    uses::Set{Symbol}
    outputs::Any
    stdout::String
    stderr::String
    images::Vector{Tuple{String, String}}  # (format, base64_data) for inline rendering
    text_plots::Vector{Dict{String,Any}}  # UnicodePlots braille rows+spans, one entry per plot
    audio::Vector{Dict{String,Any}}
    plot_data::Union{Vector{Dict{String,Any}}, Nothing}  # raw x/y series for interactive charts
    error::Union{Exception, Nothing}
    stacktrace::Union{Vector, Nothing}
    notes::Vector{String}
    status::Symbol  # :pending, :running, :done, :error
    run_seq::Int
    duration::Union{Int, Nothing}  # wall time of the last run in whole ms, nothing until run
end

# Constructors
Cell(index::Int) = Cell(index, nothing, nothing, "", Set{Symbol}(), Set{Symbol}(), nothing, "", "", [], [], Dict{String,Any}[], nothing, nothing, nothing, String[], :pending, 0, nothing)

# Global registry
const CELLS = Dict{Int, Cell}()
const VARIABLE_SOURCES = Dict{Symbol, Int}()  # Which cell defines each variable
const VARIABLE_USERS = Dict{Symbol, Set{Int}}()  # var → set of cell indices that use it
# Runtime type snapshot. Populated by the cell macro/executor after each
# successful run with `string(typeof(value))` of every symbol in
# `cell.defines`. Stale entries linger when a cell is re-run and no
# longer defines a variable — acceptable because the hints that consume
# this map tolerate "historical" info.
const VARIABLE_TYPES = Dict{Symbol, String}()

const RUN_SEQ = Ref{Int}(0)

const CURRENT_CELL = Ref{Int}(-1)

function next_run_seq!()::Int
    RUN_SEQ[] += 1
    RUN_SEQ[]
end

set_current_cell!(idx::Int) = (CURRENT_CELL[] = idx)
current_cell()::Int = CURRENT_CELL[]

# Clear registry (useful for testing)
function clear_registry()
    empty!(CELLS)
    empty!(VARIABLE_SOURCES)
    empty!(VARIABLE_USERS)
    empty!(VARIABLE_TYPES)
    RUN_SEQ[] = 0
    CURRENT_CELL[] = -1
end

# Get cells that this cell depends on
function get_dependencies(cell_idx::Int)::Vector{Int}
    !haskey(CELLS, cell_idx) && return Int[]
    cell = CELLS[cell_idx]
    deps = Int[]
    for var in cell.uses
        if haskey(VARIABLE_SOURCES, var) && VARIABLE_SOURCES[var] != cell_idx
            push!(deps, VARIABLE_SOURCES[var])
        end
    end
    sort!(unique!(deps))
end

# Get cells that depend on this cell (uses reverse index for O(d) lookup)
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

# Get topological execution order for a set of cells
function get_execution_order(cell_indices::Vector{Int})::Vector{Int}
    visited = Set{Int}()
    order = Int[]

    function visit(idx::Int)
        idx in visited && return
        push!(visited, idx)
        for dep in get_dependencies(idx)
            dep in cell_indices && visit(dep)
        end
        push!(order, idx)
    end

    for idx in cell_indices
        visit(idx)
    end

    order
end

"""
For a set of variable names, return context about where they're defined
and whether those cells have been executed.
"""
function lookup_variable_context(var_names)::Dict{String, Any}
    context = Dict{String, Any}()
    for var in var_names
        sym = var isa Symbol ? var : Symbol(var)
        if haskey(VARIABLE_SOURCES, sym)
            src_idx = VARIABLE_SOURCES[sym]
            if haskey(CELLS, src_idx)
                src_cell = CELLS[src_idx]
                # Wire format matches the Rust `VarContext` enum's
                # `#[serde(tag = "source")]` discriminator. Cells in
                # :done/:error status carry `status`; pending cells
                # (shouldn't normally land here since VARIABLE_SOURCES
                # is populated on execute, but handle defensively) use
                # the pending_registered variant.
                context[string(sym)] = if src_cell.status in (:done, :error)
                    Dict{String, Any}(
                        "source" => "executed",
                        "defined_in_cell" => src_idx,
                        "status" => string(src_cell.status),
                    )
                else
                    Dict{String, Any}(
                        "source" => "pending_registered",
                        "defined_in_cell" => src_idx,
                    )
                end
            end
        end
    end
    context
end

"""
Return cell indices that this cell depends on but haven't been executed.
"""
function unexecuted_dependencies(cell_idx::Int)::Vector{Int}
    deps = get_dependencies(cell_idx)
    filter(d -> haskey(CELLS, d) && CELLS[d].status ∉ (:done,), deps)
end

"""
Group currently-known variable types for the error enricher. Returns
`Dict{type_string => [{"name", "cell"}, …]}` so Rust can answer
"which in-scope variables have type T?" with one map lookup per type
parsed out of a MethodError signature.
"""
function in_scope_variables_by_type()::Dict{String, Vector{Dict{String, Any}}}
    out = Dict{String, Vector{Dict{String, Any}}}()
    for (sym, typ) in VARIABLE_TYPES
        haskey(VARIABLE_SOURCES, sym) || continue
        src_idx = VARIABLE_SOURCES[sym]
        entry = Dict{String, Any}(
            "name" => string(sym),
            "cell" => src_idx,
        )
        push!(get!(() -> Dict{String, Any}[], out, typ), entry)
    end
    out
end

function provenance_notes(cell_idx::Int, uses)::Vector{String}
    notes = String[]
    for v in sort!(collect(uses))
        haskey(VARIABLE_SOURCES, v) || continue
        writer = VARIABLE_SOURCES[v]
        writer == cell_idx && continue
        haskey(CELLS, writer) || continue
        if !(v in CELLS[writer].defines)
            push!(notes, "note: $(v) was last assigned by cell $(writer), whose current code no longer assigns it")
        elseif writer > cell_idx
            push!(notes, "note: $(v) was last assigned by cell $(writer), below this cell")
        end
    end
    notes
end

function input_relationship(cell::Cell, v::Symbol)::String
    writer = VARIABLE_SOURCES[v]
    wcell = CELLS[writer]
    if !(v in wcell.defines)
        "orphan"
    elseif writer > cell.index
        "below"
    elseif wcell.run_seq > cell.run_seq
        "stale"
    else
        "fresh"
    end
end

function cell_state_from_inputs(inputs)::String
    rank = 0
    for inp in inputs
        r = inp["rel"]
        rank = max(rank, r == "below" ? 3 : r == "orphan" ? 2 : r == "stale" ? 1 : 0)
    end
    rank == 3 ? "out-of-order" : rank == 2 ? "orphan-input" : rank == 1 ? "stale-input" : "fresh"
end

function classify_all()::Dict{String, Any}
    out = Dict{String, Any}()
    for (idx, cell) in CELLS
        (cell.status === :done || cell.status === :error) || continue
        inputs = Vector{Dict{String, Any}}()
        for v in sort!(collect(cell.uses))
            haskey(VARIABLE_SOURCES, v) || continue
            writer = VARIABLE_SOURCES[v]
            writer == idx && continue
            haskey(CELLS, writer) || continue
            push!(inputs, Dict{String, Any}(
                "name" => string(v),
                "writer" => writer,
                "rel" => input_relationship(cell, v),
            ))
        end
        out[string(idx)] = Dict{String, Any}(
            "state" => cell_state_from_inputs(inputs),
            "inputs" => inputs,
            "duration" => cell.duration,
        )
    end
    out
end

end # module
