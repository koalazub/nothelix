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
    notes = CellRegistry.provenance_notes(idx, analysis.uses)
    cell.notes = notes
    CellRegistry.CELLS[idx] = cell
    for v in analysis.defines
        CellRegistry.VARIABLE_SOURCES[v] = idx
    end
    notes
end

@testset "provenance notes" begin
    @testset "read above a below-writer emits a note" begin
        CellRegistry.clear_registry()
        run_cell!(5, "A = [1 2; 3 4]")
        notes = run_cell!(3, "eigen(A)")
        @test notes == ["note: A was last assigned by cell 5, below this cell"]
    end

    @testset "read below an upstream writer emits no note" begin
        CellRegistry.clear_registry()
        run_cell!(5, "A = [1 2; 3 4]")
        notes = run_cell!(7, "eigen(A)")
        @test notes == String[]
    end

    @testset "a same-cell re-read of a self-assigned name is not a hazard" begin
        CellRegistry.clear_registry()
        run_cell!(5, "A = 1")
        notes = run_cell!(5, "A = A + 1")
        @test notes == String[]
    end

    @testset "function-local names are not tracked" begin
        CellRegistry.clear_registry()
        run_cell!(5, "function f()\n    x = 1\n    x\nend")
        @test CellRegistry.CELLS[5].defines == Set([:f])
        @test !haskey(CellRegistry.VARIABLE_SOURCES, :x)
        notes = run_cell!(3, "x + 1")
        @test notes == String[]
    end

    @testset "tuple-destructuring targets are tracked" begin
        CellRegistry.clear_registry()
        run_cell!(5, "(a, b) = (10, 20)")
        @test CellRegistry.VARIABLE_SOURCES[:a] == 5
        @test CellRegistry.VARIABLE_SOURCES[:b] == 5
        notes = run_cell!(3, "a + b")
        @test "note: a was last assigned by cell 5, below this cell" in notes
        @test "note: b was last assigned by cell 5, below this cell" in notes
    end

    @testset "using/import names are ignored" begin
        CellRegistry.clear_registry()
        run_cell!(8, "using Statistics")
        run_cell!(9, "using LinearAlgebra: eigen")
        @test !haskey(CellRegistry.VARIABLE_SOURCES, :Statistics)
        @test !haskey(CellRegistry.VARIABLE_SOURCES, :mean)
        @test !haskey(CellRegistry.VARIABLE_SOURCES, :eigen)
        notes = run_cell!(3, "eigen(mean([1, 2, 3]))")
        @test notes == String[]
    end

    @testset "a stale writer whose current code drops the assignment is noted" begin
        CellRegistry.clear_registry()
        run_cell!(5, "A = 1\nB = 2")
        run_cell!(5, "A = 1")
        notes = run_cell!(3, "B + 1")
        @test notes == ["note: B was last assigned by cell 5, whose current code no longer assigns it"]
    end
end
