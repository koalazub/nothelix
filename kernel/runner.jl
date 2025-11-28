# Nothelix Kernel Runner
# This is the main entry point for the Julia kernel process

using Dates
using JSON3

# Get kernel directory from command line
const KERNEL_DIR = length(ARGS) >= 1 ? ARGS[1] : "/tmp/helix-kernel-test"
const INPUT_FILE = joinpath(KERNEL_DIR, "input.json")
const OUTPUT_FILE = joinpath(KERNEL_DIR, "output.json")
const LOG_FILE = joinpath(KERNEL_DIR, "kernel.log")
const READY_FILE = joinpath(KERNEL_DIR, "ready")

# Logging - only write to log file, NOT stdout (stdout gets captured during cell execution)
function log_msg(level::Symbol, msg::String)
    timestamp = Dates.format(now(), "yyyy-mm-dd HH:MM:SS.sss")
    log_line = "[$timestamp] [$level] $msg"
    open(LOG_FILE, "a") do io
        println(io, log_line)
    end
end

log_info(msg) = log_msg(:INFO, msg)
log_error(msg) = log_msg(:ERROR, msg)
log_debug(msg) = log_msg(:DEBUG, msg)

log_info("Kernel starting in $KERNEL_DIR")

# Load modules
const KERNEL_ROOT = @__DIR__
log_info("Loading modules from $KERNEL_ROOT")

include(joinpath(KERNEL_ROOT, "cell_registry.jl"))
include(joinpath(KERNEL_ROOT, "ast_analysis.jl"))
include(joinpath(KERNEL_ROOT, "output_capture.jl"))
include(joinpath(KERNEL_ROOT, "cell_macros.jl"))

using .CellRegistry
using .ASTAnalysis
using .OutputCapture
using .CellMacros

log_info("Modules loaded successfully")

# Export macros to Main module so they're available in cell execution
Core.eval(Main, :(using ..CellMacros: @cell, @markdown))
Core.eval(Main, :(using ..CellRegistry))

# Write output response
function write_response(data::Dict)
    json_str = JSON3.write(data)
    log_debug("Writing response: $(length(json_str)) bytes")
    open(OUTPUT_FILE, "w") do io
        write(io, json_str)
    end
    touch(OUTPUT_FILE * ".done")
end

# Command handlers
function handle_execute_cell(cmd::Dict)
    cell_idx = get(cmd, "cell_index", 0)
    code = get(cmd, "code", "")

    log_info("Executing cell $cell_idx ($(length(code)) bytes)")

    try
        # Execute the cell
        result = CellMacros.execute_cell(cell_idx, code)

        if result.success
            log_info("Cell $cell_idx executed successfully")
            cell_result = CellMacros.get_cell_result_json(cell_idx)
            write_response(Dict(
                "status" => "ok",
                "cell" => cell_result
            ))
        else
            log_error("Cell $cell_idx failed: $(result.error)")
            write_response(Dict(
                "status" => "error",
                "error" => sprint(showerror, result.error),
                "stacktrace" => sprint(showerror, result.error, result.stacktrace)
            ))
        end
    catch e
        log_error("Exception executing cell $cell_idx: $e")
        write_response(Dict(
            "status" => "error",
            "error" => sprint(showerror, e),
            "stacktrace" => sprint(showerror, e, catch_backtrace())
        ))
    end
end

function handle_execute_reactive(cmd::Dict)
    cell_idx = get(cmd, "cell_index", 0)
    code = get(cmd, "code", "")

    log_info("Reactive execution starting from cell $cell_idx")

    executed = Int[]

    try
        # Execute the target cell first
        result = CellMacros.execute_cell(cell_idx, code)
        push!(executed, cell_idx)

        if !result.success
            log_error("Primary cell $cell_idx failed")
            write_response(Dict(
                "status" => "error",
                "error" => sprint(showerror, result.error),
                "executed" => executed
            ))
            return
        end

        # Get and execute dependents in order
        dependents = CellRegistry.get_dependents(cell_idx)
        log_info("Dependents of cell $cell_idx: $dependents")

        for dep_idx in dependents
            if haskey(CellRegistry.CELLS, dep_idx)
                dep_cell = CellRegistry.CELLS[dep_idx]
                log_info("Re-executing dependent cell $dep_idx")
                dep_result = CellMacros.execute_cell(dep_idx, dep_cell.code_string)
                push!(executed, dep_idx)

                if !dep_result.success
                    log_error("Dependent cell $dep_idx failed")
                    # Continue executing other dependents
                end
            end
        end

        # Gather results for all executed cells
        cell_results = Dict(
            idx => CellMacros.get_cell_result_json(idx)
            for idx in executed
        )

        write_response(Dict(
            "status" => "ok",
            "executed" => executed,
            "cells" => cell_results
        ))

    catch e
        log_error("Exception in reactive execution: $e")
        write_response(Dict(
            "status" => "error",
            "error" => sprint(showerror, e),
            "executed" => executed
        ))
    end
