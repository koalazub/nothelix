module CellRegistry

export Cell, CELLS, VARIABLE_SOURCES, get_dependencies, get_dependents, clear_registry

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
    error::Union{Exception, Nothing}
    status::Symbol  # :pending, :running, :done, :error
end

# Constructors
Cell(index::Int) = Cell(index, nothing, nothing, "", Set{Symbol}(), Set{Symbol}(), nothing, "", "", [], nothing, :pending)

# Global registry
const CELLS = Dict{Int, Cell}()
const VARIABLE_SOURCES = Dict{Symbol, Int}()  # Which cell defines each variable

# Clear registry (useful for testing)
function clear_registry()
    empty!(CELLS)
    empty!(VARIABLE_SOURCES)
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

# Get cells that depend on this cell
function get_dependents(cell_idx::Int)::Vector{Int}
    !haskey(CELLS, cell_idx) && return Int[]
    cell = CELLS[cell_idx]
    dependents = Int[]

    # Find all variables this cell defines
    for var in cell.defines
        # Find all cells that use this variable
        for (idx, other_cell) in CELLS
            if idx != cell_idx && var in other_cell.uses
                push!(dependents, idx)
            end
        end
    end
    sort!(unique!(dependents))
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

end # module
