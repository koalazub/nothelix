---
title: Language server
nav_order: 6
---

# Julia code intelligence

Code intelligence is optional — bring any Julia LSP; setup is below. Execution, rendering, and export work without it.

```bash
just setup-lsp   # ensures JSON3 (the kernel dep) in your default @v#.# environment
```

Then point Helix's Julia language at your server in `languages.toml`: a `[language-server.<name>]` entry with the launch command, and `language-servers = ["<name>"]` on the Julia `[[language]]` block. Restart Helix fully afterward.

## Servers

| Server | Notes |
|---|---|
| [JETLS](https://github.com/aviatesk/JETLS) | Newer, built on the JET framework. Reach for this first. Delegates format-on-request to a [Runic](https://github.com/fredrikekre/Runic.jl) binary on your PATH. |
| [LanguageServer.jl](https://github.com/julia-vscode/LanguageServer.jl) | Long-established server behind the Julia VS Code extension. |

Both expose the same protocol. Take the exact launch command from the server's own docs.

A Julia version bump gives you a fresh, empty default environment — re-run `just setup-lsp` after upgrading.

See [architecture](architecture.md) for why nothelix ships no language server of its own.
