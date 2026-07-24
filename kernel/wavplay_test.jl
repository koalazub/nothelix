using Test

include(joinpath(@__DIR__, "cell_registry.jl"))
include(joinpath(@__DIR__, "audio.jl"))

using .CellRegistry
using .AudioArtifacts

le_u16(bytes, o) = Int(bytes[o]) | (Int(bytes[o + 1]) << 8)
le_u32(bytes, o) = le_u16(bytes, o) | (le_u16(bytes, o + 2) << 16)
le_i16(bytes, o) = reinterpret(Int16, UInt16(le_u16(bytes, o)))
tag(bytes, o) = String(bytes[o:o + 3])

@testset "PCM16 WAV writer" begin
    @testset "mono round trip: header fields, byte length, samples, duration" begin
        path = tempname() * ".wav"
        written = AudioArtifacts.write_pcm16_wav(path, Float64[0.0, 1.0, -1.0, 0.5], 4)
        bytes = read(path)
        rm(path)

        @test written.frames == 4
        @test written.channels == 1
        @test written.rate == 4
        @test written.duration_ms == 1000

        @test length(bytes) == 44 + 4 * 1 * 2
        @test tag(bytes, 1) == "RIFF"
        @test le_u32(bytes, 5) == 36 + 8
        @test tag(bytes, 9) == "WAVE"
        @test tag(bytes, 13) == "fmt "
        @test le_u32(bytes, 17) == 16
        @test le_u16(bytes, 21) == 1
        @test le_u16(bytes, 23) == 1
        @test le_u32(bytes, 25) == 4
        @test le_u32(bytes, 29) == 4 * 1 * 2
        @test le_u16(bytes, 33) == 2
        @test le_u16(bytes, 35) == 16
        @test tag(bytes, 37) == "data"
        @test le_u32(bytes, 41) == 8

        @test le_i16(bytes, 45) == 0
        @test le_i16(bytes, 47) == 32767
        @test le_i16(bytes, 49) == -32767
        @test le_i16(bytes, 51) == 16384
    end

    @testset "clamps out-of-range floats to the PCM16 rails" begin
        path = tempname() * ".wav"
        AudioArtifacts.write_pcm16_wav(path, Float64[2.5, -2.5], 8000)
        bytes = read(path)
        rm(path)
        @test le_i16(bytes, 45) == 32767
        @test le_i16(bytes, 47) == -32767
    end

    @testset "matrix columns are interleaved as channels" begin
        path = tempname() * ".wav"
        written = AudioArtifacts.write_pcm16_wav(path, [0.0 0.5; 1.0 -1.0], 8000)
        bytes = read(path)
        rm(path)
        @test written.channels == 2
        @test written.frames == 2
        @test le_u16(bytes, 23) == 2
        @test length(bytes) == 44 + 2 * 2 * 2
        @test le_i16(bytes, 45) == 0
        @test le_i16(bytes, 47) == 16384
        @test le_i16(bytes, 49) == 32767
        @test le_i16(bytes, 51) == -32767
    end

    @testset "header parse recovers the duration a fresh write reported" begin
        path = tempname() * ".wav"
        written = AudioArtifacts.write_pcm16_wav(path, zeros(Float64, 16000), 8000)
        duration_ms, reason = AudioArtifacts.parse_wav_duration_ms(path)
        rm(path)
        @test reason === nothing
        @test duration_ms == written.duration_ms == 2000
    end

    @testset "an absent file parses to a zero duration with a reason" begin
        duration_ms, reason = AudioArtifacts.parse_wav_duration_ms(tempname() * ".wav")
        @test duration_ms == 0
        @test reason isa AbstractString
    end
end

@testset "wavplay registers an artifact on the running cell" begin
    dir = mktempdir()
    AudioArtifacts.set_audio_dir(dir)
    CellRegistry.clear_registry()
    CellRegistry.CELLS[3] = CellRegistry.Cell(3)
    CellRegistry.set_current_cell!(3)

    AudioArtifacts.wavplay_impl(Float64[0.0, 1.0], 8000)
    AudioArtifacts.wavplay_impl(Float64[0.0, 1.0], 8000)

    arts = CellRegistry.CELLS[3].audio
    @test length(arts) == 2
    @test isfile(arts[1]["path"])
    @test basename(arts[1]["path"]) == "cell_3.wav"
    @test basename(arts[2]["path"]) == "cell_3_1.wav"
    @test arts[1]["duration_ms"] isa Int
end

@testset "module shadowing: a local wavplay survives a soft using" begin
    @eval module StandInProvider
        export wavplay
        wavplay(args...) = :from_provider
    end

    @eval module ShimWithStandIn
        wavplay(y, fs) = :from_shim
        using ..StandInProvider
    end

    @test ShimWithStandIn.wavplay(1, 2) == :from_shim

    has_wav = try
        @eval using WAV
        true
    catch
        false
    end

    if has_wav
        @eval module ShimWithWAV
            wavplay(y, fs) = :from_shim
            using WAV
        end
        @test ShimWithWAV.wavplay(Float64[0.0], 8000) == :from_shim
    end
end
