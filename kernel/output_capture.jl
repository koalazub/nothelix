module OutputCapture

using Base64
using Dates
using ..CellRegistry

export CapturedOutput, capture_execution, capture_toplevel, is_displayable_plot, capture_plot_png, capture_animated_output, extract_plot_data, is_unicode_plot, parse_ansi_rows, capture_unicode_plot_text, set_log_file, set_kernel_dir, plot_route

# Log to kernel directory if available (set by runner.jl)
const CAPTURE_LOG_FILE = Ref{Union{String, Nothing}}(nothing)

# Kernel directory for writing sidecar image files (set by runner.jl)
const KERNEL_DIR = Ref{Union{String, Nothing}}(nothing)
const IMAGE_COUNTER = Ref{Int}(0)

function set_kernel_dir(path::String)
    KERNEL_DIR[] = path
end

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
    images::Vector{Tuple{String, String}}
    text_plots::Vector{Dict{String,Any}}  # UnicodePlots braille: {"rows"=>[...], "spans"=>[...]}
    plot_data::Union{Vector{Dict{String,Any}}, Nothing}
    error::Union{Exception, Nothing}
    stacktrace::Union{Vector, Nothing}
    structured_error::Union{Dict{String,Any}, Nothing}
end

CapturedOutput() = CapturedOutput(nothing, "", "", [], [], nothing, nothing, nothing, nothing)

"""
Extract line and column from a ParseError message.
Scans for "Error @ file:LINE:COL" pattern.
Returns (line, col) tuple or nothing.
"""
function extract_parse_error_location(msg::String)
    for line in split(msg, '\n')
        stripped = lstrip(lstrip(strip(line), '#'))
        idx = findfirst("Error @ ", stripped)
        idx === nothing && continue
        after_at = stripped[idx[end]+1:end]
        # Split from the right on ':' to get file:line:col
        parts = split(after_at, ':')
        length(parts) < 3 && continue
        col_str = strip(parts[end])
        line_str = strip(parts[end-1])
        all(isdigit, col_str) && all(isdigit, line_str) || continue
        return (parse(Int, line_str), parse(Int, col_str))
    end
    nothing
end

"""
Extract structured error metadata for the Rust error formatter.
Returns a Dict with: error_type, message, frames, source_line,
cell_index, cell_line. Returns nothing if no error.
"""
function extract_structured_error(error, stacktrace, code::String, cell_index::Int)
    error === nothing && return nothing
    frames = Dict{String,Any}[]
    code_lines = split(code, '\n')

    if stacktrace !== nothing
        for (i, frame) in enumerate(stacktrace)
            file = string(frame.file)
            is_user = occursin("<cell>", file) || occursin("none", file) ||
                      startswith(file, "REPL") || file == "none"
            push!(frames, Dict{String,Any}(
                "file" => file,
                "line" => frame.line,
                "func" => string(frame.func),
                "is_user_code" => is_user
            ))
        end
    end

    # Find the source line from the stacktrace
    source_line = ""
    cell_line = 0
    for f in frames
        if f["is_user_code"] && f["line"] > 0
            line_idx = f["line"]
            if 1 <= line_idx <= length(code_lines)
                source_line = code_lines[line_idx]
                cell_line = line_idx
            end
            break
        end
    end

    # For ParseError: extract line/col from the error message since the
    # stacktrace won't contain a useful user-code frame.
    if source_line == "" && error isa Base.Meta.ParseError
        parse_loc = extract_parse_error_location(sprint(showerror, error))
        if parse_loc !== nothing
            parse_line, parse_col = parse_loc
            if 1 <= parse_line <= length(code_lines)
                source_line = code_lines[parse_line]
                cell_line = parse_line
            end
        end
    end

    result = Dict{String,Any}(
        "error_type" => string(typeof(error)),
        "message" => sprint(showerror, error),
        "frames" => frames,
        "source_line" => source_line,
        "cell_index" => cell_index,
        "cell_line" => cell_line
    )

    # Cross-cell context: for UndefVarError, show which cell defines the variable
    if error isa UndefVarError
        var_sym = error.var
        cell_context = CellRegistry.lookup_variable_context(Set([var_sym]))
        if !isempty(cell_context)
            result["cell_context"] = cell_context
        end
    end

    # Type-aware hints for MethodError. Rust-side enricher consumes two
    # maps we attach here:
    #   - in_scope_variable_types: `Dict{typeof_str => [{name, cell}, …]}`,
    #     so the enricher can answer "which in-scope variables are that
    #     type?" for each ::T in the error signature. Lets us write
    #     "Vector{ComplexF64} in scope: `eigenvalues` (cell 17)" instead
    #     of just echoing the type the user already saw.
    #   - method_candidates: in-scope values that the failing function
    #     does have a method for (when the error is a single-arg call).
    #     Computed via `hasmethod(f, Tuple{typeof(val)})` — guarded by
    #     try/catch because some exotic types can't be reflected on.
    if error isa MethodError
        scope_types = CellRegistry.in_scope_variables_by_type()
        if !isempty(scope_types)
            result["in_scope_variable_types"] = scope_types
        end

        if isdefined(error, :f) && isdefined(error, :args) && length(error.args) == 1
            f = error.f
            candidates = Dict{String, Any}[]
            for (sym, typ) in CellRegistry.VARIABLE_TYPES
                isdefined(Main, sym) || continue
                try
                    val = getfield(Main, sym)
                    if hasmethod(f, Tuple{typeof(val)})
                        src_idx = get(CellRegistry.VARIABLE_SOURCES, sym, -1)
                        push!(candidates, Dict{String, Any}(
                            "name" => string(sym),
                            "type" => typ,
                            "cell" => src_idx,
                        ))
                    end
                catch
                end
            end
            if !isempty(candidates)
                result["method_candidates"] = candidates
            end
        end
    end

    # For any error: check if the current cell has unexecuted dependencies
    unexec = CellRegistry.unexecuted_dependencies(cell_index)
    if !isempty(unexec)
        result["unexecuted_deps"] = unexec
    end

    result
