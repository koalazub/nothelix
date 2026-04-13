using NothelixMacros

@markdown 0
# # nothelix demo
#
# <space>nr  run cell  |  <space>nn  new cell  |  <space>nj  jump
# ]l / [l    next/prev |  dd on marker line to delete a cell
#
# If something breaks: `nothelix doctor` in a shell.

@cell 1 :julia
using LinearAlgebra, Statistics

A = [1.0 2.0 3.0;
     4.0 5.0 6.0;
     7.0 8.0 10.0]

display(A)
println("det = ", det(A), "  rank = ", rank(A), "  ‖A‖ = ", norm(A))

@cell 2 :julia
# First run ~60s (Plots precompile). Inline chart after.
using Plots

x = range(0, 4π; length=200)
plot(x, sin.(x), label="sin", lw=2, title="hello from nothelix")
plot!(x, cos.(x), label="cos", lw=2)

@markdown 3
### Next steps
#
# - `nothelix path/to/notebook.ipynb` to open an existing notebook
# - `:new-notebook` to scaffold a fresh one
# - `@cell<space>` on empty line → cell-type picker
# - `:sync-to-ipynb` to save back to .ipynb
