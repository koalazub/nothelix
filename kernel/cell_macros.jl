module CellMacros

using ..CellRegistry
using ..ASTAnalysis
using ..OutputCapture

export @cell, @markdown, execute_cell, get_cell_result_json

# The main @cell macro - wraps code execution with tracking
macro cell(index, exec_count, body)
    idx = esc(index)
    ec = esc(exec_count)
    body_quoted = QuoteNode(body)
    body_escaped = esc(body)

    quote
        local cell_idx = $idx
        local cell_ec = $ec == :nothing ? nothing : $ec
        local cell_expr = $body_quoted
        local code_str = string($body_quoted)

        # Analyze dependencies from the AST
        local analysis = ASTAnalysis.analyze_code(code_str)
        local defines = analysis.defines
        local uses = analysis.uses

        # Create cell in registry
        local cell = CellRegistry.Cell(cell_idx)
        cell.exec_count = cell_ec
        cell.code = cell_expr
        cell.code_string = code_str
        cell.defines = defines
        cell.uses = uses
        cell.status = :running
        CellRegistry.CELLS[cell_idx] = cell

        # Update variable sources
        for var in defines
            CellRegistry.VARIABLE_SOURCES[var] = cell_idx
        end

        # Execute with output capture
        local captured = OutputCapture.capture_simple() do
            $body_escaped
        end

        # Store results
        cell.outputs = captured.return_value
        cell.stdout = captured.stdout
        cell.stderr = captured.stderr
        cell.error = captured.error
        cell.status = captured.error === nothing ? :done : :error

        # Return the captured output (for REPL display)
        if captured.error !== nothing
            rethrow(captured.error)
        else
            captured.return_value
        end
    end
end

# Markdown cell - just stores content, doesn't execute Julia code
macro markdown(index, body)
    idx = esc(index)
    body_str = string(body)

    quote
        local cell_idx = $idx
        local cell = CellRegistry.Cell(cell_idx)
        cell.code_string = $body_str
        cell.outputs = $body_str
        cell.status = :done
        CellRegistry.CELLS[cell_idx] = cell
        nothing
    end
end

# Execute a cell by index (for programmatic execution)
# Uses include_string for TRUE top-level execution (like Jupyter does)
function execute_cell(cell_idx::Int, code::String)
    # Create cell in registry
    cell = CellRegistry.Cell(cell_idx)
    cell.code_string = code
    cell.status = :running
    CellRegistry.CELLS[cell_idx] = cell

    # Analyze dependencies from code (before execution)
    analysis = ASTAnalysis.analyze_code(code)
    cell.defines = analysis.defines
    cell.uses = analysis.uses

    # Update variable sources for defines
    for var in cell.defines
        CellRegistry.VARIABLE_SOURCES[var] = cell_idx
    end

    # Execute at TRUE top level with output capture
    # This is how Jupyter does it - include_string runs at module top level
    captured = OutputCapture.capture_toplevel(Main, code)

    # Store results in cell
    cell.outputs = captured.return_value
    cell.stdout = captured.stdout
    cell.stderr = captured.stderr
    cell.images = captured.images  # Store captured images (format, base64_data)
    cell.error = captured.error
    cell.status = captured.error === nothing ? :done : :error

    if captured.error !== nothing
        return (success=false, error=captured.error, stacktrace=captured.stacktrace)
    else
        return (success=true, result=captured.return_value)
    end
end

# Get cell execution result as JSON-compatible Dict
function get_cell_result_json(cell_idx::Int)
    !haskey(CellRegistry.CELLS, cell_idx) && return Dict(
        "error" => "Cell $cell_idx not found"
    )

    cell = CellRegistry.CELLS[cell_idx]
    deps = CellRegistry.get_dependencies(cell_idx)
    dependents = CellRegistry.get_dependents(cell_idx)

    result = Dict{String, Any}(
        "cell_index" => cell_idx,
        "status" => string(cell.status),
        "defines" => [string(s) for s in cell.defines],
        "uses" => [string(s) for s in cell.uses],
        "dependencies" => deps,
        "dependents" => dependents,
        "stdout" => cell.stdout,
        "stderr" => cell.stderr,
        "has_error" => cell.error !== nothing,
    )

    if cell.error !== nothing
        result["error"] = sprint(showerror, cell.error)
    end

    # Output representation
    result["output_type"] = get_output_type(cell.outputs)
    result["output_repr"] = get_output_repr(cell.outputs)

    # Include images for inline rendering (base64 encoded)
    # Format: [{"format": "png", "data": "base64..."}]
    if !isempty(cell.images)
        result["images"] = [
            Dict("format" => fmt, "data" => data)
            for (fmt, data) in cell.images
        ]
    end

    result
end

function get_output_type(x)
    x === nothing && return "nothing"
    x isa Exception && return "error"
    t = string(typeof(x))
    if OutputCapture.is_displayable_plot(x)
        return "plot"
    elseif x isa AbstractArray
        return "array"
    elseif occursin("DataFrame", t)
        return "dataframe"
    elseif x isa AbstractString
        return "string"
    elseif x isa Number
        return "number"
    else
        return "value"
    end
end

function get_output_repr(x)
    x === nothing && return ""
    try
        # Try text/plain MIME first
        io = IOBuffer()
        show(io, MIME("text/plain"), x)
        return String(take!(io))
    catch
        try
            return repr(x)
        catch
            return "<unable to display>"
        end
    end
end

end # module
