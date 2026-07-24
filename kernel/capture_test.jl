using Test

include(joinpath(@__DIR__, "cell_registry.jl"))
include(joinpath(@__DIR__, "ast_analysis.jl"))
include(joinpath(@__DIR__, "output_capture.jl"))

using .CellRegistry
using .ASTAnalysis
using .OutputCapture

@testset "live output capture" begin
    dir = mktempdir()
    OutputCapture.set_kernel_dir(dir)
    live = joinpath(dir, "live.out")

    code = """
    println("STDOUT-FIRST")
    flush(stdout)
    sleep(0.5)
    println(stderr, "STDERR-SECOND")
    flush(stderr)
    """

    task = @async OutputCapture.capture_toplevel(Main, code)

    saw_first_before_done = false
    deadline = time() + 10
    while time() < deadline
        if isfile(live) && occursin("STDOUT-FIRST", read(live, String))
            saw_first_before_done = !istaskdone(task)
            break
        end
        istaskdone(task) && break
        sleep(0.02)
    end

    result = fetch(task)

    @testset "the first chunk streams to live.out before the cell finishes" begin
        @test saw_first_before_done
    end

    @testset "both streams land in live.out in arrival order" begin
        final_live = read(live, String)
        first_at = findfirst("STDOUT-FIRST", final_live)
        second_at = findfirst("STDERR-SECOND", final_live)
        @test first_at !== nothing
        @test second_at !== nothing
        @test first(first_at) < first(second_at)
    end

    @testset "the completed result keeps its shape" begin
        @test result isa OutputCapture.CapturedOutput
        @test result.error === nothing
        @test occursin("STDOUT-FIRST", result.stdout)
        @test occursin("STDERR-SECOND", result.stderr)
    end
end

@testset "an unwritable live.out never breaks execution" begin
    dir = mktempdir()
    OutputCapture.set_kernel_dir(joinpath(dir, "live.out"))
    touch(joinpath(dir, "live.out"))

    result = OutputCapture.capture_toplevel(Main, "println(\"STILL-RUNS\"); 21 + 21")

    @test result.error === nothing
    @test result.return_value == 42
    @test occursin("STILL-RUNS", result.stdout)
end
