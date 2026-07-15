---
title: Language server
nav_order: 6
---

# Julia code intelligence

Code intelligence is optional. Bring any Julia language server you like, or none at all. Execution, inline rendering, and export all work without one. Wire up a server only when you want completions, diagnostics, and go-to-definition inside your notebooks.

The kernel needs one Julia package resolvable in your default environment, and one command installs it.

```bash
nothelix setup-lsp   # adds JSON3, the kernel's runtime dependency, to your default @v#.# environment
```

This does not install a language server. It ensures the kernel can start. From a source checkout the equivalent contributor recipe is `just setup-lsp`, but a reader who installed from the tarball should use `nothelix setup-lsp`.

## Wiring a server into languages.toml

Helix drives the server, so you point Helix's Julia language at the launch command. Add a `[language-server.<name>]` entry with the command, then list that name under `language-servers` on the Julia `[[language]]` block. Restart Helix fully afterward, because it caches language configuration at startup.

Take the exact launch command from your chosen server's own documentation. Both servers below speak the same Language Server Protocol, so Helix treats them the same way.

## Servers

| Server | Notes |
|---|---|
| [JETLS](https://github.com/aviatesk/JETLS) | The active choice for nothelix. Newer, built on the JET framework. Reach for this first. It delegates format-on-request to a [Runic](https://github.com/fredrikekre/Runic.jl) binary on your PATH. |
| [LanguageServer.jl](https://github.com/julia-vscode/LanguageServer.jl) | The long-established server behind the Julia VS Code extension. |

## After a Julia version bump

A Julia upgrade gives you a fresh, empty default environment that no longer has JSON3. Re-run `nothelix setup-lsp` after upgrading so the kernel can start again.

See [architecture](architecture.md) for why nothelix ships no language server of its own. See [installation](install.md) for where `nothelix setup-lsp` fits in first-run setup.
