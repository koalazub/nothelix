# Bugs to Fix Later

## Helix keymap macro bug

**Location**: `helix-term/src/commands/engine/steel/keymaps.scm:216-218`

**Issue**: Pattern variable mismatch - pattern uses `map` but body uses `kmap`

```scheme
[(_ (extension name (inherit-from map)) args ...)
 (helix.keymaps.#%add-extension-or-labeled-keymap name
                                                  (merge-keybindings kmap (keymap args ...)))]
```

**Fix**: Change `map` to `kmap` in the pattern on line 216

**Workaround**: Call `#%add-extension-or-labeled-keymap` and `merge-keybindings` directly instead of using the `(inherit-from ...)` macro syntax.
