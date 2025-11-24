# Nothelix Installation Guide

## Quick Install

### Step 1: Copy plugin files

```bash
# Create plugins directory if it doesn't exist
mkdir -p ~/.config/helix/plugins

# Copy all plugin files
cp nothelix.scm ~/.config/helix/plugins/
cp kernel-manager.scm ~/.config/helix/plugins/
cp nothelix-autoconvert.scm ~/.config/helix/plugins/  # Optional
```

### Step 2: Load the plugin

Add to your `~/.config/helix/init.scm` (create if it doesn't exist):

```scheme
;; Load Nothelix plugin
(require "plugins/nothelix.scm")

;; Optional: Enable auto-conversion of raw .ipynb JSON files
(require "plugins/nothelix-autoconvert.scm")
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
    ├── nothelix.scm              # Main notebook plugin
    ├── kernel-manager.scm        # Kernel management
    └── nothelix-autoconvert.scm  # Optional: auto-convert JSON (optional)
```

## Uninstallation

### Step 1: Remove from init.scm

Remove or comment out these lines in `~/.config/helix/init.scm`:

```scheme
;; (require "plugins/nothelix.scm")
;; (require "plugins/nothelix-autoconvert.scm")
```

### Step 2: Delete plugin files

```bash
rm ~/.config/helix/plugins/nothelix.scm
rm ~/.config/helix/plugins/kernel-manager.scm
rm ~/.config/helix/plugins/nothelix-autoconvert.scm
```

### Step 3: Restart Helix

```bash
:reload-config
```

## Troubleshooting

### "Cannot find module" error

Make sure the files are in the right location:
```bash
ls ~/.config/helix/plugins/
# Should show: nothelix.scm kernel-manager.scm
```

### Keybindings not working

1. Check you're in a `.ipynb` file
2. Keybindings are extension-specific and only work in notebook files
3. Try `:execute-cell` directly to test if plugin loaded

### Auto-conversion not working

1. Make sure you required `nothelix-autoconvert.scm` in init.scm
2. Open a raw JSON .ipynb file (not already converted)
3. Check status line for conversion messages

## What Gets Loaded

When you `(require "plugins/nothelix.scm")`:

✅ Commands registered: `:execute-cell`, `:next-cell`, `:previous-cell`, `:cell-picker`
✅ Keybindings registered for `.ipynb` files only
✅ Cell picker component available
✅ Kernel manager loaded automatically

Your existing Helix config remains **completely untouched**.