end

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
        animated = capture_animated_output(result.return_value)
        if animated !== nothing
            ext = mime_to_extension(animated[1])
            push!(result.images, ("plot.$ext", animated[2]))
        else
            img_b64 = capture_plot_png(result.return_value)
            if img_b64 !== nothing
                push!(result.images, ("png", img_b64))
            end
        end
        result.plot_data = extract_plot_data(result.return_value)
    end

    result
end

const PLOT_TYPE_PATTERNS = ("Plot", "Figure", "Scene", "FigureAxis", "Chart", "Canvas", "Drawing", "GtkCanvas")

# Check if a value is a displayable plot
function is_displayable_plot(x)
    x === nothing && return false
    T = typeof(x)
    # Fast path: check by module name
    mod_name = nameof(parentmodule(T))
    mod_name in (:Plots, :Makie, :CairoMakie, :GLMakie, :WGLMakie) && return true
    # Fallback: string match on type name
    t = string(nameof(T))
    any(p -> occursin(p, t), PLOT_TYPE_PATTERNS)
end

# Check if a value is a UnicodePlots plot (braille/Unicode terminal plot).
# Guarded with try/catch + invokelatest for world-age safety — harmless
# (returns false) when UnicodePlots isn't loaded in the session.
function is_unicode_plot(x)
    x === nothing && return false
    try
        T = Base.invokelatest(typeof, x)
        mod = Base.invokelatest(parentmodule, T)
        Base.invokelatest(nameof, mod) === :UnicodePlots && return true
        is_unicodeplots_backend_plot(x)
    catch
        false
    end
end

# A Plots.jl figure rendered with the UnicodePlots backend shows as an ANSI
# text canvas, so it routes through the braille path like a native
# UnicodePlots value (covers `plot(...; seriestype=:path3d)` under
# `unicodeplots()`). Guarded — false when Plots isn't loaded.
function is_unicodeplots_backend_plot(x)
    x === nothing && return false
    try
        T = Base.invokelatest(typeof, x)
        Base.invokelatest(nameof, T) === :Plot || return false
        mod = Base.invokelatest(parentmodule, T)
        Base.invokelatest(nameof, mod) === :Plots || return false
        isdefined(Main, :Plots) || return false
        plots_mod = getfield(Main, :Plots)
        isdefined(plots_mod, :backend) || return false
        b = Base.invokelatest(plots_mod.backend)
        occursin("UnicodePlots", string(Base.invokelatest(typeof, b)))
    catch
        false
    end
end

