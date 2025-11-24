# Julia LSP and Validation Notes

## "Missing reference" Errors for Runtime Imports

### The Issue

When writing Julia code like:

```julia
import Pkg; Pkg.add("Wavelets")
using Wavelets, Random
```

You may see LSP errors:
```
●  import Pkg; Pkg.add("Wavelets")
         │    └─Missing reference: Pkg
         └─Missing reference: Pkg
●  using Wavelets, Random
                  └─Missing reference: Random
```

### Why This Happens

This is **not a Nothelix bug** - it's a limitation of how Julia's LSP (Language Server Protocol) works:

1. The LSP performs **static analysis** before code runs
2. `Pkg` is a standard library module that needs to be imported first
3. The LSP doesn't execute `import Pkg` - it just sees you're using `Pkg` before it's been loaded
4. Same for `Random` and other standard library modules

### This is Normal

These warnings are **cosmetic only** and don't affect execution:

- ✅ The code **will run correctly** when you execute the cell
- ✅ Julia will import `Pkg` and then use it
- ✅ Packages will be added/loaded successfully
- ⚠️ LSP just shows diagnostics based on static analysis

### Workarounds

#### Option 1: Separate Cells (Recommended)

Split imports across cells:

```julia
# Cell 1 - Import standard library modules
import Pkg
using Random

# Cell 2 - Use them
Pkg.add("Wavelets")
```

Run `:execute-cells-above` to execute both in order.

#### Option 2: Ignore the Warnings

The warnings are harmless. Just execute the cell - it will work.

#### Option 3: Preload in Startup

Add to `~/.julia/config/startup.jl`:

```julia
import Pkg
using Random, Statistics, LinearAlgebra
```

Then these modules are always available.

#### Option 4: Use Fully Qualified Names

Instead of:
```julia
import Pkg; Pkg.add("Wavelets")
```

Use:
```julia
Base.Pkg.add("Wavelets")  # Pkg is in Base
```

But this is verbose and not idiomatic.

## Standard Library Modules

These commonly show "Missing reference" warnings:

| Module | Purpose | Workaround |
|--------|---------|------------|
| `Pkg` | Package management | `import Pkg` in separate cell |
| `Random` | Random number generation | `using Random` in separate cell |
| `Statistics` | Basic statistics | `using Statistics` in separate cell |
| `LinearAlgebra` | Linear algebra | `using LinearAlgebra` in separate cell |
| `Test` | Unit testing | `using Test` in separate cell |

## Package Loading Patterns

### Good Pattern (No LSP Warnings)

```julia
# Cell 1: Imports
import Pkg
using Plots, DataFrames

# Cell 2: Package installation (only first time)
# Pkg.add("SpecialPackage")

# Cell 3: Load and use
using SpecialPackage
```

Execute sequentially: `:execute-cells-above`

### Common Pattern (Has LSP Warnings, But Works)

```julia
# Cell 1: Everything at once
import Pkg; Pkg.add("Wavelets")
using Wavelets, Random

# Works fine when executed, LSP just complains
```

## LSP Configuration

You can adjust LSP strictness in Helix config `~/.config/helix/languages.toml`:

```toml
[[language]]
name = "julia"
language-servers = ["julia-lsp"]

[language-server.julia-lsp]
command = "julia"
args = [
  "--startup-file=no",
  "--history-file=no",
  "-e",
  "using LanguageServer; runserver()"
]
```

However, this won't eliminate the "Missing reference" warnings - they're fundamental to static analysis.

## Summary

- **These are LSP diagnostics, not runtime errors**
- **Your code will execute correctly**
- **Use separate cells for imports if warnings bother you**
- **Or just ignore them - they're harmless**

The Julia LSP is conservative and shows warnings for things that might be errors in other contexts, but are perfectly valid in notebook-style execution where cells run sequentially.
