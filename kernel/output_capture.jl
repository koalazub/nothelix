module OutputCapture

using Base64
using Dates

export CapturedOutput, capture_execution, capture_toplevel, is_displayable_plot, capture_plot_png, extract_plot_data, set_log_file

# Log to kernel directory if available (set by runner.jl)
const CAPTURE_LOG_FILE = Ref{Union{String, Nothing}}(nothing)

function capture_log(msg::String)
    log_file = CAPTURE_LOG_FILE[]
    if log_file !== nothing
        timestamp = Dates.format(now(), "yyyy-mm-dd HH:MM:SS.sss")
        open(log_file, "a") do io
            println(io, "[$timestamp] [CAPTURE] $msg")
        end
    end
end

function set_log_file(path::String)
    CAPTURE_LOG_FILE[] = path
end

mutable struct CapturedOutput
    return_value::Any
    stdout::String
    stderr::String
    images::Vector{Tuple{String, String}}  # (format, base64_data)
    plot_data::Union{Vector{Dict{String,Any}}, Nothing}  # raw x/y series for interactive charts
    error::Union{Exception, Nothing}
    stacktrace::Union{Vector, Nothing}
end

CapturedOutput() = CapturedOutput(nothing, "", "", [], nothing, nothing, nothing)

function capture_execution(f)
    result = CapturedOutput()

    # Create IO buffers for capturing output
    stdout_buf = IOBuffer()
    stderr_buf = IOBuffer()

    # Capture stdout and stderr
    old_stdout = stdout
    old_stderr = stderr

    # Track if code execution itself had an error
    code_error = nothing
    code_stacktrace = nothing

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
            code_error = e
            code_stacktrace = catch_backtrace()
        end

        # Restore stdout/stderr - these should not fail, but if they do, don't
        # override a code execution error
        try redirect_stdout(old_stdout) catch end
        try redirect_stderr(old_stderr) catch end
        try close(wr_out) catch end
        try close(wr_err) catch end

        # Wait for capture tasks (with timeout to prevent hanging)
        try
            wait(stdout_task)
            wait(stderr_task)
        catch
            # Ignore task wait errors
        end

    catch e
        # Only set error if code execution didn't already have an error
        if code_error === nothing
            code_error = e
            code_stacktrace = catch_backtrace()
        end
        # Ensure we restore stdout/stderr on error
        try redirect_stdout(old_stdout) catch end
        try redirect_stderr(old_stderr) catch end
    end

    # Set the error from code execution (not cleanup)
    result.error = code_error
    result.stacktrace = code_stacktrace

    result.stdout = String(take!(stdout_buf))
    result.stderr = String(take!(stderr_buf))

    # Check for displayable plot
    if result.error === nothing && is_displayable_plot(result.return_value)
        img_b64 = capture_plot_png(result.return_value)
        if img_b64 !== nothing
            push!(result.images, ("png", img_b64))
        end
        result.plot_data = extract_plot_data(result.return_value)
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
# Uses Base.invokelatest to handle world age issues when code is evaluated with Core.eval
function capture_plot_png(p)
    capture_log("capture_plot_png called with type: $(typeof(p))")
    
    # Method 1: Plots.jl savefig to tempfile (most reliable for Plots.jl)
    plots_defined = isdefined(Main, :Plots)
    capture_log("Plots defined in Main: $plots_defined")
    
    if plots_defined
        try
            tmpfile = tempname() * ".png"
            capture_log("Attempting savefig to: $tmpfile")
            Base.invokelatest(Main.Plots.savefig, p, tmpfile)
            if isfile(tmpfile)
                data = read(tmpfile)
                capture_log("savefig succeeded, file size: $(length(data)) bytes")
                rm(tmpfile)
                return base64encode(data)
            else
                capture_log("savefig: file not created")
            end
        catch e
            capture_log("savefig failed: $e")
        end
    end

    # Method 2: Direct PNG show via MIME
    try
        io = IOBuffer()
        capture_log("Attempting MIME show")
        Base.invokelatest(show, io, MIME("image/png"), p)
        data = take!(io)
        if !isempty(data)
            capture_log("MIME show succeeded, size: $(length(data)) bytes")
            return base64encode(data)
        else
            capture_log("MIME show: empty data")
        end
    catch e
        capture_log("MIME show failed: $e")
    end

    # Method 3: Makie save
    makie_defined = isdefined(Main, :Makie) || isdefined(Main, :CairoMakie) || isdefined(Main, :GLMakie)
    capture_log("Makie defined: $makie_defined")
    
    if makie_defined
        try
            tmpfile = tempname() * ".png"
            makie_mod = isdefined(Main, :CairoMakie) ? Main.CairoMakie :
                       isdefined(Main, :GLMakie) ? Main.GLMakie : Main.Makie
            Base.invokelatest(makie_mod.save, tmpfile, p)
            if isfile(tmpfile)
                data = read(tmpfile)
                capture_log("Makie save succeeded, size: $(length(data)) bytes")
                rm(tmpfile)
                return base64encode(data)
            end
        catch e
            capture_log("Makie save failed: $e")
        end
    end

    capture_log("All methods failed for type: $(typeof(p))")
    nothing
