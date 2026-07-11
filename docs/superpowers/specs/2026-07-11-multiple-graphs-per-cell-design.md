# Multiple graphs per cell — design (spec 2 of the output-architecture epic)

## Problem

A cell that produces several plots renders only the **first** inline. The
kernel already captures every plot (`kernel/output_capture.jl` pushes each into
`result.images`, a vector), but the plugin's insertion path pulls only
`json-get-first-image` and paints a single image with one id per cell
(`output-insert.scm`; `cell-index->image-id`, `image-cache.scm:42`). So `k`
plots → 1 shown.

## Enabler

Plan 1 made the per-cell output store hold an **ordered outputs array**, and the
kernel JSON already carries `{"images": [...]}` (all of them). So this is
primarily a plugin change: render every image entry, each with a distinct id and
its own reserved region, stacked.

## Design

### libnothelix (Rust)

- `json_get_all_images(json, kernel_dir) -> String` — return every image entry
  (base64 or sidecar-resolved), one per line (or a length + indexed getter),
  instead of only the first. Keep `json_get_first_image` for callers that still
  want one.
- No FFI version bump needed beyond adding the function (additive) — but bump if
  the handshake convention requires it.

### Image-id allocation (`image-cache.scm`)

Today `cell-index->image-id = 1000 + (cell_index mod 3999000)` — one id per
cell. Introduce a per-cell **sub-band**:

```
image-id(cell_index, img_index) = 1000 + ((cell_index * SLOTS + img_index) mod (3999000 - SLOTS))
```

with `SLOTS` = 32 (configurable via `.nothelix.conf` `plots-per-cell`; sane
default). Cache files named `cell-<idx>-<img>.png`; `path->image-id` parses BOTH
indices so ids never collide within a cell. Stays under the 4M plot band and the
2^24 Kitty-id ceiling.

### Insertion (`output-insert.scm`)

Loop over all image entries: for each, `save-image-to-cache!` (indexed name),
insert its `# @image <path>` marker + reserve its height rows, register
`add-raw-content-with-placeholders!` with its distinct id — stacked vertically
below the cell's output anchor. The `plot_data` interactive-overlay slot stays
first-image-only (one overlay per cell, as today).

### Reopen (`image-cache.scm`)

`render-cached-images` already scans `# @image` markers and re-registers each —
with per-image ids from `path->image-id`, multiple markers per cell now
round-trip. Confirm the reopen path sizes each image's height from its own blank
padding.

### Interaction with Plan 1

Images still use real reserve lines (committed via the tagged primitive once
Plan 2 lands; plain commit before that). Text output stays virtual. A cell's
output region is: virtual text rows (Plan 1) + N stacked images (this spec).

### Config

- `.nothelix.conf`: `plots-per-cell = 32` (cap; overridable). `log()` a note if
  a cell exceeds the cap (silent truncation is a defect).

## Non-goals

- Colored braille text-plots — spec 3.
- Image-out-of-undo — Plan 2 (this spec keeps the current reserve-line commit
  mechanism).

## Testing

- libnothelix: `json_get_all_images` returns all entries for a multi-image JSON;
  empty for none; sidecar resolution per entry.
- `image-cache.scm` id allocation: distinct ids for `(cell, img)` pairs within
  and across cells; no collisions under `SLOTS`; stays under 4M / 2^24.
- Manual (live): a cell with 3 `plot(...)` calls renders 3 stacked images;
  reopen restores all 3; re-execute replaces them; the per-cell cap truncates
  with a status note.
