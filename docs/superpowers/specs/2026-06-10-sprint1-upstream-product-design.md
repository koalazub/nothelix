# Sprint 1: Upstream Wave 1 + Product Sprint — Design

Date: 2026-06-10
Status: approved (owner), execution started same day
Companion: `docs/upstreaming-roadmap-2026-06.md` (research report this design executes against)

## Goal

Run two parallel workstreams that (a) start shrinking the helix-fork delta toward stock
`mattwparas:steel-event-system` via small reviewable upstream PRs, and (b) ship the
highest-value product work — conversion correctness, project credibility, release
automation — which has zero fork dependency. This is M1 + M3 of the roadmap, plus the
FFI version handshake pulled forward from M2.

## Decisions log (owner-approved)

| Decision | Choice |
|---|---|
| Sequencing posture | Parallel-track: upstream PRs + product sprint simultaneously |
| Wave-1 PRs | All four tracks: focus/viewport events; robustness fixes; tiny-fix batch; overlay API (hardened, files last). Plus the Steel ByteVector PR. |
| Round-trip guarantee | **Lossless content**: code, markdown, raw cells, attachments, outputs, cell ids survive arbitrary round-trips; JSON formatting may differ. Property-tested fixpoint. |
| FFI mismatch behavior | **Hard-fail loud** at plugin load with actionable message; no degraded half-state |
| Release policy | **Auto-release on green main**: patch auto-bump; minor/major manual |
| linux-arm64 | Deferred, documented |
| Blank-lines contract, Kitty-only, animation fate | Deferred to the inline-images RFC design pass (sprint 2+) |
| M2 codegen adoption + image-FFI consolidation | Sprint 2 |

## Workstream U — upstream (helix fork + steel)

### U1. Fork hygiene (first, ~2 days, −9,269 LOC)
On `feature/inline-image-rendering` in ~/projects/helix:
1. `git rm -r '$HOME'` — 7,553 lines of generated cogs; Helix regenerates them into the
   real `~/.steel` at startup. Zero loss.
2. Move `how_it_works.md`, demo `.jl`/`.ipynb`, design specs, loose `.md` in
   `helix-core/src` → nothelix `docs/fork-notes/`.
3. Revert pure-churn renames (`extensions.rs → steel_reflect.rs`) and Cargo.toml reorder.
4. Drop the ~40-line `insert_string` debug instrumentation.

### U2. PR branches (each cherry-picked fresh off `upstream/steel-event-system`)
Owner reviews every branch + PR text before anything is pushed or filed.

| Branch | Content | Notes |
|---|---|---|
| `pr/focus-viewport-events` | `DocumentFocusGained` + `ViewportChanged` events, Steel hook registration (already in upstream's `register_*` helper idiom), `editor.scm` docs | File first; mirrors merged #111/#112; animation auto-pause depends on it; no plugin-side workaround exists |
| `pr/steel-robustness` | `try_get` sweep, `Tree::get` diagnostic panics, `commit-changes-to-history` | Crash classes exist upstream verbatim |
| `pr/fixes-batch` | GlobSet pattern-loss bug (lead), `grapheme_width("")` guard, `list.rs` doctest, `equality_hint` | Split 2–3 ways if maintainer prefers |
| `pr/overlay-api` | Conceal overlay API hardened: private field + accessors, edit-remap via `update_positions` OR documented re-set contract, `ui.virtual.conceal` theme docs | Files last (~1 week hardening); timeboxed — slips to wave 2 if remap semantics balloon |
| Steel `bytevector-ffi-return` | `From<Vec<u8>>`/`FromFFIArg<Vec<u8>>` for ByteVector — already rebased onto `b67efd5` | PR to mattwparas/steel; stock marshals returned `Vec<u8>` as list-of-ints, breaking `bytes-length` on tick output |

## Workstream P — product (nothelix repo)

### P1. Conversion to lossless-content (`libnothelix/src/notebook/`)
- `strip_suffix` fix for `.replace(".jl", …)` path bug ×3: `convert.rs:240`, `export.rs:49`, `export.rs:91`.
- nbformat 4.5 `id` field for fresh cells.
- `@raw N` cell marker: raw cells round-trip verbatim (today they're coerced to code — lossy).
- Attachment extraction on ipynb→jl: write into `.nothelix/images/`, reference via `@image`;
  makes `embed.rs` symmetric (today attachments silently drop on edit).
- Content-based original-cell matching fallback so outputs survive cell reorder
  (today matching is positional-only).
- Corpus fixpoint property test: `ipynb→jl→ipynb` reaches a fixpoint. Corpus =
  `examples/` + synthesized notebooks covering raw cells, attachments, reorders,
  missing ids, exotic outputs. Public-API only (fuzzy-tests rule).

### P2. Credibility
- Delete dead `compute-and-apply-conceal-async` (`plugin/nothelix/conceal.scm:133`).
- Purge: `uv.lock`, `.devenv.flake.nix`, grammar leftovers, tracked `result` symlink,
  stray `~/` directory in repo root.
- CI workflow (required gate): `cargo nextest run` + `cargo clippy --all-targets -- -D warnings` + fmt check.
- Fix `install.sh --uninstall` by delegating to `dist/uninstall.sh`; explicit not-installed case.

### P3. Release automation
- Auto-release on green main: patch auto-bump, tag, artifacts darwin-arm64 + linux-x86_64,
  GitHub release. First release ships the FFI arity fix pending since April.

### P4. FFI version handshake (pulled forward from M2)
- `NOTHELIX_FFI_VERSION: u32` in `lib.rs`, exposed as `nothelix-ffi-version` FFI fn.
- `nothelix.scm` asserts at load; mismatch → hard fail with
  `"libnothelix FFI vN, plugin expects vM — run just install"`.
- Doctor gains a distinct issue id for mismatch. Bumping the constant joins the
  FFI-change checklist.

## Testing
- Conversion: corpus fixpoint test is the centerpiece; existing 31 notebook tests stay
  green; each new behavior gets public-API tests; all `cargo nextest`.
- PR branches: must build + pass the helix workspace suite standing alone on the branch.
  Focus/viewport PR additionally verified with the runtime register-hook probe against a
  binary built from that branch.
- Handshake: test asserting the exact hard-fail message; doctor test for the new issue id.

## Error handling
- Conversion keeps the `"ERROR: …"` string contract at FFI boundaries.
- Handshake hard-fail is a load-time error with the actionable message — no half-state.
- `install.sh --uninstall` explicit not-installed handling.

## Verification constraint
The agent's sandbox cannot run cargo (`nix develop` polling failure). Builds/tests run in
the owner's environment at checkpoints, or through CI once P2's workflow lands — which is
why CI lands early in workstream P.

## Rollout order
1. Hygiene commits → owner look → push fork branch.
2. CI workflow lands in nothelix first; all subsequent product commits gated.
3. P1/P2/P4 merge to main behind the gate.
4. Auto-release fires (ships April-pending fixes).
5. PR branches: owner review → push → file; focus/viewport first, overlay last.

## Risks
- PR-4 hardening may surface real design problems (edit-remap semantics) — timeboxed,
  slips to wave 2 rather than blocking.
- Auto-release: auto-patch only; minor/major manual.
- Hygiene deletion is large but fully recoverable from git history; moved docs land in
  nothelix in the paired commit.
- #8675 may merge mid-sprint → rebase churn on filed PRs; small PRs keep that cheap,
  which is the point of filing now.
