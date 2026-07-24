module KernelWidgets

using ..CellRegistry

export record_slider!, record_choice!, set_var!, is_identifier, widget_kind_for

function register!(kind::AbstractString, name::AbstractString, params::AbstractString, current)
    idx = CellRegistry.current_cell()
    haskey(CellRegistry.CELLS, idx) || return nothing
    push!(CellRegistry.CELLS[idx].widgets, Dict{String, Any}(
        "kind" => kind,
        "name" => name,
        "params" => params,
        "current" => current === nothing ? "" : string(current),
    ))
    nothing
end

function current_of(mod::Module, name::AbstractString)
    sym = Symbol(name)
    isdefined(mod, sym) || return nothing
    try
        getfield(mod, sym)
    catch
        nothing
    end
end

function record_slider!(mod::Module, name::AbstractString, lo::Real, hi::Real, step::Real)
    params = string(lo) * ":" * string(hi) * ":" * string(step)
    register!("slider", name, params, current_of(mod, name))
end

function record_choice!(mod::Module, name::AbstractString, options)
    params = join(options, "|")
    register!("choice", name, params, current_of(mod, name))
end

function is_identifier(name::AbstractString)::Bool
    isempty(name) && return false
    for (i, c) in enumerate(name)
        ok = if i == 1
            ('a' <= c <= 'z') || ('A' <= c <= 'Z') || c == '_'
        else
            ('a' <= c <= 'z') || ('A' <= c <= 'Z') || ('0' <= c <= '9') || c == '_' || c == '!'
        end
        ok || return false
    end
    true
end

function widget_kind_for(cell_index::Integer, name::AbstractString)
    haskey(CellRegistry.CELLS, cell_index) || return nothing
    for w in CellRegistry.CELLS[cell_index].widgets
        get(w, "name", "") == name && return get(w, "kind", nothing)
    end
    nothing
end

function parse_value(kind, raw::AbstractString)
    kind == "choice" && return String(raw)
    iv = tryparse(Int, raw)
    iv !== nothing && return iv
    fv = tryparse(Float64, raw)
    fv !== nothing && return fv
    kind == "slider" && return nothing
    String(raw)
end

function set_var!(mod::Module, name::AbstractString, raw::AbstractString, cell_index::Integer)
    is_identifier(name) || return (ok = false, reason = "invalid variable name: $name")
    kind = widget_kind_for(cell_index, name)
    value = parse_value(kind, raw)
    value === nothing && return (ok = false, reason = "unparseable value for $name: $raw")

    sym = Symbol(name)
    Core.eval(mod, Expr(:(=), sym, value))
    CellRegistry.VARIABLE_SOURCES[sym] = cell_index
    if haskey(CellRegistry.CELLS, cell_index)
        cell = CellRegistry.CELLS[cell_index]
        push!(cell.defines, sym)
        cell.run_seq = CellRegistry.next_run_seq!()
        for w in cell.widgets
            get(w, "name", "") == name && (w["current"] = string(value))
        end
    end
    try
        CellRegistry.VARIABLE_TYPES[sym] = string(typeof(value))
    catch
    end
    (ok = true, reason = nothing)
end

end # module
