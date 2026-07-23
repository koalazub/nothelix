using Test

include(joinpath(@__DIR__, "cell_registry.jl"))
include(joinpath(@__DIR__, "ast_analysis.jl"))
include(joinpath(@__DIR__, "output_capture.jl"))
include(joinpath(@__DIR__, "cell_macros.jl"))

using .CellRegistry
using .ASTAnalysis
using .OutputCapture
using .CellMacros

@testset "cell duration" begin
    @testset "elapsed wall time lands on the cell record" begin
        CellRegistry.clear_registry()
        CellMacros.execute_cell(0, "sleep(0.02); 1 + 1")
        rec = CellRegistry.CELLS[0]
        @test rec.duration isa Int
        @test rec.duration >= 10
    end

    @testset "the duration rides the cell_states payload" begin
        CellRegistry.clear_registry()
        CellMacros.execute_cell(4, "2 + 2")
        entry = CellRegistry.classify_all()["4"]
        @test haskey(entry, "duration")
        @test entry["duration"] == CellRegistry.CELLS[4].duration
        @test entry["duration"] isa Int
    end

    @testset "a never-run cell carries a nothing duration" begin
        CellRegistry.clear_registry()
        pending = CellRegistry.Cell(9)
        @test pending.duration === nothing
    end
end
