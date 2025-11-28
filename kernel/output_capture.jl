module OutputCapture

using Base64

export CapturedOutput, capture_execution, capture_toplevel, is_displayable_plot, capture_plot_png

mutable struct CapturedOutput
    return_value::Any
    stdout::String
    stderr::String
    images::Vector{Tuple{String, String}}  # (format, base64_data)
    error::Union{Exception, Nothing}
    stacktrace::Union{Vector, Nothing}
end

CapturedOutput() = CapturedOutput(nothing, "", "", [], nothing, nothing)

function capture_execution(f)
    result = CapturedOutput()

    # Create IO buffers for capturing output
    stdout_buf = IOBuffer()
    stderr_buf = IOBuffer()

    # Capture stdout and stderr
    old_stdout = stdout
    old_stderr = stderr

    try
        # Redirect output
        rd_out, wr_out = redirect_stdout()
        rd_err, wr_err = redirect_stderr()

        # Start tasks to read from pipes (capture only, no echo)
        stdout_task = @async begin
            while isopen(rd_out)
                data = String(readavailable(rd_out))
                write(stdout_buf, data)
            end
        end

        stderr_task = @async begin
            while isopen(rd_err)
                data = String(readavailable(rd_err))
                write(stderr_buf, data)
            end
        end

        # Execute the function
        try
            result.return_value = f()
        catch e
            result.error = e
            result.stacktrace = catch_backtrace()
        end

        # Restore stdout/stderr
        redirect_stdout(old_stdout)
        redirect_stderr(old_stderr)
        close(wr_out)
        close(wr_err)

        # Wait for capture tasks
        wait(stdout_task)
        wait(stderr_task)

    catch e
        # Ensure we restore stdout/stderr on error
        redirect_stdout(old_stdout)
        redirect_stderr(old_stderr)
        result.error = e
        result.stacktrace = catch_backtrace()
    end

    result.stdout = String(take!(stdout_buf))
    result.stderr = String(take!(stderr_buf))

    # Check for displayable plot
    if result.error === nothing && is_displayable_plot(result.return_value)
        img_b64 = capture_plot_png(result.return_value)
        if img_b64 !== nothing
            push!(result.images, ("png", img_b64))
        end
    end

    result
end

# Check if a value is a displayable plot
function is_displayable_plot(x)
    x === nothing && return false
    t = string(typeof(x))
    patterns = [
        "Plot",       # Plots.jl
        "Figure",     # Makie, PyPlot, Gadfly
        "Scene",      # Makie
        "FigureAxis", # Makie
        "Chart",      # VegaLite
        "Canvas",     # UnicodePlots
        "Drawing",    # Luxor
        "GtkCanvas",  # Gtk plots
    ]
    any(p -> occursin(p, t), patterns)
end

# Capture a plot as PNG base64
function capture_plot_png(p)
    try
        # Try different methods to save the plot
        io = IOBuffer()

        # Method 1: Direct PNG show
        try
            show(io, MIME("image/png"), p)
            data = take!(io)
            if !isempty(data)
                return base64encode(data)
            end
        catch
        end

        # Method 2: Plots.jl savefig to tempfile
        if isdefined(Main, :Plots)
            try
                tmpfile = tempname() * ".png"
                Main.Plots.savefig(p, tmpfile)
                if isfile(tmpfile)
                    data = read(tmpfile)
                    rm(tmpfile)
                    return base64encode(data)
                end
            catch
            end
        end

        # Method 3: Makie save
        if isdefined(Main, :Makie) || isdefined(Main, :CairoMakie) || isdefined(Main, :GLMakie)
            try
                tmpfile = tempname() * ".png"
                makie_mod = isdefined(Main, :CairoMakie) ? Main.CairoMakie :
                           isdefined(Main, :GLMakie) ? Main.GLMakie : Main.Makie
                makie_mod.save(tmpfile, p)
                if isfile(tmpfile)
                    data = read(tmpfile)
                    rm(tmpfile)
                    return base64encode(data)
                end
            catch
            end
        end

        nothing
    catch e
        @warn "Failed to capture plot: $e"
        nothing
    end
end

# Simple capture without redirect (for debugging)
function capture_simple(f)
    result = CapturedOutput()
    try
        result.return_value = f()
    catch e
        result.error = e
        result.stacktrace = catch_backtrace()
    end
    result
end

# Capture output from code executed at TRUE top level via include_string
# This is how Jupyter does it - code runs at module top level, not inside a function
function capture_toplevel(mod::Module, code::String)
    result = CapturedOutput()

    # Create IO buffers for capturing output
    stdout_buf = IOBuffer()
    stderr_buf = IOBuffer()

    # Save original stdout/stderr
    old_stdout = stdout
    old_stderr = stderr

    try
        # Redirect output BEFORE executing code
        rd_out, wr_out = redirect_stdout()
        rd_err, wr_err = redirect_stderr()

        # Start tasks to read from pipes (capture only, no echo)
        stdout_task = @async begin
            while isopen(rd_out)
                data = String(readavailable(rd_out))
                write(stdout_buf, data)
            end
        end

        stderr_task = @async begin
            while isopen(rd_err)
                data = String(readavailable(rd_err))
                write(stderr_buf, data)
            end
        end

        # Execute code at TRUE top level - no function wrapper!
        try
            result.return_value = Base.include_string(mod, code)
        catch e
            result.error = e
            result.stacktrace = catch_backtrace()
        end

        # Restore stdout/stderr
        redirect_stdout(old_stdout)
        redirect_stderr(old_stderr)
        close(wr_out)
        close(wr_err)

        # Wait for capture tasks
        wait(stdout_task)
        wait(stderr_task)

    catch e
        # Ensure we restore stdout/stderr on error
        try redirect_stdout(old_stdout) catch end
        try redirect_stderr(old_stderr) catch end
        result.error = e
        result.stacktrace = catch_backtrace()
    end

    result.stdout = String(take!(stdout_buf))
    result.stderr = String(take!(stderr_buf))

    # Check for displayable plot
    if result.error === nothing && is_displayable_plot(result.return_value)
        img_b64 = capture_plot_png(result.return_value)
        if img_b64 !== nothing
            push!(result.images, ("png", img_b64))
        end
    end

    result
end

end # module
