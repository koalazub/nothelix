---
title: Troubleshooting
nav_order: 8
---

# Troubleshooting

```bash
nothelix doctor          # environment checks
nothelix doctor --smoke  # also spawns a real Julia kernel end to end
```

Start here. The doctor checks Julia, the graphics protocol, and the plugin
install, then tells you what is wrong. If it passes, turn on debug logging.

## Debug logging

Off by default. Turn it on, reproduce, read the log, turn it off.

```
:nothelix-debug-on       # start logging (status line shows the state)
:nothelix-debug-off      # stop
```

Launch Helix with `-vv` so info-level lines are not swallowed:

```bash
hx -vv path/to/notebook.jl
grep 'nothelix:' ~/.cache/helix/helix.log
```

The log lives at `~/.cache/helix/helix.log`. Every plugin line is prefixed
`nothelix:`. When a plot fails, look for a line reporting a registered inline
image: if present, the break is downstream (renderer or terminal); if absent,
scan upward for an stdout or cache error.

### Image repro harness

```bash
nu scripts/capture-image-log.nu path/to/notebook.jl
```

Truncates the log, starts `hx -vv`, and prints an image-lifecycle summary on
quit. Enable debug mode inside, reproduce, quit. The tail shows whether the
problem is the plugin, the fork, or the terminal.

## Common breakages

| Symptom | Cause | Fix |
|---|---|---|
| No plots, math, or tables; Unicode math still works | tmux or Zellij strips Kitty graphics sequences | Run Helix directly in a Kitty-protocol terminal; `:graphics-check` to confirm |
| Nothing renders and there is no multiplexer | Terminal has no supported image protocol | Use a Kitty-protocol terminal; Sixel is not implemented |
| Images fall back to placeholders in a good terminal | Running stock Helix, not the fork | Install the [Helix fork](https://github.com/koalazub/helix/tree/feature/inline-image-rendering); see [Architecture](architecture.md#why-a-fork) |
| Doctor reports Julia missing | Julia is not on `PATH` | Install Julia (juliaup recommended) and reopen your shell |
| Doctor warns about tmux/Zellij | Running inside a multiplexer | Detach and run Helix directly, or accept text placeholders |
| Plugin or library edits do nothing | Helix caches loaded modules and the dylib | Quit Helix fully and reopen; run `just install` first after a Rust change |
| Stale code loads after reinstall | `~/.config/helix/nothelix*` shadows `~/.steel/cogs` | Remove the old copy; `just install` warns when it finds one |
| Kernel reports "not ready" after a Julia upgrade | Fresh default env is missing kernel deps | Re-run `just setup-lsp`, then restart |
| macOS: editor dies on first feature use | Rebuild invalidated the dylib code signature | Run `just install`, which copies then re-signs |

## Current limitations

- Julia is the only supported kernel. Python is planned.
- Inline rendering requires the [Helix fork](architecture.md#why-a-fork).
- tmux and Zellij strip the Kitty graphics sequences.
- Sixel is not implemented; unsupported terminals show text placeholders.
- Prebuilt binaries ship for macOS arm64 and Linux x86_64 only.
