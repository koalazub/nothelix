---
title: Troubleshooting
nav_order: 8
---

# Troubleshooting

Start with the doctor. It checks Julia, the graphics protocol, and the plugin
install, then tells you exactly what is wrong.

```bash
nothelix doctor          # environment checks
nothelix doctor --smoke  # also spawns a real Julia kernel end to end
```

If the doctor passes but something still misbehaves, turn on debug logging and
reproduce the problem.

## Debug logging

Logging is off by default. The routine is simple. Turn it on, reproduce the
failure, read the log, then turn it off again.

```
:nothelix-debug-on       # start logging (status line shows the state)
:nothelix-debug-off      # stop
```

Launch Helix with `-vv` so info-level lines are not swallowed.

```bash
hx -vv path/to/notebook.jl
grep 'nothelix:' ~/.cache/helix/helix.log
```

The log lives at `~/.cache/helix/helix.log`, and every plugin line is prefixed
`nothelix:`. When a plot fails, look for a line reporting a registered inline
image. If that line is present, the break is downstream in the renderer or the
terminal. If it is absent, scan upward for an stdout or cache error instead.

### Image repro harness

```bash
nu scripts/capture-image-log.nu path/to/notebook.jl
```

This truncates the log, starts `hx -vv`, and prints an image-lifecycle summary
when you quit. Enable debug mode inside the session, reproduce the failure, then
quit. The tail tells you whether the problem is the plugin, the fork, or the
terminal.

## Common breakages

| Symptom | Cause | Fix |
|---|---|---|
| No plots, math, or tables, though Unicode math still works | tmux or Zellij strips Kitty graphics sequences | Run Helix directly in a Kitty-protocol terminal, then run `:graphics-check` to confirm |
| Nothing renders and there is no multiplexer | Terminal has no supported image protocol | Use a Kitty-protocol terminal. Sixel is not implemented |
| Images fall back to placeholders in a good terminal | Running stock Helix, not the fork | Install the [Helix fork](https://github.com/koalazub/helix/tree/feature/inline-image-rendering) and see [Architecture](architecture.md#why-a-fork) |
| Doctor reports Julia missing | Julia is not on `PATH` | Install Julia with juliaup, then reopen your shell |
| Doctor warns about tmux or Zellij | Running inside a multiplexer | Detach and run Helix directly, or accept text placeholders |
| Plugin or library edits do nothing | Helix caches loaded modules and the dylib | Quit Helix fully and reopen. Run `just install` first after a Rust change |
| Stale code loads after reinstall | `~/.config/helix/nothelix*` shadows `~/.steel/cogs` | Remove the old copy. `just install` warns when it finds one |
| Kernel reports "not ready" after a Julia upgrade | Fresh default env is missing kernel deps | Re-run `just setup-lsp`, then restart |
| macOS editor dies on first feature use | Rebuild invalidated the dylib code signature | Run `just install`, which copies then re-signs |

## Current limitations

- Julia is the only supported kernel. Python is planned.
- Inline rendering requires the [Helix fork](architecture.md#why-a-fork).
- tmux and Zellij strip the Kitty graphics sequences.
- Sixel is not implemented, so unsupported terminals show text placeholders.
- Prebuilt binaries ship for macOS arm64 and Linux x86_64 only.

For the full list of commands and keys behind these fixes, see
[Commands and keys](commands.md).
