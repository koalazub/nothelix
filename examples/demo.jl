# ═══════════════════════════════════════════════════════════════════════════
# nothelix demo — Jupyter-style notebooks inside Helix
# ═══════════════════════════════════════════════════════════════════════════
#
# Each `@cell N :julia` block below is a code cell. Place your cursor
# inside one and hit <space>nr to execute it. Output lands under
# `# ─── Output ───` as commented lines, so the file stays valid Julia
# at rest.
#
# Keys worth knowing:
#   <space>nr           execute the cell under the cursor
#   <space>nj           picker: jump to any cell by index
#   <space>nn           insert a new cell
#   ]l / [l             next / previous cell
#   :execute-all-cells  run the whole notebook top to bottom
#   :sync-to-ipynb      round-trip this file to a real .ipynb for sharing
#   :w                  save (stays as .jl)
#
# If anything here surprises you: run `nothelix doctor` in a shell.

@cell 0 :julia
# Stdlib only — runs instantly. Confirms execution works and shows how
# `display` output is captured as commented lines below the cell.
using LinearAlgebra
using Statistics

A = [1.0 2.0 3.0;
     4.0 5.0 6.0;
     7.0 8.0 10.0]

display(A)
println("det(A) = ", det(A))
println("rank(A) = ", rank(A))
println("‖A‖ = ", norm(A))

@cell 1 :julia
# This cell triggers Plots precompilation on first run (~60s on a cold
# machine, instant after that). When it finishes you should see a
# rendered chart inline, not a `# [Plot: …]` text placeholder. If you
# see the placeholder, your terminal doesn't speak the Kitty graphics
# protocol — run `nothelix doctor` and check the terminal line.
using Plots

x = range(0, 4π; length = 200)
plot(x,  sin.(x), label = "sin", lw = 2, title = "hello from nothelix")
plot!(x, cos.(x), label = "cos", lw = 2)
plot!(x, sin.(x) .* cos.(x), label = "sin·cos", lw = 2, ls = :dash)

@markdown 2
### What's next?

- Open any `.ipynb` with `nothelix path/to/notebook.ipynb` — it
  auto-converts to `.jl` on open and back to `.ipynb` on
  `:sync-to-ipynb`.
- Create new cells anywhere with `<space>nn`. Pick code or markdown
  from the popup.
- Type `@cell` followed by space on an empty line and the autofill
  picker comes up; type `@md` followed by space and it expands straight
  to a markdown cell.
- Run `:new-notebook` to scaffold a fresh `.jl` notebook.

That's the tour. Delete this file when you're done — it's just a demo,
and `nothelix upgrade` restores it.
