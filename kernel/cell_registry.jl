module CellRegistry

export Cell, CELLS, VARIABLE_SOURCES, VARIABLE_USERS, get_dependencies, get_dependents, clear_registry, lookup_variable_context, unexecuted_dependencies

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
    plot_data::Union{Vector{Dict{String,Any}}, Nothing}  # raw x/y series for interactive charts
    error::Union{Exception, Nothing}
    stacktrace::Union{Vector, Nothing}
    status::Symbol  # :pending, :running, :done, :error
end

# Constructors
Cell(index::Int) = Cell(index, nothing, nothing, "", Set{Symbol}(), Set{Symbol}(), nothing, "", "", [], nothing, nothing, nothing, :pending)

# Global registry
const CELLS = Dict{Int, Cell}()
const VARIABLE_SOURCES = Dict{Symbol, Int}()  # Which cell defines each variable
const VARIABLE_USERS = Dict{Symbol, Set{Int}}()  # var → set of cell indices that use it

# Clear registry (useful for testing)
function clear_registry()
    empty!(CELLS)
    empty!(VARIABLE_SOURCES)
    empty!(VARIABLE_USERS)
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

end # module
