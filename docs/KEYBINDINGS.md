# Nothelix Keybindings

Add these to your `~/.config/helix/config.toml` under `[keys.normal]`:

## Recommended Keybindings

```toml
[keys.normal]
# Notebook operations
C-n = ":convert-notebook"           # Convert .ipynb to editable format

# Cell execution
C-e = ":execute-cell"               # Execute current cell
C-S-e = ":execute-all-cells"        # Execute all cells (top to bottom)
C-A-e = ":execute-cells-above"      # Execute all cells up to current

# Cell navigation
"]" = ":next-cell"                  # Jump to next cell
"[" = ":previous-cell"              # Jump to previous cell
C-p = ":cell-picker"                # Fuzzy picker for cells

# Text objects (select in visual mode)
"ic" = ":select-cell-code"          # Select cell code only
"ac" = ":select-cell"               # Select entire cell (code + output)
"io" = ":select-output"             # Select output section only
```

## Alternative Keybindings (Space Leader)

If you prefer space-based mnemonics:

```toml
[keys.normal.space.n]  # Space + n = notebook
c = ":convert-notebook"             # Convert
e = ":execute-cell"                 # Execute current
a = ":execute-all-cells"            # Execute all
u = ":execute-cells-above"          # Execute up to here
j = ":next-cell"                    # Jump to next cell (j = down)
k = ":previous-cell"                # Jump to previous cell (k = up)
n = ":next-cell"                    # Next cell (alternative)
p = ":previous-cell"                # Previous cell (alternative)
l = ":cell-picker"                  # List/picker
```

## Commands Available

All commands can be run via `:command-name`:

| Command | Description | Usage |
|---------|-------------|-------|
| `:convert-notebook` | Convert `.ipynb` to editable `.jl` format | Open a `.ipynb` file and run |
| `:sync-to-ipynb` | Sync `.jl` changes back to `.ipynb` | Save edits back to original notebook |
| `:execute-cell` | Execute code in current cell | Cursor anywhere in cell |
| `:execute-all-cells` | Execute all cells sequentially | Anywhere in document |
| `:execute-cells-above` | Execute from top to current cell | Cursor in target cell |
| `:cancel-cell` | Interrupt/cancel running execution | Any time during execution |
| `:next-cell` | Jump to next cell marker | Navigate between cells |
| `:previous-cell` | Jump to previous cell marker | Navigate between cells |
| `:cell-picker` | Fuzzy picker for cells | Quick jump to any cell |
| `:select-cell` | Select entire cell | Use in visual mode |
| `:select-cell-code` | Select only code | Use in visual mode |
| `:select-output` | Select only output | Use in visual mode |
| `:graphics-check` | Check terminal graphics support | See what protocol is detected |
| `:graphics-protocol` | Show current graphics protocol | Returns "kitty", "iterm2", "sixel", or "none" |

## Workflow Example

1. Open notebook: `hx analysis.ipynb`
2. Convert: `C-n` or `:convert-notebook` (creates `analysis.jl`)
3. Navigate: `]` and `[` to move between cells in `.jl` file
4. Execute in order: `C-S-e` to run all cells from top
5. Or execute incrementally: `C-A-e` to run cells up to current
6. Iterate: Edit cell, `C-e` to re-execute just that cell
7. Sync changes back: `:sync-to-ipynb` to update the original `.ipynb`
8. Open in Jupyter/VS Code with your changes preserved

## Status Messages

Watch the status line (bottom of screen) for execution feedback:

- `Executing 3 cells...` - Running multiple cells
- `Executed 2/3 cells` - Progress indicator
- `✓ Cell executed (with plot)` - Success with plot rendered
- `✓ Cell executed` - Success without plot
- `ERROR: ...` - Kernel error with details
