---
title: Language server
nav_order: 6
---

# Julia code intelligence

Nothelix does not build a language server, and it does not bundle one. What it
gives you is the interface: it runs inside Helix, and Helix already speaks the
Language Server Protocol. So code intelligence — hover, completion,
go-to-definition, diagnostics — is something you connect, with whichever Julia
language server you prefer, the same way you would for any other language in
Helix.

This is a deliberate boundary. Execution and rendering are nothelix's job; static
analysis is the language server's. Keeping them separate means nothelix does not
force a particular server on you, does not litter per-notebook project files next
to your work, and stays fully useful even if you never wire one up. The tradeoff
is the small amount of setup below — an honest one.

Notebook execution, rendering, and export all work without any of this.

## Two options

There are two mature Julia language servers, and Helix can drive either.

- **[JETLS](https://github.com/aviatesk/JETLS)** is the newer one, built on the
  JET analysis framework. It is the direction the ecosystem is moving toward and
  what we would reach for first.
- **[LanguageServer.jl](https://github.com/julia-vscode/LanguageServer.jl)** is the
  long-established server behind the Julia VS Code extension. It is well understood
  and widely deployed.

Both expose the same protocol, so the choice is yours. Follow each project's own
documentation for the exact launch command — they evolve, and hardcoding one here
would only go stale.

## What either one needs from nothelix

A language server analyses your code statically, so it has to make sense of the
`@cell` and `@markdown` markers a notebook is built from, and resolve the same
packages your cells run against. Two things in your default Julia environment
(`@v#.#`) take care of that:

- **NothelixMacros** — the first-party package that defines the `@cell` and
  `@markdown` macros. Without it, every marker is flagged as an unknown macro and
  analysis falls apart on the first line of every notebook.
- **JSON3** — the kernel's runtime dependency. Having it in the same environment
  keeps static analysis consistent with what actually runs.

If you installed from source, one recipe sets both up idempotently:

```bash
just setup-lsp
```

Otherwise do the equivalent by hand — `JSON3` from the registry, and
`NothelixMacros` developed from its path in the nothelix source tree:

```julia
using Pkg
Pkg.add("JSON3")
Pkg.develop(path="/path/to/nothelix/lsp/NothelixMacros")
```

A Julia version bump gives you a fresh, empty default environment, so re-run this
after upgrading Julia.

## Wiring it into Helix

Point Helix's Julia language at your chosen server in `languages.toml`. The shape
is the standard Helix one: a `[language-server.<name>]` entry with the launch
command, and a `language-servers = ["<name>"]` line on the `[[language]]` block for
Julia. The exact command comes from the server's own documentation — see
[JETLS](https://github.com/aviatesk/JETLS) or
[LanguageServer.jl](https://github.com/julia-vscode/LanguageServer.jl).

After installing the packages and editing the config, restart Helix fully so the
language server starts against the updated environment.

## Formatting

If you want format-on-request, install whichever formatter your server delegates
to — JETLS, for instance, shells out to an external
[Runic](https://github.com/fredrikekre/Runic.jl) binary discovered on your PATH.
This too is optional; nothing else depends on it.