# Decide how an already-produced top-level value should be routed for
# display, given the request's `plot_mode` ("auto"|"raster"|"braille")
# and whether the value itself is a UnicodePlots plot.
#
#   "auto"    — today's behaviour: a UnicodePlots value renders braille,
#               anything else falls through to the raster/repr path.
#   "braille" — a UnicodePlots value renders braille, same as "auto". A
#               NON-UnicodePlots value (e.g. a Plots.jl figure) is never
#               converted to braille — it still falls through to raster,
#               same as "auto". The only observable difference "braille"
#               makes is at the call site: it force-triggers the
#               UnicodePlots self-heal even when the cell's source text
#               doesn't mention "UnicodePlots".
#   "raster"  — a UnicodePlots value is NOT routed to braille even
#               though `is_unicode` is true. It falls through to
#               :raster, where `is_displayable_plot`'s string-fallback
#               still matches UnicodePlots' `Plot` type name, so
#               `capture_plot_png` is attempted — and fails (UnicodePlots
#               can't produce a PNG), leaving the value to render as
#               plain text repr.
#
# Pure and total over the 3 modes x 2 `is_unicode` combinations — no I/O,
# no globals — so it's unit-testable directly.
function plot_route(mode::String, is_unicode::Bool)
    is_unicode && mode != "raster" && return :braille
    :raster
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
                if KERNEL_DIR[] !== nothing
                    IMAGE_COUNTER[] += 1
                    filename = "image_$(IMAGE_COUNTER[]).png"
                    filepath = joinpath(KERNEL_DIR[], filename)
                    write(filepath, data)
                    return "file:$filename"
                else
                    return base64encode(data)
                end
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
            if KERNEL_DIR[] !== nothing
                IMAGE_COUNTER[] += 1
                filename = "image_$(IMAGE_COUNTER[]).png"
                filepath = joinpath(KERNEL_DIR[], filename)
                write(filepath, data)
                return "file:$filename"
            else
                return base64encode(data)
            end
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
                if KERNEL_DIR[] !== nothing
                    IMAGE_COUNTER[] += 1
                    filename = "image_$(IMAGE_COUNTER[]).png"
                    filepath = joinpath(KERNEL_DIR[], filename)
                    write(filepath, data)
                    return "file:$filename"
                else
                    return base64encode(data)
                end
            end
        catch e
            capture_log("Makie save failed: $e")
        end
    end

    capture_log("All methods failed for type: $(typeof(p))")
    nothing
end

# Parse an ANSI-SGR-colored string (as UnicodePlots emits for `show`) into
# stripped rows + a list of non-default-color spans.
#
# A scanner, NOT regex: walks the string char-by-char. On `\e[` it enters
# escape state, reads the numeric SGR params up to the terminating `m`,
# and updates the current foreground color (30-37 -> 0-7, 90-97 -> 8-15;
# reset on 0 or 39). Every other char is stripped of escapes and appended
# to the current row; rows are split on '\n'. Positions are 0-based CHAR
# offsets (not bytes) into the stripped row, counted via `collect(s)` so
# multi-byte glyphs (braille dots) count as one position.
#
# Returns `(rows, spans)` where `spans` is a `Vector` of
# `[row, start, end, color]` (end EXCLUSIVE) for each maximal run of a
# single non-default color. A color left open at end-of-row (no reset
# before the newline) closes there and reopens at column 0 of the next
# row. Unknown/unsupported SGR codes (bg colors, 256-color, bold, etc.)
# are ignored and leave the current fg color unchanged.
function parse_ansi_rows(s::String)
    chars = collect(s)
    n = length(chars)

    rows = String[]
    spans = Vector{Int}[]
    current_row = Char[]

    row_idx = 0
    col = 0
    current_color = nothing   # Union{Int, Nothing} — active fg color, or default
    span_start = nothing      # col where the open span began

    function close_span!()
        if current_color !== nothing && span_start !== nothing
            push!(spans, [row_idx, span_start, col, current_color])
        end
    end

    i = 1
    while i <= n
        c = chars[i]
        if c == '\e' && i < n && chars[i + 1] == '['
            i += 2
            param_start = i
            while i <= n && chars[i] != 'm'
                i += 1
            end
            if i <= n && chars[i] == 'm'
                param_str = String(chars[param_start:i - 1])
                i += 1
                params = isempty(param_str) ? ["0"] : split(param_str, ';')
                for p in params
                    code = tryparse(Int, p)
                    code === nothing && continue
                    if code == 0 || code == 39
                        close_span!()
                        current_color = nothing
                        span_start = nothing
                    elseif 30 <= code <= 37
                        new_color = code - 30
                        if current_color === nothing || current_color != new_color
                            close_span!()
                            span_start = col
                        end
                        current_color = new_color
                    elseif 90 <= code <= 97
                        new_color = code - 90 + 8
                        if current_color === nothing || current_color != new_color
                            close_span!()
                            span_start = col
                        end
                        current_color = new_color
                    end
                    # unknown SGR codes are ignored — they don't change fg color
                end
            end
        elseif c == '\n'
            close_span!()
            push!(rows, String(current_row))
            current_row = Char[]
            row_idx += 1
            col = 0
            span_start = current_color === nothing ? nothing : 0
            i += 1
        else
            push!(current_row, c)
            col += 1
            i += 1
        end
    end

    close_span!()
    push!(rows, String(current_row))

    (rows, spans)
