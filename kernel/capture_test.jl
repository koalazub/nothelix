using Test
import Pkg

include(joinpath(@__DIR__, "cell_registry.jl"))
include(joinpath(@__DIR__, "ast_analysis.jl"))
include(joinpath(@__DIR__, "output_capture.jl"))
include(joinpath(@__DIR__, "cell_macros.jl"))

using .CellRegistry
using .ASTAnalysis
using .OutputCapture
using .CellMacros

# Compiled at include time: the kernel's main loop has the same property, so a
# call through this frame reproduces the world-age failure mode where methods
# defined by a later `using` are "too new" for the inlined Base machinery.
function result_repr_via_boot_frame(cell_idx)
    CellMacros.get_cell_result_json(cell_idx)["output_repr"]
end

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

@testset "export scanning finds providers for undefined names" begin
    src = """
    export AbstractToeplitz, Toeplitz, SymmetricToeplitz, Circulant, Hankel
    export durbin, trench, levinson
    # export NotThis
    foo = 1
    \"\"\"
    export ProseOnly
    \"\"\"
    """
    exports = OutputCapture.collect_exports(src)
    @test :Circulant in exports
    @test :durbin in exports
    @test :NotThis ∉ exports
    @test :ProseOnly ∉ exports
    @test :foo ∉ exports

    depot = OutputCapture.installed_exports()
    @test !isempty(depot)
    if haskey(depot, :Circulant)
        @test "ToeplitzMatrices" in depot[:Circulant]
    end
end

@testset "a value whose size methods load after boot still displays" begin
    result = CellMacros.execute_cell(60, "using ToeplitzMatrices\nCirculant([-2, 1, 0, 0, 1])")
    @test result.success
    payload = result_repr_via_boot_frame(60)
    @test payload != "<unable to display>"
    @test occursin("Circulant", payload)
    @test occursin("-2", payload)
end
