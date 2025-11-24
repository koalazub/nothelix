# Manual Testing Guide: Inline Image Rendering

## Prerequisites

- ✅ Helix built from `feature/inline-image-rendering` branch
- ✅ Ghostty/Kitty/iTerm2 terminal
- ✅ Julia installed with Plots.jl
- ✅ libnothelix.dylib in `~/.steel/native/`
- ✅ nothelix.scm in `~/.config/helix/plugins/`

## Quick Test: Verify Protocol Detection

1. Open Helix:
   ```bash
   hx
   ```

2. Test protocol detection:
   ```
   :scm (require "plugins/nothelix.scm")
   :scm (graphics-protocol)
   ```

   **Expected**: Should return `"kitty"` (for Ghostty/Kitty) or `"iterm2"`

3. Test capabilities:
   ```
   :scm (protocol-capabilities (graphics-protocol))
   ```

   **Expected**: JSON with supported formats

## Test 1: Simple Plot Generation

Create a test notebook `test_plot.ipynb`:

```julia
using Plots

x = 0:0.1:2π
y = sin.(x)
plot(x, y, label="sin(x)", linewidth=2)
```

### Steps:

1. Open in Helix: `hx test_plot.ipynb`
2. Convert: `:convert-notebook` (creates `test_plot.jl`)
3. Execute cell: `:execute-cell` while cursor in cell
4. Check output section for:
   - Text output (if any)
   - `# [Plot: kitty | XXkB]` marker
   - **Inline image should render below**

### Expected Behavior:

- **With RawContent API**: Image renders inline
- **Without RawContent API**: Shows `# [Plot saved: /tmp/helix-kernel-X/plot_output.png | XXkB]`

## Test 2: Multiple Plots

```julia
# Cell 1
using Plots

# Cell 2
plot(1:10, 1:10, title="Line")

# Cell 3
scatter(rand(10), rand(10), title="Scatter")

# Cell 4
histogram(randn(1000), bins=30, title="Distribution")
```

Execute all cells: `:execute-all-cells`

**Expected**: Each cell shows its own inline plot

## Test 3: Error Handling

```julia
# This should fail gracefully
plot(undefined_variable)
```

**Expected**: Error message in output, no crash

## Test 4: Protocol-Specific Features

### Ghostty/Kitty Format (with escape sequences visible):

The generated escape sequence should look like:
```
\x1b_Gf=100,t=d,a=T,i=1,s=WIDTH,v=HEIGHT;BASE64_DATA\x1b\
```

### iTerm2 Format:
```
\x1b]1337;File=inline=1;width=auto;height=auto:BASE64_DATA\x07
```

## Debugging

If images don't render:

1. **Check protocol detection**:
   ```
   :scm (detect-graphics-protocol)
   ```

2. **Check kernel is running**:
   ```bash
   ls -la /tmp/helix-kernel-1/
   # Should see: pid, input.jl, output.txt, plot.b64
   ```

3. **Check plot file exists**:
   ```bash
   ls -lh /tmp/helix-kernel-1/plot.b64
   # Should be >0 bytes
   ```

4. **Verify base64 encoding**:
   ```bash
   head -c 100 /tmp/helix-kernel-1/plot.b64
   # Should show base64 characters
   ```

5. **Test rendering manually**:
   ```bash
   # For Ghostty/Kitty:
   printf '\x1b_Gf=100,a=T,t=d;'
   cat /tmp/helix-kernel-1/plot.b64
   printf '\x1b\\'
   ```

## Success Criteria

✅ **Protocol detected correctly** (`"kitty"` for Ghostty)
✅ **Escape sequences generated** (49 tests pass)
✅ **Plots saved to kernel dir** (plot.b64 file exists)
✅ **No crashes or errors**
✅ **Images render inline** (requires Helix with RawContent API)

## Current Status

- **Rust library**: ✅ All 49 tests passing
- **Protocol detection**: ✅ Ghostty/Kitty/iTerm2 supported
- **Escape sequence generation**: ✅ Kitty and iTerm2 protocols
- **Format conversion**: ✅ Auto-converts JPEG→PNG for Kitty
- **Integration with Helix**: ⚠️ Requires `add-raw-content!` Steel binding

## Next Steps

To get inline rendering working in Helix:

1. Verify Helix has `add-raw-content!` binding:
   ```
   hx
   :scm add-raw-content!
   ```

   Should not show "FreeIdentifier" error

2. If binding missing, check Helix branch:
   ```bash
   cd ~/projects/helix
   git branch
   # Should be on: feature/inline-image-rendering
   ```

3. Rebuild Helix with Steel support:
   ```bash
   cd ~/projects/helix
   cargo build --release --features steel
   ```

4. Test with simple raw content:
   ```
   :scm (add-raw-content! (string->bytes "\x1b[31mRED\x1b[0m") 1 0)
   ```

   Should see "RED" in red color

## Validation Checklist

- [ ] Protocol detection works (`"kitty"` or `"iterm2"`)
- [ ] Cell execution generates plots
- [ ] plot.b64 file created in kernel dir
- [ ] Escape sequences generated correctly
- [ ] Status messages show plot info
- [ ] Images render inline (with RawContent API)
- [ ] Multiple plots work correctly
- [ ] Error handling is graceful
