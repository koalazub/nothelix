# Documentation Suite Rewrite — Design

Date: 2026-06-28

## Goal

Replace the single stale `README.md` with a well-presented documentation suite,
written as clear prose for researchers and scientists who want to stay in Helix
and focus on their work. No emoji, no manufactured metrics. Attribute the
open-source tooling nothelix builds on, and explain why each piece is used.

## Audience and tone

Primary reader: a researcher or scientist who already lives in a terminal and a
modal editor, and wants notebook-style computation without a browser. User-facing
pages answer three questions for each feature: what it does, how to use it, why it
helps the work. Engineering internals are confined to a single architecture page
so the main path stays light.

## Hosting

GitHub Pages, `just-the-docs` remote theme (sidebar navigation + built-in search,
built by GitHub, no local build step).

- Pages source: branch `main`, folder `/docs`.
- User-facing pages live at `docs/` root.
- `docs/dev/` holds contributor/internal docs and is excluded from the Jekyll
  build via `_config.yml`.
- `README.md` shrinks to a landing page: one-paragraph pitch, install one-liner,
  requirements, and a link table into the site at
  `https://koalazub.github.io/nothelix`.

## Page suite (`docs/`)

| File | Purpose |
|---|---|
| `index.md` | Why nothelix. The thesis: stay in Helix, no browser, focus on the work. |
| `install.md` | Curl install, requirements, doctor, upgrade/uninstall, build-from-source. |
| `notebooks.md` | Cell format, convert/sync `.ipynb`, scaffolding/autofill, execution, renumber. |
| `rendering.md` | Inline Unicode math, display-math Typst images, plots, tables, graphics matrix, multiplexer caveat. |
| `export.md` | Markdown / Typst / PDF export and the LaTeX→Typst path. |
| `lsp.md` | JETLS as optional, bring-your-own Julia code intelligence. |
| `commands.md` | Authoritative command + keybinding + autofill reference. |
| `architecture.md` | Four-layer design, kernel file-IPC, error enrichment, fork APIs/FFI, math strategy. |
| `credits.md` | Open-source attribution with a reason per tool. |
| `troubleshooting.md` | doctor, debug mode, log filtering, image repro harness, common breakages. |

## Attribution (`credits.md`)

Each entry carries the reason it is used: Helix and the inline-image fork, Steel,
Julia and juliaup, JETLS, Typst (typst-svg/pdf/layout/kit), MiTeX, resvg +
tiny-skia, the `image` crate, rayon, serde, the Kitty graphics protocol,
tree-sitter, Runic. Prior art that shaped the math approach (euporie, SymPy
pretty-print, termtex) is acknowledged. NothelixMacros is noted as first-party.

## Housekeeping

Move into `docs/dev/` (excluded from Pages):

- `docs/superpowers/` (plans + specs)
- `docs/upstreaming-roadmap-2026-06.md`
- `docs/fork-notes/`
- `docs/math-rendering-strategy.md`

`architecture.md` links to the surviving dev docs rather than duplicating them.

## Corrections to stale facts

- Remove the `lsp/julia-lsp` / LanguageServer.jl / bootstrap-environment story.
  The LSP is JETLS, bring-your-own; `lsp.rs` and `lsp/` are gone.
- Add export (Markdown, Typst, PDF) — absent from the old README.
- Refresh the command and keybinding tables against the current plugin.
- Attribute MiTeX (now wired into both export and display-math rendering).
- Keep Sixel and Python kernels as honest "not yet" limitations.

## Non-goals

- No installer changes. JETLS stays bring-your-own; docs are honest about that.
- No code/behaviour changes. Documentation and file moves only.
