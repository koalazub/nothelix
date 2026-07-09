# Notebook resume position — design

## Problem

When you reopen a notebook, the cursor lands at the top of the file. If you
were working in cell 42, you have to scroll and hunt for it. Notebooks are
long; this is friction every session.

The cursor *anchor* model already exists — `cursor-restore.scm` records a
position as `(cell-marker-ordinal, line-offset-within-cell, column)` so it
survives output being inserted below cells. But that state lives in memory and
dies with the editor process. The gap is **persistence across sessions**.

## Non-goals

- No new dotfiles in the user's project. The project tree already carries
  `.nothelix.conf`; we add nothing beside it.
- Not shared/committed state. A cursor position is personal and volatile —
  teammates should not inherit it, and it should not churn a tracked file.
- Not raw line/column persistence. Anchoring to the cell (not an absolute line)
  is what makes "land in cell 42" survive edits and inserted output.
- WASM packaging of notebooks is explicitly out of scope — a later, separate
  spec gated on Julia's capabilities.

## Storage

Reuse the global state directory that already exists for the trust allowlist:
`~/.local/share/nothelix/`. Resume state is a sibling file there:

```
~/.local/share/nothelix/
  trusted-dirs      # existing
  resume            # new — one line per notebook
```

Line format, mirroring `trusted-dirs` (line-based, cannot wedge a parser, no
JSON dependency — the same robustness argument `project-config.scm` makes):

```
<canonical-abs-path>\t<cell-ordinal>\t<line-offset>\t<column>
```

- One notebook per line; a repeated save updates that notebook's line in place.
- Paths are canonicalized so `./foo.jl`, `foo.jl`, and the absolute form key the
  same entry (same reason `trust.rs` canonicalizes).
- Unparseable lines are skipped, never fatal.

This means **zero files appear anywhere the user works**, and no *new* global
location is introduced — only a second file in a directory nothelix already owns.

## What is stored

The exact triple `cursor-restore.scm` already computes:

- `cell-ordinal` — the 1-based index of the `@cell` marker enclosing the cursor
  (0 if above the first marker).
- `line-offset` — lines below that marker.
- `column` — column within the line.

Restoring this triple lands you on the precise spot inside cell 42, not merely
the cell's first line, and it tolerates output inserted below other cells.

## Rust module: `resume.rs`

A small module mirroring `trust.rs` (same file-store shape, same test style).
Chosen over a pure-Steel implementation because `write-string-to-file!` uses
`std::fs::write`, which does not create parent directories — so a first-ever
write to `~/.local/share/nothelix/resume` before that dir exists would fail.
`trust.rs`'s `write_at` already handles this with `create_dir_all`, and reusing
its shape also gives us canonical path keys for free.

FFI surface (registered in `lib.rs`, bumps `NOTHELIX_FFI_VERSION` 18 → 19):

- `resume-get(path: String) -> String` — the stored `"ordinal\toffset\tcol"`
  for `path`, or `""` if none.
- `resume-set(path, ordinal, offset, col) -> String` — upsert the line; `""` on
  success, `"ERROR: …"` otherwise.

Internals reuse the `trust.rs` idioms: `home_dir()`, a `resume_path()` returning
`~/.local/share/nothelix/resume`, `list_at`/`write_at` with `create_dir_all`,
and a canonicalizing key. Path canonicalization failures (a since-deleted
notebook) skip the entry rather than erroring.

## Steel wiring

A new `resume.scm` module, thin over the FFI, providing:

- `save-resume-position!` — read the focused notebook's path + current anchor
  (reusing `cursor-restore.scm`'s ordinal/offset/col computation), call
  `resume-set`.
- `restore-resume-position!` — on open, call `resume-get`; if a line exists,
  resolve the ordinal to its current marker line (reusing
  `restore-cursor-for!`'s clamping) and move the cursor there.

### Capture points

- **On save** — inside the existing `post-command` hook path that already runs
  `renumber-cells!` after a buffer-writing command. Capture *after* renumber so
  the stored ordinal matches the file as saved.
- **On cell-run** — in `:execute-cell` (`<space>nr`), so a run-heavy session
  with no explicit `:w` still records where you are.

Both are low-frequency events — no per-keystroke writes.

### Restore point

- **On open** — in the existing `document-opened` hook, after
  `maybe-apply-project-config!` and after the conceal-on-open mutation settles.
  `cursor-restore.scm` was built to restore across exactly this kind of async
  buffer mutation, so restore is sequenced to run once the buffer is stable.

## Error handling

- Missing `resume` file, missing entry, unparseable line → no-op, cursor stays
  at the top. Resume is best-effort; it never blocks opening a notebook.
- A stored ordinal that no longer resolves (file shrank, cells removed) → clamp
  to the last marker / valid line via the existing `restore-cursor-for!` clamp,
  landing you nearby instead of erroring.
- `resume-set` write failures surface as `"ERROR: …"` to the caller and are
  logged to status, never fatal to the save.

## Testing

- `resume.rs` unit tests mirroring `trust.rs`: set/get roundtrip, upsert (a
  second set for the same path updates, not duplicates), canonical-path keying,
  missing-file returns empty, malformed line skipped. Store path injected via a
  test seam like `trust.rs`'s `*_at` helpers so tests hit a tempdir, not `$HOME`.
- Steel behaviour covered by the existing headless plugin-load gate (module
  loads, symbols resolve). Anchor math is already exercised by
  `cursor-restore.scm`'s usage; resume reuses it rather than reimplementing.

## Rollout

- Bump `NOTHELIX_FFI_VERSION` 18 → 19; `just install` after the Rust build.
- No project-file migration; the `resume` file is created lazily on first save.