end

function handle_get_dependencies(cmd::Dict)
    cell_idx = get(cmd, "cell_index", 0)

    log_info("Getting dependencies for cell $cell_idx")

    deps = CellRegistry.get_dependencies(cell_idx)
    dependents = CellRegistry.get_dependents(cell_idx)

    write_response(Dict(
        "status" => "ok",
        "cell_index" => cell_idx,
        "dependencies" => deps,
        "dependents" => dependents
    ))
end

function handle_list_cells(cmd::Dict)
    log_info("Listing all cells")

    cells = Dict(
        string(idx) => Dict(
            "defines" => [string(s) for s in cell.defines],
            "uses" => [string(s) for s in cell.uses],
            "status" => string(cell.status),
            "has_output" => cell.outputs !== nothing
        )
        for (idx, cell) in CellRegistry.CELLS
    )

    write_response(Dict(
        "status" => "ok",
        "cells" => cells,
        "variable_sources" => Dict(
            string(var) => idx
            for (var, idx) in CellRegistry.VARIABLE_SOURCES
        )
    ))
end

function handle_get_cell(cmd::Dict)
    cell_idx = get(cmd, "cell_index", 0)

    log_info("Getting cell $cell_idx")

    if haskey(CellRegistry.CELLS, cell_idx)
        write_response(Dict(
            "status" => "ok",
            "cell" => CellMacros.get_cell_result_json(cell_idx)
        ))
    else
        write_response(Dict(
            "status" => "error",
            "error" => "Cell $cell_idx not found"
        ))
    end
end

function handle_clear(cmd::Dict)
    log_info("Clearing cell registry")
    CellRegistry.clear_registry()
    write_response(Dict("status" => "ok"))
end

# Main command dispatcher
function handle_command(cmd::Dict)
    cmd_type = get(cmd, "type", "")
    log_info("Handling command: $cmd_type")

    if cmd_type == "execute_cell"
        handle_execute_cell(cmd)
    elseif cmd_type == "execute_reactive"
        handle_execute_reactive(cmd)
    elseif cmd_type == "get_dependencies"
        handle_get_dependencies(cmd)
    elseif cmd_type == "list_cells"
        handle_list_cells(cmd)
    elseif cmd_type == "get_cell"
        handle_get_cell(cmd)
    elseif cmd_type == "clear"
        handle_clear(cmd)
    elseif cmd_type == "ping"
        write_response(Dict("status" => "ok", "message" => "pong"))
    else
        log_error("Unknown command type: $cmd_type")
        write_response(Dict(
            "status" => "error",
            "error" => "Unknown command: $cmd_type"
        ))
    end
end

# Signal that kernel is ready
# NOTE: Only touch READY_FILE, don't use write_response here!
# write_response creates output.json.done which confuses polling for actual commands
log_info("Kernel ready, writing ready marker")
touch(READY_FILE)

# Main loop
log_info("Entering main loop")
while true
    try
        if isfile(INPUT_FILE)
            log_debug("Input file detected")

            # Read and parse command
            cmd_str = read(INPUT_FILE, String)

            # Remove safely (might already be gone due to race condition)
            try
                rm(INPUT_FILE)
            catch
                # File already removed - that's fine
            end

            if !isempty(cmd_str)
                cmd = JSON3.read(cmd_str, Dict)
                handle_command(cmd)
            end
        end
    catch e
        log_error("Error in main loop: $e")
        try
            write_response(Dict(
                "status" => "error",
                "error" => sprint(showerror, e)
            ))
        catch
        end
    end

    sleep(0.05)  # 50ms polling interval
end
