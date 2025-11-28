# Nothelix Installation Guide

## Quick Install

### Step 1: Copy plugin files

```bash
# Create plugins directory if it doesn't exist
mkdir -p ~/.config/helix/plugins

# Copy main plugin file and module directory
cp nothelix.scm ~/.config/helix/plugins/
cp -r nothelix ~/.config/helix/plugins/
```

**OR** use symlinks for development:

```bash
# Symlink for automatic updates
ln -sf "$(pwd)/nothelix.scm" ~/.config/helix/plugins/nothelix.scm
ln -sf "$(pwd)/nothelix" ~/.config/helix/plugins/nothelix
```

### Step 2: Load the plugin

Add to your `~/.config/helix/init.scm` (create if it doesn't exist):

```scheme
;; Load Nothelix plugin
(require "plugins/nothelix.scm")
```

### Step 3: Restart Helix

```bash
# Close and reopen Helix, or use:
:reload-config
```

## Verify Installation

Open a `.ipynb` file and try:
- `]l` - Jump to next cell
- `[l` - Jump to previous cell
- `gnr` - Execute cell (requires Julia installed)
- `<space>nj` - Open cell picker

## File Structure After Installation

```
~/.config/helix/
├── init.scm                      # Your main init file (loads plugins)
├── config.toml                   # Your Helix config (untouched)
└── plugins/
    ├── libnothelix.dylib         # Rust FFI library
    ├── nothelix.scm              # Main plugin entry point
    └── nothelix/                 # Module directory
        ├── string-utils.scm      # String utilities
        ├── cursor.scm            # Cursor helpers
        ├── graphics.scm          # Graphics protocol
        ├── kernel.scm            # Kernel management
        ├── conversion.scm        # Notebook conversion
        ├── navigation.scm        # Cell navigation
        ├── execution.scm         # Cell execution
        ├── selection.scm         # Text objects
        └── picker.scm            # Cell picker
```

## Uninstallation

### Step 1: Remove from init.scm

Remove or comment out this line in `~/.config/helix/init.scm`:

```scheme
;; (require "plugins/nothelix.scm")
```

### Step 2: Delete plugin files

```bash
rm ~/.config/helix/plugins/nothelix.scm
rm -r ~/.config/helix/plugins/nothelix/
rm ~/.config/helix/plugins/libnothelix.dylib  # Optional: remove FFI library
```

### Step 3: Restart Helix

```bash
:reload-config
```

## Troubleshooting

### "Cannot find module" error

Make sure both the main file and module directory are in place:
```bash
ls ~/.config/helix/plugins/
# Should show: nothelix.scm nothelix/ libnothelix.dylib

ls ~/.config/helix/plugins/nothelix/
# Should show: string-utils.scm cursor.scm graphics.scm kernel.scm conversion.scm navigation.scm execution.scm selection.scm picker.scm
```

### Keybindings not working

1. Check you're in a `.ipynb` file
2. Keybindings are extension-specific and only work in notebook files
3. Try `:execute-cell` directly to test if plugin loaded

## What Gets Loaded

When you `(require "plugins/nothelix.scm")`:

✅ Commands registered: `:execute-cell`, `:next-cell`, `:previous-cell`, `:cell-picker`, `:convert-notebook`, `:sync-to-ipynb`
✅ Keybindings registered for `.ipynb` files only
✅ Cell picker component available
✅ All 9 modules loaded automatically

Your existing Helix config remains **completely untouched**.
