using Test

include(joinpath(@__DIR__, "cell_registry.jl"))
include(joinpath(@__DIR__, "ast_analysis.jl"))
include(joinpath(@__DIR__, "widgets.jl"))

using .CellRegistry
using .ASTAnalysis
using .KernelWidgets

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

declare_slider!(idx::Int, mod::Module, name, lo, hi, step) = begin
    CellRegistry.set_current_cell!(idx)
    KernelWidgets.record_slider!(mod, name, lo, hi, step)
end

@testset "kernel widgets" begin
    @testset "record_slider! lands a spec on the current cell" begin
        CellRegistry.clear_registry()
        CellRegistry.CELLS[2] = CellRegistry.Cell(2)
        CellRegistry.set_current_cell!(2)
        @eval module SliderMod
            freq = 440
        end
        KernelWidgets.record_slider!(SliderMod, "freq", 220, 880, 10)
        specs = CellRegistry.CELLS[2].widgets
        @test length(specs) == 1
        @test specs[1]["kind"] == "slider"
        @test specs[1]["name"] == "freq"
        @test specs[1]["params"] == "220:880:10"
        @test specs[1]["current"] == "440"
    end

    @testset "record_choice! encodes the options and the current value" begin
        CellRegistry.clear_registry()
        CellRegistry.CELLS[3] = CellRegistry.Cell(3)
        CellRegistry.set_current_cell!(3)
        @eval module ChoiceMod
            wave = "sin"
        end
        KernelWidgets.record_choice!(ChoiceMod, "wave", ["sin", "cos", "tan"])
        specs = CellRegistry.CELLS[3].widgets
        @test length(specs) == 1
        @test specs[1]["kind"] == "choice"
        @test specs[1]["params"] == "sin|cos|tan"
        @test specs[1]["current"] == "sin"
    end

    @testset "a spec whose variable is undefined carries an empty current" begin
        CellRegistry.clear_registry()
        CellRegistry.CELLS[4] = CellRegistry.Cell(4)
        CellRegistry.set_current_cell!(4)
        @eval module BareMod end
        KernelWidgets.record_slider!(BareMod, "g", 0, 1, 0)
        @test CellRegistry.CELLS[4].widgets[1]["current"] == ""
    end

    @testset "a spec records nothing when there is no current cell" begin
        CellRegistry.clear_registry()
        @eval module OrphanMod
            a = 1
        end
        KernelWidgets.record_slider!(OrphanMod, "a", 0, 10, 1)
        @test isempty(CellRegistry.CELLS)
    end

    @testset "is_identifier accepts plain names and rejects the rest" begin
        @test KernelWidgets.is_identifier("freq")
        @test KernelWidgets.is_identifier("_x")
        @test KernelWidgets.is_identifier("x1_var!")
        @test !KernelWidgets.is_identifier("")
        @test !KernelWidgets.is_identifier("1x")
        @test !KernelWidgets.is_identifier("a b")
        @test !KernelWidgets.is_identifier("a.b")
        @test !KernelWidgets.is_identifier("a=1")
        @test !KernelWidgets.is_identifier("x; rm -rf")
    end

    @testset "set_var! assigns, records the writer, and bumps run_seq" begin
        CellRegistry.clear_registry()
        run_cell!(0, "freq = 440")
        @eval module AssignMod end
        declare_slider!(0, AssignMod, "freq", 220, 880, 10)
        before = CellRegistry.CELLS[0].run_seq

        outcome = KernelWidgets.set_var!(AssignMod, "freq", "450", 0)
        @test outcome.ok
        @test AssignMod.freq == 450
        @test AssignMod.freq isa Int
        @test CellRegistry.VARIABLE_SOURCES[:freq] == 0
        @test CellRegistry.CELLS[0].run_seq > before
        @test :freq in CellRegistry.CELLS[0].defines
        @test CellRegistry.CELLS[0].widgets[1]["current"] == "450"
    end

    @testset "set_var! parses a choice value as a string" begin
        CellRegistry.clear_registry()
        run_cell!(0, "wave = \"sin\"")
        @eval module ChoiceAssignMod end
        CellRegistry.set_current_cell!(0)
        KernelWidgets.record_choice!(ChoiceAssignMod, "wave", ["sin", "cos", "tan"])
        outcome = KernelWidgets.set_var!(ChoiceAssignMod, "wave", "cos", 0)
        @test outcome.ok
        @test ChoiceAssignMod.wave == "cos"
        @test ChoiceAssignMod.wave isa AbstractString
    end

    @testset "set_var! rejects a non-identifier name and leaves the module untouched" begin
        CellRegistry.clear_registry()
        run_cell!(0, "freq = 440")
        @eval module RejectMod end
        declare_slider!(0, RejectMod, "freq", 220, 880, 10)
        outcome = KernelWidgets.set_var!(RejectMod, "bad name", "1", 0)
        @test !outcome.ok
        @test occursin("invalid variable name", outcome.reason)
        @test !isdefined(RejectMod, Symbol("bad name"))
    end

    @testset "classify reflects the new writer, staling a downstream reader" begin
        CellRegistry.clear_registry()
        run_cell!(0, "freq = 440")
        @eval module ClassifyMod end
        declare_slider!(0, ClassifyMod, "freq", 220, 880, 10)
        run_cell!(1, "y = freq * 2")
        @test state_of(1) == "fresh"

        KernelWidgets.set_var!(ClassifyMod, "freq", "450", 0)
        @test state_of(1) == "stale-input"
        @test CellRegistry.classify_all()["1"]["inputs"][1]["rel"] == "stale"
    end

    @testset "a towidget method projects a struct onto the current cell" begin
        CellRegistry.clear_registry()
        CellRegistry.CELLS[5] = CellRegistry.Cell(5)
        CellRegistry.set_current_cell!(5)
        @eval module ProjectMod
            struct Dial
                lo::Int
                hi::Int
                value::Int
            end
            nothelix_towidget(::Any) = nothing
            nothelix_towidget(d::Dial) =
                (kind = "slider", name = "gain", lo = d.lo, hi = d.hi, step = 1, current = d.value)
        end
        warning = KernelWidgets.project_result!(ProjectMod, ProjectMod.Dial(0, 10, 5))
        @test warning === nothing
        specs = CellRegistry.CELLS[5].widgets
        @test length(specs) == 1
        @test specs[1]["kind"] == "slider"
        @test specs[1]["name"] == "gain"
        @test specs[1]["params"] == "0:10:1"
        @test specs[1]["current"] == "5"
    end

    @testset "the base towidget fallback projects nothing" begin
        CellRegistry.clear_registry()
        CellRegistry.CELLS[5] = CellRegistry.Cell(5)
        CellRegistry.set_current_cell!(5)
        @eval module FallbackMod
            nothelix_towidget(::Any) = nothing
        end
        warning = KernelWidgets.project_result!(FallbackMod, 42)
        @test warning === nothing
        @test isempty(CellRegistry.CELLS[5].widgets)
    end

    @testset "an unknown-kind projection warns, registers nothing, and the result survives" begin
        CellRegistry.clear_registry()
        CellRegistry.CELLS[5] = CellRegistry.Cell(5)
        CellRegistry.set_current_cell!(5)
        @eval module UnknownKindMod
            struct Blob end
            nothelix_towidget(::Any) = nothing
            nothelix_towidget(::Blob) = (kind = "mystery", name = "x")
        end
        survivor = UnknownKindMod.Blob()
        warning = KernelWidgets.project_result!(UnknownKindMod, survivor)
        @test warning !== nothing
        @test occursin("unknown kind", warning)
        @test isempty(CellRegistry.CELLS[5].widgets)
        @test survivor isa UnknownKindMod.Blob
    end

    @testset "a bad-name projection warns and registers nothing" begin
        CellRegistry.clear_registry()
        CellRegistry.CELLS[5] = CellRegistry.Cell(5)
        CellRegistry.set_current_cell!(5)
        @eval module BadNameMod
            struct Blob end
            nothelix_towidget(::Any) = nothing
            nothelix_towidget(::Blob) = (kind = "slider", name = "bad name", lo = 0, hi = 1)
        end
        warning = KernelWidgets.project_result!(BadNameMod, BadNameMod.Blob())
        @test warning !== nothing
        @test occursin("invalid name", warning)
        @test isempty(CellRegistry.CELLS[5].widgets)
    end

    @testset "nothelix_widget registers a validated choice spec directly" begin
        CellRegistry.clear_registry()
        CellRegistry.CELLS[6] = CellRegistry.Cell(6)
        CellRegistry.set_current_cell!(6)
        KernelWidgets.register_spec!(Dict(
            "kind" => "choice", "name" => "wave",
            "options" => ["sin", "cos"], "current" => "cos"))
        specs = CellRegistry.CELLS[6].widgets
        @test length(specs) == 1
        @test specs[1]["kind"] == "choice"
        @test specs[1]["params"] == "sin|cos"
        @test specs[1]["current"] == "cos"
    end

    @testset "nothelix_widget ignores a malformed spec and registers nothing" begin
        CellRegistry.clear_registry()
        CellRegistry.CELLS[6] = CellRegistry.Cell(6)
        CellRegistry.set_current_cell!(6)
        redirect_stderr(devnull) do
            KernelWidgets.register_spec!(Dict("kind" => "slider", "name" => "g"))
        end
        @test isempty(CellRegistry.CELLS[6].widgets)
    end
end
