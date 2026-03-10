# ═══ Nothelix Notebook: /Users/koalazub/projects/nothelix/libnothelix/tests/fixtures/simple.ipynb ═══
# Cells: 4

@cell 0 julia
using Plots


# ─── Output ───
# ─────────────

@cell 1 julia
x = 1:10
y = x.^2


# ─── Output ───
10-element Vector{Int64}:
   1
   4
   9
  16
  25
  36
  49
  64
  81
 100
# ─────────────

@markdown 2
# # Results
# 
# This shows the quadratic function.

@cell 3 julia
plot(x, y)


# ─── Output ───
# ─────────────

