using Test

include(joinpath(@__DIR__, "cell_registry.jl"))
include(joinpath(@__DIR__, "ast_analysis.jl"))

using .CellRegistry
using .ASTAnalysis

function run_cell!(idx::Int, code::String)
    analysis = ASTAnalysis.analyze_code(code)
    cell = CellRegistry.Cell(idx)
    cell.code_string = code
    cell.defines = analysis.defines
    cell.uses = analysis.uses
    cell.status = :done
    cell.run_seq = CellRegistry.next_run_seq!()
    CellRegistry.CELLS[idx] = cell
    for v in analysis.defines
        CellRegistry.VARIABLE_SOURCES[v] = idx
    end
    cell
end

state_of(idx::Int) = CellRegistry.classify_all()[string(idx)]["state"]
inputs_of(idx::Int) = CellRegistry.classify_all()[string(idx)]["inputs"]

@testset "cell classifier" begin
    @testset "a clean top-to-bottom run leaves every cell fresh" begin
        CellRegistry.clear_registry()
        run_cell!(0, "A = 1")
        run_cell!(1, "B = A + 1")
        run_cell!(2, "C = B + A")
        @test state_of(0) == "fresh"
        @test state_of(1) == "fresh"
        @test state_of(2) == "fresh"
        @test length(inputs_of(2)) == 2
    end

    @testset "a read whose writer sits below is out-of-order" begin
        CellRegistry.clear_registry()
        run_cell!(5, "A = [1 2; 3 4]")
        run_cell!(3, "eigen(A)")
        @test state_of(3) == "out-of-order"
        detail = inputs_of(3)[1]
        @test detail["name"] == "A"
        @test detail["writer"] == 5
        @test detail["rel"] == "below"
    end

    @testset "a writer that re-runs after its reader makes the reader stale-input" begin
        CellRegistry.clear_registry()
        run_cell!(0, "A = 1")
        run_cell!(1, "B = A + 1")
        @test state_of(1) == "fresh"
        run_cell!(0, "A = 2")
        @test state_of(1) == "stale-input"
        @test inputs_of(1)[1]["rel"] == "stale"
    end

    @testset "a writer that drops the assignment orphans its reader" begin
        CellRegistry.clear_registry()
        run_cell!(0, "A = 1\nB = 2")
        run_cell!(1, "C = B + 1")
        @test state_of(1) == "fresh"
        run_cell!(0, "A = 1")
        @test state_of(1) == "orphan-input"
        @test inputs_of(1)[1]["rel"] == "orphan"
    end

    @testset "an unexecuted cell is absent from the classification" begin
        CellRegistry.clear_registry()
        run_cell!(0, "A = 1")
        pending = CellRegistry.Cell(9)
        pending.status = :pending
        CellRegistry.CELLS[9] = pending
        @test !haskey(CellRegistry.classify_all(), "9")
        @test haskey(CellRegistry.classify_all(), "0")
    end

    @testset "imported and builtin reads never trigger a state" begin
        CellRegistry.clear_registry()
        run_cell!(8, "using Statistics")
        run_cell!(3, "eigen(mean([1, 2, 3]))")
        @test state_of(3) == "fresh"
        @test isempty(inputs_of(3))
    end
end
