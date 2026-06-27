# Nothelix Demo - Inline Image Rendering in Helix
# Source: nothelix_demo.jl

# No-op macro for cell markers (allows standalone execution)
macro cell(idx, exec_count) end
macro markdown(idx) end

# ═══════════════════════════════════════════════════════════════════
@markdown 1
#=
# Welcome to Nothelix

Jupyter-style notebooks inside Helix editor with **inline image rendering**.

Features:
- Execute Julia code cells with `space n x`
- Execute all cells with `space n a`  
- Navigate cells with `space n n` (next) and `space n p` (previous)
- Inline plot rendering using Kitty graphics protocol
=#

# ═══════════════════════════════════════════════════════════════════
@cell 1 nothing
using Plots

# ═══════════════════════════════════════════════════════════════════
@markdown 2
#=
## Simple Line Plot

Let's start with a basic sine wave.
=#

# ═══════════════════════════════════════════════════════════════════
@cell 2 nothing
x = 0:0.1:2π
y = sin.(x)
plot(x, y, 
    title="Sine Wave", 
    xlabel="x", 
    ylabel="sin(x)",
    linewidth=2,
    color=:blue,
    legend=false)

# ═══════════════════════════════════════════════════════════════════
@markdown 3
#=
## Multiple Series

Comparing sine and cosine functions.
=#

# ═══════════════════════════════════════════════════════════════════
@cell 3 nothing
x = 0:0.1:2π
plot(x, sin.(x), label="sin(x)", linewidth=2)
plot!(x, cos.(x), label="cos(x)", linewidth=2)
plot!(title="Trigonometric Functions", xlabel="x", ylabel="y")

# ═══════════════════════════════════════════════════════════════════
@markdown 4
#=
## Scatter Plot

Random data visualisation.
=#

# ═══════════════════════════════════════════════════════════════════
@cell 4 nothing
n = 50
x = randn(n)
y = randn(n)
colours = rand(n)
scatter(x, y, 
    zcolor=colours,
    title="Random Scatter",
    xlabel="x",
    ylabel="y",
    markersize=8,
    legend=false,
    colorbar=true)

# ═══════════════════════════════════════════════════════════════════
@markdown 5
#=
## Histogram

Distribution of random samples.
=#

# ═══════════════════════════════════════════════════════════════════
@cell 5 nothing
data = randn(1000)
histogram(data, 
    bins=30,
    title="Normal Distribution",
    xlabel="Value",
    ylabel="Frequency",
    fillalpha=0.7,
    color=:steelblue,
    legend=false)

# ═══════════════════════════════════════════════════════════════════
@markdown 6
#=
## Subplots

Multiple plots in a grid layout.
=#

# ═══════════════════════════════════════════════════════════════════
@cell 6 nothing
x = 0:0.1:4π

p1 = plot(x, sin.(x), title="sin(x)", legend=false)
p2 = plot(x, cos.(x), title="cos(x)", legend=false, color=:red)
p3 = plot(x, tan.(x), title="tan(x)", legend=false, color=:green, ylims=(-5, 5))
p4 = plot(x, sin.(x) .* cos.(x), title="sin(x)·cos(x)", legend=false, color=:purple)

plot(p1, p2, p3, p4, layout=(2, 2), size=(600, 400))

# ═══════════════════════════════════════════════════════════════════
@markdown 7
#=
## 3D Surface Plot

Visualising a 2D function.
=#

# ═══════════════════════════════════════════════════════════════════
@cell 7 nothing
x = -2:0.1:2
y = -2:0.1:2
f(x, y) = sin(sqrt(x^2 + y^2))
surface(x, y, f, 
    title="Ripple Surface",
    xlabel="x",
    ylabel="y",
    zlabel="z",
    colorbar=true)

# ═══════════════════════════════════════════════════════════════════
@markdown 8
#=
## Heatmap

Matrix visualisation.
=#

# ═══════════════════════════════════════════════════════════════════
@cell 8 nothing
data = [sin(x) * cos(y) for x in 0:0.2:2π, y in 0:0.2:2π]
heatmap(data,
    title="sin(x)·cos(y) Heatmap",
    color=:viridis,
    aspect_ratio=:equal)

# ═══════════════════════════════════════════════════════════════════
@markdown 9
#=
## Conclusion

This demo shows Nothelix rendering inline plots directly in Helix.

- Plots scroll with the document
- Multiple images are handled efficiently  
- No external viewer needed

Try scrolling up and down to see the images move with the text!
=#
