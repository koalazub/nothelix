using Test

include(joinpath(@__DIR__, "cell_registry.jl"))
include(joinpath(@__DIR__, "ast_analysis.jl"))
include(joinpath(@__DIR__, "output_capture.jl"))
include(joinpath(@__DIR__, "cell_macros.jl"))

using .CellMacros

@testset "result output is bounded" begin
    huge = collect(1.0:2_000_000.0)
    repr_text = CellMacros.get_output_repr(huge)
    @test sizeof(repr_text) < CellMacros.OUTPUT_REPR_MAX_BYTES + 256
    @test occursin("2000000-element", repr_text)

    long = repeat("x", 3 * CellMacros.OUTPUT_REPR_MAX_BYTES)
    bounded = CellMacros.bound_output_text(long)
    @test sizeof(bounded) < CellMacros.OUTPUT_REPR_MAX_BYTES + 256
    @test occursin("output truncated", bounded)
    @test occursin(string(sizeof(long)), bounded)

    multibyte = repeat("⋮", CellMacros.OUTPUT_REPR_MAX_BYTES)
    cut = CellMacros.bound_output_text(multibyte)
    @test isvalid(cut, lastindex(cut))

    small = "fine"
    @test CellMacros.bound_output_text(small) === small
end