end

# Extract raw (x, y, label) data from a plot object for interactive braille charts.
# Returns a Vector of Dicts, one per series, or nothing if extraction fails.
function extract_plot_data(p)
    capture_log("extract_plot_data called with type: $(typeof(p))")

    # ── Plots.jl ──────────────────────────────────────────────────────────
    if isdefined(Main, :Plots)
        try
            type_str = string(typeof(p))
            if occursin("Plot", type_str) && hasproperty(p, :series_list)
                series_data = Dict{String,Any}[]
                for (i, series) in enumerate(p.series_list)
                    try
                        x_raw = Base.invokelatest(getindex, series, :x)
                        y_raw = Base.invokelatest(getindex, series, :y)
                        label_raw = Base.invokelatest(getindex, series, :label)

                        x = Float64.(collect(x_raw))
                        y = Float64.(collect(y_raw))
                        label = string(label_raw)

                        entry = Dict{String,Any}(
                            "x" => x,
                            "y" => y,
                            "label" => label,
                            "series_index" => i
                        )

                        try
                            st = Base.invokelatest(getindex, series, :seriestype)
                            entry["series_type"] = string(st)
                        catch; end

                        push!(series_data, entry)
                    catch e
                        capture_log("Failed to extract series $i: $e")
                    end
                end

                if !isempty(series_data)
                    capture_log("Extracted $(length(series_data)) series from Plots.jl")
                    return series_data
                end
            end
        catch e
            capture_log("Plots.jl extraction failed: $e")
        end
    end

    # ── Makie / CairoMakie ────────────────────────────────────────────────
    makie_mod = if isdefined(Main, :CairoMakie)
        Main.CairoMakie
    elseif isdefined(Main, :GLMakie)
        Main.GLMakie
    elseif isdefined(Main, :Makie)
        Main.Makie
    else
        nothing
    end

    if makie_mod !== nothing
        try
            type_str = string(typeof(p))
            if occursin("Figure", type_str) || occursin("FigureAxis", type_str)
                series_data = Dict{String,Any}[]
                fig = occursin("FigureAxis", type_str) ? p[1] : p
                contents = Base.invokelatest(getproperty, fig, :content)
                series_idx = 0
                for block in contents
                    if hasproperty(block, :scene)
                        scene = Base.invokelatest(getproperty, block, :scene)
                        plots = Base.invokelatest(getproperty, scene, :plots)
                        for plot_obj in plots
                            try
                                converted = Base.invokelatest(getindex, plot_obj, 1)
                                points = converted[]
                                if !isempty(points)
                                    x = Float64[pt[1] for pt in points]
                                    y = Float64[pt[2] for pt in points]
                                    series_idx += 1
                                    label = ""
                                    try
                                        attrs = Base.invokelatest(getproperty, plot_obj, :attributes)
                                        if haskey(attrs, :label)
                                            label = string(attrs.label[])
                                        end
                                    catch; end
                                    push!(series_data, Dict{String,Any}(
                                        "x" => x, "y" => y,
                                        "label" => label,
                                        "series_index" => series_idx,
                                        "series_type" => string(typeof(plot_obj).name.name)
                                    ))
                                end
                            catch; end
                        end
                    end
                end
                if !isempty(series_data)
                    capture_log("Extracted $(length(series_data)) series from Makie")
                    return series_data
                end
            end
        catch e
            capture_log("Makie extraction failed: $e")
        end
    end

    capture_log("No plot data extracted for type: $(typeof(p))")
    nothing
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

    # Track if code execution itself had an error
    code_error = nothing
    code_stacktrace = nothing

    # Track whether we pushed a replacement TextDisplay so we can
    # pop exactly what we pushed (and not anything the user's code
    # may have pushed during execution).
    display_pushed = false

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

        # `Base.Multimedia.displays` was built at kernel startup with
        # a `TextDisplay(stdout)` that holds the ORIGINAL stdout ref,
        # so `display(x)` still writes to the terminal even after we
        # redirected `stdout` above. That's why a cell like
        # `display(A); print(A)` showed nothing from `display` — it
        # escaped the pipe and went to the kernel's real stdout.
        #
        # Push a fresh TextDisplay that closes over the *new* (piped)
        # stdout so `display(x)` routes through our capture. Pop it in
        # the `finally`-ish cleanup below.
        try
            pushdisplay(TextDisplay(stdout))
            display_pushed = true
        catch e
            capture_log("pushdisplay(TextDisplay(stdout)) failed: $e")
        end

        # Execute code with REPL-like soft scope semantics
        # Julia 1.5+ introduced "hard scope" for non-REPL contexts which causes
        # assignments to create local variables instead of global ones.
        # Using Meta.parse + Core.eval ensures variables persist in the module.
        try
            # Parse and evaluate each top-level expression separately
            # This mimics REPL behaviour where each line is evaluated at module scope
            exprs = Meta.parseall(code)
            local last_result = nothing
            for expr in exprs.args
                if expr isa LineNumberNode
                    continue
                end
                last_result = Core.eval(mod, expr)
            end
            result.return_value = last_result
        catch e
            code_error = e
            code_stacktrace = catch_backtrace()
        end

        # Pop the TextDisplay we pushed above (if any) so we don't
        # leave the display stack growing across cell executions.
        if display_pushed
            try popdisplay() catch e
                capture_log("popdisplay() failed: $e")
            end
            display_pushed = false
        end

        # Restore stdout/stderr - these should not fail, but if they do, don't
        # override a code execution error
        try redirect_stdout(old_stdout) catch end
        try redirect_stderr(old_stderr) catch end
        try close(wr_out) catch end
        try close(wr_err) catch end

        # Wait for capture tasks (with timeout to prevent hanging)
        try
            wait(stdout_task)
            wait(stderr_task)
        catch
            # Ignore task wait errors
        end

    catch e
        # Only set error if code execution didn't already have an error
        if code_error === nothing
            code_error = e
            code_stacktrace = catch_backtrace()
        end
        # Ensure we restore stdout/stderr and pop display on error
        if display_pushed
            try popdisplay() catch end
            display_pushed = false
        end
        try redirect_stdout(old_stdout) catch end
        try redirect_stderr(old_stderr) catch end
    end

    # Set the error from code execution (not cleanup)
    result.error = code_error
    result.stacktrace = code_stacktrace

    result.stdout = String(take!(stdout_buf))
    result.stderr = String(take!(stderr_buf))

    # Check for displayable plot
    if result.error === nothing && is_displayable_plot(result.return_value)
        img_b64 = capture_plot_png(result.return_value)
        if img_b64 !== nothing
            push!(result.images, ("png", img_b64))
        end
        result.plot_data = extract_plot_data(result.return_value)
    end

    result
end

end # module
