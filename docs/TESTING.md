# Testing Guide for Nothelix Plot Rendering

## Prerequisites

✅ **Helix**: Built from `/Users/alielali/projects/helix` (feature/inline-image-rendering branch)
✅ **Terminal**: Kitty, iTerm2, or Sixel-capable terminal
✅ **Julia**: Installed with Plots.jl package
✅ **Plugin**: Installed to `~/.config/helix/plugins/nothelix.scm`
✅ **Library**: `libnothelix.dylib` in `~/.steel/native/`

## Quick Start Test

### 1. Create Test Notebook

Open an existing `.ipynb` file in Helix and run `:convert-notebook`. This creates a new `.jl` file (preserving the original `.ipynb`) with:

```julia
# nothelix-source: /path/to/test.ipynb

# ─── Code Cell 0 [ ] ───
# Install Plots if needed (first time only)
# using Pkg; Pkg.add("Plots")

# ─── Code Cell 1 [ ] ───
using Plots

# ─── Code Cell 2 [ ] ───
# Simple sine wave
t = 0:0.01:2π
y = sin.(t)
plot(t, y, label="Sine Wave", linewidth=2)
xlabel!("Time")
ylabel!("Amplitude")
title!("Test Plot - Inline Rendering")

# ─── Code Cell 3 [ ] ───
# Multiple series
x = 1:10
y1 = x.^2
y2 = x.^3
plot(x, y1, label="x²", linewidth=2)
plot!(x, y2, label="x³", linewidth=2)
title!("Power Functions")
```

Note: The cell indices (0, 1, 2, 3) and metadata header enable proper cell identification for `execute-cells-above`.

### 2. Work with Converted File

The converted `.jl` file is now open. You can:
- Edit cells like regular Julia code
- Execute cells with `:execute-cell`, `:execute-all-cells`, `:execute-cells-above`
- The original `.ipynb` remains untouched for use with other tools

### 3. Execute Cells in Order

**Option A: Execute all at once**
```
:execute-all-cells
```

**Option B: Execute incrementally**
- Position cursor in Cell 2: `:execute-cell` (loads Plots.jl)
- Position cursor in Cell 3: `:execute-cells-above` (runs Cell 2 + Cell 3)
- Position cursor in Cell 4: `:execute-cells-above` (runs all 3 cells)

### 4. Expected Output for Cell 2

```
# ─── Code Cell 2 [ ] ───
t = 0:0.01:2π
y = sin.(t)
plot(t, y, label="Sine Wave", linewidth=2)
xlabel!("Time")
ylabel!("Amplitude")
title!("Test Plot - Inline Rendering")

# ─── Output ───
# 📊 [Plot: kitty | 45KB]
[ACTUAL SINE WAVE IMAGE RENDERED HERE]
# ─────────────
```

## Troubleshooting

### Issue: "No graphics protocol available"

**Check terminal:**
```bash
echo $TERM
# Should be: xterm-kitty, xterm-256color (iTerm2), or similar
```

**Check detection in Helix:**
```
:scm (graphics-protocol)
# Should return: "kitty" or "iterm2" or "sixel"
# NOT "none"
```

**Fix:** Make sure you're running Helix inside Kitty/iTerm2, not a basic terminal.

### Issue: "Julia not found in PATH"

**Check Julia:**
```bash
which julia
# Should show: /path/to/julia
```

**Fix:** Install Julia or add to PATH:
```bash
export PATH="/Applications/Julia-1.11.app/Contents/Resources/julia/bin:$PATH"
```

### Issue: UndefVarError for variables

**Problem:** Executing cells out of order.

**Fix:** Use `:execute-cells-above` or `:execute-all-cells` to run dependencies first.

### Issue: Plots.jl precompilation errors

**First run:** Plots.jl needs to compile (30-60 seconds). This is normal.

**Status shows:**
```
Precompiling Plots...
✓ Plots
```

**Subsequent runs:** Instant (uses compiled version).

### Issue: Image doesn't render

**Check 1:** Verify plot was generated
```bash
ls -lh /tmp/helix-kernel-1/plot.png
# Should exist and be >0 bytes
```

**Check 2:** Verify base64 file
```bash
ls -lh /tmp/helix-kernel-1/plot.b64
# Should exist and be >0 bytes
```

**Check 3:** Look for errors
```bash
tail -20 /tmp/helix-kernel-1/kernel.log
```

**Check 4:** Test Steel binding directly
```
:scm (add-raw-content! (string->bytes "test") 1 0)
# Should not error
```

## Validation Checklist

After successful execution, verify:

- [ ] Text output appears in `# ─── Output ───` section
- [ ] See `# 📊 [Plot: <protocol> | <size>KB]` line
- [ ] **Image renders inline below the marker**
- [ ] Image persists when scrolling up/down
- [ ] Can see multiple images in same document
- [ ] Re-executing cell updates the image
- [ ] Status bar shows `✓ Cell executed (with plot)`

## Performance Notes

**First cell execution:**
- Julia startup: ~2 seconds
- Plots.jl compilation (first time): ~30-60 seconds
- Plot generation: ~1 second

**Subsequent executions:**
- Plot generation: <1 second (Plots already loaded)
- Multiple cells: ~1 second per cell

## Example Workflows

### Workflow 1: Data Analysis

1. Load data in Cell 1
2. Process in Cell 2-3
3. Visualize in Cell 4
4. Run `:execute-all-cells` to see full pipeline
5. Tweak Cell 4, run `:execute-cell` to update just the plot

### Workflow 2: Interactive Exploration

1. Start with Cell 1: Load libraries
2. Cell 2: Basic plot
3. Run `:execute-cells-above` (loads libs + shows plot)
4. Edit Cell 2 parameters
5. `:execute-cell` to see changes
6. Add Cell 3 with different visualization
7. `:execute-cell` to compare

### Workflow 3: Debugging

1. Run `:execute-all-cells`
2. Cell 5 errors: "UndefVarError"
3. Navigate to Cell 5 with `]`
4. Fix the code
5. Run `:execute-cells-above` to re-run with dependencies
6. Verify output

## Advanced Testing

### Test Different Plot Types

```julia
# Scatter
scatter(rand(10), rand(10), label="Random Points")

# Histogram
histogram(randn(1000), label="Normal Distribution", bins=30)

# Heatmap
heatmap(rand(10, 10), title="Random Heatmap")

# 3D Surface
x = y = -3:0.1:3
f(x,y) = x^2 + y^2
surface(x, y, f, title="Paraboloid")
```

### Test Error Handling

```julia
# Should show error in output
plot(undefined_variable)

# Should recover and work
plot([1,2,3], [1,4,9])
```

### Test Large Plots

```julia
# Generate large dataset
x = 0:0.001:10π
y = sin.(x) .+ 0.1*randn(length(x))
plot(x, y, alpha=0.5, label="Noisy Sine")
```

## Success Criteria

✅ Images render inline in the terminal
✅ Multiple images visible simultaneously
✅ Images update when re-executing cells
✅ No visual artifacts or corruption
✅ Status messages are clear and helpful
✅ Errors are reported with actionable details
✅ Performance is acceptable (<2s per cell after warmup)

## Known Limitations

- **Kitty only**: Currently best support on Kitty terminal
- **PNG only**: Plots.jl saves as PNG (other formats via conversion)
- **Julia only**: Kernel currently Julia-specific
- **Linear execution**: Cells must define dependencies in order
