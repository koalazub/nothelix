# ═══ Nothelix Notebook: /Users/koalazub/projects/nothelix/libnothelix/tests/fixtures/simple.ipynb ═══
# Cells: 4

@cell 0 julia
using Plots

@cell 1 julia
x = 1:10
y = x.^2

@markdown 2
# # Results
# 
# This shows the quadratic function.

@cell 3 julia
plot(x, y)

