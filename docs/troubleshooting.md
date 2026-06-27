---
title: Troubleshooting
nav_order: 10
---

# Troubleshooting

When something is not working, start with the doctor, then turn on debug logging
and read what nothelix tells you.

## Start with the doctor

```bash
nothelix doctor
```

This runs a battery of environment checks — Julia on PATH, the graphics protocol,
the plugin install, and more — and tells you what is wrong. Add `--smoke` to go
further and spawn a real Julia kernel, exercising the full execution pipeline end
to end. If the doctor is happy and the smoke test passes, the problem is more
specific, and the debug log is the next place to look.

## Debug logging

Nothelix ships a debug mode that is off by default. When something misbehaves, turn
it on from command mode, reproduce the problem, read the log, and turn it back off.

| Command | Effect |
|---|---|
| `:nothelix-debug-on` | Start emitting debug lines |
| `:nothelix-debug-off` | Stop emitting debug lines |
| `:nothelix-debug-toggle` | Flip the current state |

Each command updates the status line, so you always know which mode you are in.
Remember to turn it off when you are done, or every later cell execution keeps
writing to the log.

Debug lines land in the same file as the rest of Helix's logging.

```
~/.cache/helix/helix.log
```

Helix logs at `warn` by default, which swallows the `info`-level lines nothelix
writes. Launch `hx -v` to let info-level lines through, or `-vv` for everything.

```bash
hx -vv path/to/notebook.jl
```

Every nothelix line is prefixed with `nothelix:`, so you can filter cleanly.

```bash
grep 'nothelix:' ~/.cache/helix/helix.log
```

A few categories are worth watching when a plot does not appear. A line reporting
that a cell registered an inline image means nothelix did its part and the break is
downstream, in Helix's renderer or the terminal. If you never see that line, the
plugin short-circuited earlier, so scan upward for an stdout or cache error. Lines
that bracket the document-opened pass tell you whether the hook fired for the
buffer you expected.

### Image repro harness

For image bugs specifically, there is a scripted harness.

```bash
nu scripts/capture-image-log.nu path/to/notebook.jl
```

It truncates the log, starts `hx -vv`, and prints a summary of the image lifecycle
when you quit. Turn on debug mode inside the session, reproduce the bug, and quit.
The tail usually tells you whether the problem is in the plugin, the Helix fork, or
the terminal.

## Common breakages

**No plots, no typeset math, no rendered tables.** Almost always the terminal or a
multiplexer. Image-based content needs a Kitty-protocol terminal; tmux and Zellij
strip the sequences it relies on. Run `:graphics-check` to see what was detected,
and run Helix directly in a Kitty-protocol terminal. Inline Unicode math still
working while images do not is the signature of this case.

**Images fall back to placeholders even in a good terminal.** You are probably on
stock Helix rather than the fork. Inline rendering depends on the
[Helix fork](https://github.com/koalazub/helix/tree/feature/inline-image-rendering);
see [Architecture](architecture.md#why-a-fork).

**Edits to the plugin or library do not take effect.** Helix caches the loaded
plugin modules and the dynamic library. A config reload is not enough — quit Helix
fully and reopen it. After a Rust change, run `just install` first.

**The plugin loads stale code after reinstalling.** A leftover copy under
`~/.config/helix/nothelix*` shadows the install in `~/.steel/cogs`, because Helix
searches the config directory first. Remove the old copy; `just install` warns when
it finds one.

**The kernel reports "not ready" after a Julia upgrade.** A Julia version bump
gives you a fresh, empty default environment that may be missing the kernel's
dependencies. Re-run `just setup-lsp` (or reinstall the kernel deps), and restart.
The same upgrade is why the [language server](lsp.md) packages need re-installing
after a Julia bump.

**On macOS, the editor dies when it first touches a feature.** The dynamic library
must be re-codesigned after every rebuild; macOS invalidates the signature when the
file changes. `just install` handles the copy-then-sign sequence — use it rather
than copying the library by hand.

## Current limitations

Stated plainly, so there are no surprises.

- Julia is the only supported kernel. Python support is planned but not available.
- Inline rendering requires the Helix fork. Stock Helix falls back to placeholders.
- Terminal multiplexers (tmux, Zellij) strip the Kitty graphics sequences.
- Sixel encoding is not implemented, so terminals without a supported image
  protocol fall back to text placeholders.
- Prebuilt binaries ship for macOS arm64 and Linux x86_64 only.
