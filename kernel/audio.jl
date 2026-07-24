module AudioArtifacts

using ..CellRegistry

export set_audio_dir, wavplay_impl, write_pcm16_wav, parse_wav_duration_ms

const AUDIO_DIR = Ref{Union{String, Nothing}}(nothing)

function set_audio_dir(kernel_dir::String)
    dir = joinpath(kernel_dir, "audio")
    isdir(dir) || mkpath(dir)
    AUDIO_DIR[] = dir
end

function samples_matrix(y::AbstractVector{<:Real})
    reshape(collect(Float64, y), (length(y), 1))
end

function samples_matrix(y::AbstractMatrix{<:Real})
    Float64.(y)
end

function to_pcm16(sample::Float64)::Int16
    round(Int16, clamp(sample, -1.0, 1.0) * 32767)
end

function write_pcm16_wav(path::AbstractString, y, fs::Real)
    samples = samples_matrix(y)
    frames = size(samples, 1)
    channels = size(samples, 2)
    rate = round(UInt32, fs)
    bits = UInt16(16)
    block_align = UInt16(channels * 2)
    byte_rate = UInt32(rate) * UInt32(block_align)
    data_bytes = UInt32(frames * channels * 2)

    open(path, "w") do io
        write(io, b"RIFF")
        write(io, UInt32(36) + data_bytes)
        write(io, b"WAVE")
        write(io, b"fmt ")
        write(io, UInt32(16))
        write(io, UInt16(1))
        write(io, UInt16(channels))
        write(io, rate)
        write(io, byte_rate)
        write(io, block_align)
        write(io, bits)
        write(io, b"data")
        write(io, data_bytes)
        for frame in 1:frames
            for channel in 1:channels
                write(io, to_pcm16(samples[frame, channel]))
            end
        end
    end

    duration_ms = rate == 0 ? 0 : round(Int, frames / Float64(rate) * 1000)
    (frames = frames, channels = channels, rate = Int(rate), duration_ms = duration_ms)
end

function parse_wav_duration_ms(path::AbstractString)
    isfile(path) || return (0, "audio file not found: $path")
    try
        open(path, "r") do io
            String(read(io, 4)) == "RIFF" || return (0, "not a RIFF container")
            read(io, UInt32)
            String(read(io, 4)) == "WAVE" || return (0, "not a WAVE stream")
            channels = 0
            rate = 0
            bits = 0
            data_bytes = 0
            while !eof(io)
                id = String(read(io, 4))
                length(id) == 4 || break
                chunk_size = Int(read(io, UInt32))
                if id == "fmt "
                    read(io, UInt16)
                    channels = Int(read(io, UInt16))
                    rate = Int(read(io, UInt32))
                    read(io, UInt32)
                    read(io, UInt16)
                    bits = Int(read(io, UInt16))
                    skip(io, chunk_size - 16)
                elseif id == "data"
                    data_bytes = chunk_size
                    break
                else
                    skip(io, chunk_size + (chunk_size & 1))
                end
            end
            bytes_per_frame = channels * div(bits, 8)
            (rate > 0 && bytes_per_frame > 0) || return (0, "unparseable WAV header")
            frames = div(data_bytes, bytes_per_frame)
            (round(Int, frames / Float64(rate) * 1000), nothing)
        end
    catch e
        (0, "failed to parse WAV header: $e")
    end
end

function register!(path::AbstractString, duration_ms::Int, reason)
    idx = CellRegistry.current_cell()
    haskey(CellRegistry.CELLS, idx) || return nothing
    entry = Dict{String, Any}("path" => abspath(path), "duration_ms" => duration_ms)
    reason === nothing || (entry["reason"] = reason)
    push!(CellRegistry.CELLS[idx].audio, entry)
    nothing
end

function next_clip_path()::String
    dir = AUDIO_DIR[]
    dir === nothing && error("audio directory not set; call set_audio_dir first")
    idx = CellRegistry.current_cell()
    count = haskey(CellRegistry.CELLS, idx) ? length(CellRegistry.CELLS[idx].audio) : 0
    name = count == 0 ? "cell_$(idx).wav" : "cell_$(idx)_$(count).wav"
    joinpath(dir, name)
end

function wavplay_impl(y, fs::Real)
    path = next_clip_path()
    written = write_pcm16_wav(path, y, fs)
    register!(path, written.duration_ms, nothing)
    nothing
end

function wavplay_impl(filename::AbstractString)
    duration_ms, reason = parse_wav_duration_ms(filename)
    register!(filename, duration_ms, reason)
    nothing
end

end # module