end

# Capture a UnicodePlots plot as braille text + ANSI color spans.
# Returns a Dict{"rows"=>[...], "spans"=>[...]} on success, or nothing.
function capture_unicode_plot_text(p)
    capture_log("capture_unicode_plot_text called with type: $(typeof(p))")
    try
        io = IOBuffer()
        ioc = IOContext(io, :color => true)
        Base.invokelatest(show, ioc, MIME("text/plain"), p)
        ansi = String(take!(io))
        rows, spans = parse_ansi_rows(ansi)
        capture_log("capture_unicode_plot_text: $(length(rows)) rows, $(length(spans)) spans")
        return Dict{String,Any}("rows" => rows, "spans" => spans)
    catch e
        capture_log("capture_unicode_plot_text failed: $e")
        return nothing
    end
end

const ANIMATED_MIMES = [
    "image/gif",
    "image/apng",
    "image/webp",
    "video/mp4",
    "video/webm",
    "application/json+lottie",
]

function mime_to_extension(mime::String)
    return get(Dict(
        "image/gif"  => "gif",
        "image/apng" => "apng",
        "image/webp" => "webp",
        "video/mp4"  => "mp4",
        "video/webm" => "webm",
        "application/json+lottie" => "lottie",
    ), mime, "bin")
end

# Try to capture an animated-MIME representation. Returns (mime::String, b64::String)
# on success, or nothing if no animated MIME is showable.
function capture_animated_output(x)
    for mime in ANIMATED_MIMES
        try
            if Base.invokelatest(showable, MIME(mime), x)
                io = IOBuffer()
                Base.invokelatest(show, io, MIME(mime), x)
                data = take!(io)
                if !isempty(data)
                    return (mime, base64encode(data))
                end
            end
        catch e
            capture_log("animated MIME show failed for $mime: $e")
        end
    end
    return nothing
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
#
# `plot_mode` ("auto"|"raster"|"braille", default "auto") is the
# project's plot-mode config forwarded from the execute request; see
# `plot_route` above for the per-value routing it drives.
function capture_toplevel(mod::Module, code::String; plot_mode::String="auto")
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
            # Parse and evaluate each top-level expression separately.
            # Mimics REPL behaviour where each line is evaluated at module
            # scope AND Jupyter/IJulia behaviour where every top-level
            # expression producing a displayable value shows up in the
            # cell's output. Previously we only captured the LAST
            # expression's return — so a cell with two `plot(...)` calls
            # displayed only the second, and a cell where the last line
            # was `println(...)` displayed zero plots even when earlier
            # `plot(...)` calls existed.
            exprs = Meta.parseall(code)
            local last_result = nothing
            local current_line = LineNumberNode(0, :none)
            for expr in exprs.args
                if expr isa LineNumberNode
                    current_line = expr
                    continue
                end
                last_result = Core.eval(mod, Expr(:toplevel, current_line, expr))
                if last_result !== nothing
                    # `plot_route` decides braille vs raster from
                    # plot_mode + is_unicode_plot; see its docstring for
                    # the full "auto"/"raster"/"braille" table. Checked
                    # before is_displayable_plot because UnicodePlots's
                    # type name also matches the "Plot" string-fallback
                    # pattern there.
                    route = plot_route(plot_mode, is_unicode_plot(last_result))
                    if route === :braille
                        tp = capture_unicode_plot_text(last_result)
                        if tp !== nothing
                            push!(result.text_plots, tp)
                        end
                    elseif is_displayable_plot(last_result)
                        animated = capture_animated_output(last_result)
                        if animated !== nothing
                            ext = mime_to_extension(animated[1])
                            push!(result.images, ("plot.$ext", animated[2]))
                        else
                            img = capture_plot_png(last_result)
                            if img !== nothing
                                push!(result.images, ("png", img))
                            end
                        end
                        # plot_data drives interactive chart overlays; we
                        # only have one overlay slot per cell, so keep the
                        # first plot's data.
                        if result.plot_data === nothing
                            result.plot_data = extract_plot_data(last_result)
                        end
                    end
                end
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

    # Extract structured error for the Rust formatter
    if code_error !== nothing
        try
            result.structured_error = extract_structured_error(
                code_error, code_stacktrace, code, 0)
        catch end
    end

    # Plot capture now happens inside the per-expression eval loop above
    # so cells with multiple top-level plots render all of them, not
    # only the final return value. We deliberately don't re-capture the
    # return value here — it would double-register the last plot.

    result
end

end # module
