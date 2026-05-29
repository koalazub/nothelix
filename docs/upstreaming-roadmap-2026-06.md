# Steel & Helix Plugin Landscape (June 2026)

**The official plugin system is about to land.** helix-editor/helix#8675 is still open/draft, but archseer's 2026-06-10 comment is the clearest roadmap to date: merge workspace-trust v2 (#15857, actively moving as of the same day) + small bugfixes → cut a Helix release → land #8675 as experimental behind a default-off feature flag. Remaining mechanics: a final review pass from archseer + the-mikedavis, a rebase (branch is `dirty` vs master, last sync 2026-04-16), and un-drafting. Practical consequence for nothelix: **mattwparas/helix will rebase/align right after the next Helix release** — every line of fork divergence gets more expensive at that moment — and the plugin audience is about to grow sharply.

**Steel itself stabilized.** First binary releases v0.8.1/v0.8.2 (Feb 2026) ship prebuilt `steel`, `forge`, `steel-language-server`, `cargo-steel-lib` (macOS arm64, Linux x64/arm64, Windows x64 — Intel mac dropped). The Helix fork pins steel-core git rev `b67efd5` (2026-05-07) with features `["anyhow","dylibs","sync","biased","imbl"]` — **no jit2**. The Feb-2026 ABI loader is asymmetric: a dylib built against steel-core **≤** the host's rev keeps loading; a dylib built against a **newer** rev is rejected. So the standing rule is: build libnothelix against exactly `b67efd5`/0.8.2 (features `dylibs`,`sync`) or older, never newer.

**Correction to internal belief:** ByteVector FFI support is *not* an April-2026 contribution — `From<Vec<u8>> for FFIValue` etc. date to 2023/2024 and exist at any realistic pin. If the team believes it upstreamed that patch, that belief is wrong for mattwparas/steel; worth checking what patch (if any) the build actually depends on.

**Behavior changes to re-test on any Steel bump:** eval now resolves against the calling module's context (#572, Nov 2025); top-level macro semantics were overhauled Feb–Mar 2026 (#631/#633/#634/#646); re-providing macros across module boundaries is broken until #645 is fixed. Also useful: value serialization + program images (#636 — relevant to state persistence and the future kernel-IPC plan), R7RS aliases, built-in sort, parameterize-aware ports.

**What "stock" (steel-event-system tip, 4d86612d, 2026-05-21) gives you today:** 11 event hooks including `terminal-focus-gained/lost`, `document-closed`, `document-saved`; tree-sitter bindings from Steel (#87/#100/#101); full component/TUI API; LSP client introspection; typed commands via `helix.scm`; `register_fn_with_ctx` implicit-context registration; rope regex + bulk removal; forge distribution (`cog.scm` with a `dylibs` key, built from source via cargo at install time — users need a Rust toolchain, and forge #423 means dylib build failures are silent). Stock does **not** have: any image/RawContent API, overlay/conceal API, math/virtual-line FFI, `document-focus-gained`, or any viewport-change observability (verified: no plugin-side workaround exists for scroll position).

# Fork Divergence Ledger

All 12 adversarial verification verdicts returned `classification_ok=true`; classifications below are final. Total fork delta: ~12,608 inserted lines.

| # | Area | What | ~LOC | Classification | Elimination path |
|---|------|------|-----:|----------------|------------------|
| 1 | `$HOME` generated cogs | 20 codegen artifacts under a literal unexpanded `$HOME/` dir | 7,553 | plugin-side-workaround | `git rm -r '$HOME'` — Helix regenerates these into real `~/.steel` at startup. Zero loss. |
| 2 | Docs/demos in fork repo | how_it_works.md, demo .jl/.ipynb, design specs, loose .md in helix-core/src | 1,716 | plugin-side-workaround | `git mv` to the nothelix repo. RAW_CONTENT_USAGE.md becomes rustdoc only if RawContent is PR'd. |
| 3 | Terminal graphics layer | GraphicsProtocol trait + Kitty impl, RawSurface, backend image methods, synchronized frames | 530 | pr-to-matt | Ship inside the one coherent inline-images RFC (M6). raw_surface.rs/terminal.rs need tests first; `current_buffer_mut` API break needs a story. |
| 4 | RawContent pipeline | Annotation type, ChangeSet remap, doc_formatter threading, Kitty placeholder painting | 620 | pr-to-matt | Centerpiece of the inline-images RFC. `remap_positions`/`replace_by_id` are untested; trace logging in the per-grapheme hot path must go; the plugin-inserts-blank-lines contract is the acceptance risk. |
| 5 | Steel FFI for images | Five overlapping adder entry points, error-level logging, docstring lies about (id,char_idx) idempotency | 200 | pr-to-matt | Consolidate to 1–2 functions *before* the RFC. No stock replacement exists — plugin cannot run on stock until this lands. |
| 6 | Animation | is_animating, AnimationConfig/Orchestrator, redraw debounce, 2 near-duplicate 6-arg FFI closures | 200 | pr-to-matt | Rides with/after the RawContent PR (literally depends on it). Collapse the duplicate closures first. |
| 7 | DocumentFocusGained + ViewportChanged | Two events + Steel hooks, mirrors merged #111/#112 pattern | 150 | pr-to-matt | **File immediately.** Highest-value, lowest-risk PR in the delta; verified no plugin-side workaround on stock. |
| 8 | Plugin overlay (conceal) | Per-view grapheme-replacement overlays, `ui.virtual.conceal`, set-overlays!/clear-overlays! | 130 | pr-to-matt | PR after hardening: accessor discipline on the bare pub field, edit-remap (or documented staleness contract). Stock inlay-hints are insert-semantics, not replace — no substitute. |
| 9 | Math virtual rows | MathLines above/below per source line, LineAnnotation + Decoration, set-math-lines-* FFI | 440 | pr-to-matt | Generalize first: rename to plugin virtual-lines, char-anchor instead of line-index, dedupe the LineAnnotation impl mirrored across two crates. Medium effort. |
| 10 | Codegen restructure + static.scm dedup | Pre-refactor wrapper-emission macros + 225 lines of dedup fixing a self-inflicted collision | 950 | replaceable-by-upstream-api | Adopt upstream `register_fn_with_ctx(CTX, ...)` for the fork's new functions; delete the macro pipeline and all dedup machinery. **Biggest single deletion with zero functionality loss.** |
| 11 | FFI robustness | try_get sweep, Tree::get diagnostic panics, commit-changes-to-history, insert_string debug trace | 140 | pr-to-matt | PR the sweep + diagnostics + commit-changes-to-history (crash classes exist upstream too); drop the ~40-line insert_string instrumentation. |
| 12 | Misc fixes + renames | grapheme_width("") guard, GlobSet pattern-loss bug (real upstream bug), doctest fix, equality_hint; plus pure-churn renames | 70 | pr-to-matt (split) | PR the four fixes (lead with GlobSet — it breaks re-serialization upstream); **revert** the extension.rs→steel_reflect.rs renames and Cargo.toml reorder in the fork. |

**Totals (post-verification):**

| Bucket | LOC | Share |
|---|---:|---:|
| plugin-side / fork hygiene (delete or move, no upstream interaction) | 9,269 | 73% |
| replaceable-by-upstream-api (adopt `register_fn_with_ctx`, delete) | 950 | 8% |
| pr-to-matt (real feature code) | 2,480 | 19% |
| fork-only (unupstreamable) | **0** | 0% |

Of the 2,480 pr-to-matt lines, ~1,550 (areas 3+4+5+6) is one inline-images RFC bundle; ~930 is small/medium independent PRs. **Nothing in the fork is condemned to stay fork-only.** Two days of hygiene plus one codegen adoption shrinks the visible delta from 12,608 to ~2,500 lines.

# Gap Analysis vs Goals

### 1. Rich math symbol rendering
**State:** strongest pillar — 5,300-line tested unicode subsystem, 2,544-symbol table, stacked limits/fracs via fork FFI, backslash-Tab completion, Typst concealer.
**Gaps:** dead-and-buggy `compute-and-apply-conceal-async` (conceal.scm:133 — violates no-dead-code rule); stacked rendering hard-gated to `# `-comment lines in .jl (math-render.scm ~137); no \sqrt/\overbrace/\binom visuals; `&` alignment lost in matrices; no nested \frac; conceal cycle fully synchronous.
**What new capabilities close:** stock **tree-sitter bindings (#87)** can replace the string-prefix comment gate — query comment/markdown nodes per language and stack math anywhere (.md/.qmd/.tex). Everything else is nothelix-internal work (scanner.rs additive cases, math_format.rs column pass); the fork dependency stays only on `set-math-lines-*`, which is the area-9 PR.

### 2. Well-formatted documents
**State:** math reflow (math_format.rs, idempotent, save-hooked) is good; code/prose formatting absent.
**Gaps:** JuliaFormatter vendored but unexposed; converter never emits the `# ─── Output ───` sections its own module header documents; export.rs promises Typst template macros it doesn't emit; no markdown prose formatting; format-math is .jl-only.
**What new capabilities close:** nothing needed from Steel/Helix — all stock-achievable today. The output-sections emit round-trips for free because cells.rs already strips the markers. `:format-cell` is a shell-out. Two doc-drift fixes are free wins.

### 3. Seamless .jl ↔ .ipynb conversion
**State:** best-tested area (31 tests), integrity-guarded round-trip, sidecar PNG embedding.
**Gaps:** raw cells coerced to code (lossy); attachments asymmetric (dropped silently on edit); outputs lost on cell reorder (positional-only matching); fresh cells lack nbformat ids; `.replace(".jl",…)` path bug ×3; exports drop all images/outputs; no SVG/HTML path; no corpus property test.
**What new capabilities close:** none required — this is pure libnothelix Rust with zero fork or Steel-version dependency (ByteVector FFI was never a blocker; it predates 2026). This is the highest-value work that is fully decoupled from upstreaming.

### 4. Editor proactively informs users
**State:** genuinely good — health.rs static checks, LTO-robust fork probe, doctor scripts, probe-and-degrade FFI with actionable status messages.
**Gaps:** the owner's #1 recurring footgun (stale dylib vs live-linked .scm) has no check; multi-issue status crammed into one truncating line; no .jl↔.ipynb staleness nudge; health runs once at load; ~⅓ of commands undocumented in README; no update check against releases (users on stale v0.1.1 told "healthy").
**What new capabilities close:** all achievable on **stock** APIs: `document-saved` hook for the sync-staleness nudge, the components API (picker pattern already in repo) for multi-issue status, a `nothelix-ffi-version` FFI constant for the dylib handshake. Terminal-focus auto-pause already converged with stock (#111/#112 merged into steel-event-system). Wire `steel-language-server` (now a prebuilt binary) into languages.toml for plugin-dev ergonomics; `;;@doc` comments feed LSP hover.

### 5. Distributable to other users
**State:** unusually real infrastructure (curl-sh installer, SHA256, nix CI releases, wrapper binary, bats suite, fork-rev pinning automation) — but pointed at a stale v0.1.1 release and an existential fork requirement.
**Gaps:** fork APIs (RawContent, math lines, overlays, focus-gained/viewport events) exist only on koalazub/helix; no written upstreaming plan (this report is it); release 2 months stale; `install.sh --uninstall` broken; no quality-gate CI despite clippy-clean rule; repo hygiene scares contributors (grammar leftovers, uv.lock, .devenv.flake.nix, `result` symlink); darwin-arm64 + linux-x86_64 only.
**What new capabilities close:** **#8675's imminent merge is the unlock** — once nothelix's needed APIs are in steel-event-system, "install the fork" becomes "install Helix with the experimental flag" and eventually stock. **Forge is the distribution mechanism**: `cog.scm` with a `dylibs` entry + `forge pkg install --git <url> --rev <sha>`; toolchain is now prebuilt (v0.8.2). Caveats to design around: users need cargo on PATH (esp. Nix), and forge #423 silently swallows dylib build failures — your existing doctor becomes the post-install verification step, a real differentiator.

# Refactor Roadmap

Ordering principle: **upstream review cycles are the long pole, so everything that touches mattwparas/helix files first**; fork hygiene goes before the post-#8675 rebase makes it expensive; conversion/health product work runs in parallel because it has zero fork dependency.

### M1 — Fork hygiene + small upstream PRs (1–2 weeks, start now)
**Scope (fork):** `git rm -r '$HOME'` (−7,553); move docs/demos/specs to nothelix (−1,716); revert extension.rs→steel_reflect.rs renames + Cargo.toml reorder; drop insert_string instrumentation. Delta: 12,608 → ~3,300.
**Scope (PRs to mattwparas/helix, file all four now):**
- **PR-1:** DocumentFocusGained + ViewportChanged events + hooks (150 LOC). Exact shape of merged #111/#112.
- **PR-2:** FFI robustness — try_get sweep, Tree::get diagnostic panics, `commit-changes-to-history` (~100 LOC). Fixes crash classes that exist upstream verbatim.
- **PR-3:** Tiny-fix batch, led by the GlobSet pattern-loss bug (real upstream bug breaking re-serialization), plus grapheme_width("") guard, list.rs doctest, equality_hint impls. Split into 2–3 PRs if Matt prefers.
- **PR-4:** Overlay/conceal API — but harden first: private field + accessors (mirror math_lines discipline), either ChangeSet remap via `update_positions` or an explicitly documented re-set contract, theme-key docs.
**Dependencies:** none. **Risk:** low — #111/#112/#113 precedent shows Matt merges exactly this category quickly. The only deadline pressure: file before #8675 lands and the branch enters rebase churn.

### M2 — Adopt upstream codegen + consolidate image FFI (1 week, fork-internal)
**Scope:** register all fork-new functions via `register_fn_with_ctx(CTX, …)`; delete the `template_function_arity_N` closures, macro pipeline, pending_emits queue, and the entire 225-line static.scm dedup machinery (−~950). Collapse the **five** image adders to one `add-raw-content!` (options for replace/animating) + maybe one placeholder variant; fix the (id,char_idx)-idempotency docstring lie (or implement it in `replace_by_id`); demote `log::error!` success logging; merge the two duplicate 6-arg animation closures.
**Deliverables:** fork mod.rs delta becomes "FFI registrations only"; plugin updated to the consolidated FFI; bump `nothelix-ffi-version` (see M3).
**Dependencies:** M1 hygiene merged (avoid conflicts). **Risk:** low-medium — plugin-facing FFI rename, but you control both sides; the live-link setup (nixoala) means coordinate `just install` with the .scm change atomically.

### M3 — Product sprint: conversion correctness + project credibility (2–3 weeks, parallel to M1/M2 review wait)
**Scope (conversion, zero fork dependency):** `strip_suffix` fix ×3 (convert.rs:240, export.rs:49/91); nbformat ids for fresh cells; `@raw` marker passthrough instead of coercing raw cells to code; extract attachments on ipynb→jl into `.nothelix/images/` + `@image` (makes embed.rs symmetric); content-based orig-matching fallback for reordered cells; corpus-driven ipynb→jl→ipynb fixpoint property test (fits the fuzzy-tests-by-default rule).
**Scope (credibility):** delete dead `compute-and-apply-conceal-async` (no-dead-code rule); purge grammar leftovers, uv.lock, .devenv.flake.nix, untrack `result`; CI workflow: `nix develop -c cargo nextest run` + `clippy --all-targets -- -D warnings` + fmt; fix `install.sh --uninstall` by delegating to dist/uninstall.sh; **cut a release from current main** (pipeline exists, unused since April — users are missing the FFI arity fix).
**Risk:** none meaningful. This is the highest user-visible-value/effort ratio in the whole plan.

### M4 — Health: close the freshness loop (1–2 weeks)
**Scope:** `nothelix-ffi-version` constant in lib.rs asserted from nothelix.scm at load → "dylib stale — run just install" (automates the owner's recurring footgun); multi-issue `:nothelix-status` rendered via picker/scratch buffer with per-issue fix hints; save-hook sync nudge on `document-saved` (stock hook): compare .jl cells vs paired .ipynb → "unsynced — :sync-to-ipynb"; `doctor --check-updates` against the releases API; generate the README command table from the registration metadata at nothelix.scm:229-281. Wire steel-language-server into languages.toml; add `;;@doc` comments.
**Dependencies:** M2 (FFI version constant ships with the consolidated dylib). **Risk:** none.

### M5 — The inline-images RFC to mattwparas/helix (start writing now, expect months)
**Scope:** one coherent RFC issue + PR series covering areas 3+4+5+6 (~1,550 LOC): graphics trait + Kitty impl + RawSurface; RawContent + remap pipeline; consolidated Steel FFI (from M2); animation config/orchestrator as the closing PR.
**Pre-work the verification demands:** tests for `remap_positions`, `replace_by_id`, RawSurface double-buffering, and Terminal::flush image diffing (currently only kitty.rs is tested); remove `log::trace!` from the `DocumentFormatter::next` hot path; resolve or explicitly defend the plugin-inserts-blank-buffer-lines contract (the single biggest acceptance risk — see Open Questions); soften the `current_buffer_mut` → `current_buffer_and_raw_mut` break (additive method instead).
**Dependencies:** M1 PRs establish credibility; M2 gives the clean FFI to present. **Risk:** high review churn by design — that's why it starts early. Timing note: pitch *after* #8675 lands and the branch stabilizes from rebase, but have the RFC text ready before.

### M6 — Generalize math virtual rows → "plugin virtual lines" PR (2–3 weeks + review)
**Scope:** rename off the math branding; char-anchor with ChangeSet remap (fixes the silent-misalignment-on-edit bug as a side effect); dedupe the LineAnnotation impl mirrored in helix-view and helix-term; then PR — pitch as generic plugin virtual lines (REPL output, doc popovers, diagnostics-style hints), with nothelix math as the demo.
**Dependencies:** independent of M5 (different machinery — reuses upstream's existing LineAnnotation hook), so it can overlap. **Risk:** medium; positioning rethink invites design debate, but upstream's own inline diagnostics use the same machinery.

### M7 — Math + formatting product wave (ongoing, interleaved)
**Scope:** lift the `# `-comment gate using stock tree-sitter queries (#87) so .md/.qmd/.tex get stacked rendering; column-alignment pass at `&` boundaries in math_format.rs; additive scanner.rs cases for \sqrt/\overbrace/\binom; emit `# ─── Output ───` sections on convert; `:format-cell` via JuliaFormatter (vendored already) behind a config flag; ship the promised Typst template + reference exported images from .md/.typ. Defer nested-\frac recursion until demand proves it.
**Risk:** low; all stock or already-pinned-fork APIs.

### M8 — Distribution endgame (after M5/M6 land or are reduced to a thin patchset)
**Scope:** `cog.scm` with package-name/version + `dylibs` entry; pin steel-core `b67efd5`/0.8.2, features `["dylibs","sync"]`; install docs: `forge pkg install --git <url> --rev <sha>`, cargo-on-PATH prerequisite (call out Nix explicitly), and **mandatory post-install `nothelix doctor`** because forge #423 hides dylib build failures. Keep the curl-sh tarball path as the no-toolchain alternative. Re-test macro-heavy plugin modules and any eval usage against the pinned rev (eval-context change #572, macro overhaul, #645 sharp edge). Consider serde program images (#636) for plugin state persistence and as groundwork for the Cap'n Proto kernel-IPC goal.
**Dependencies:** the fork delta must be ≈0 or a documented thin patchset. **Risk:** gated entirely on upstream review velocity — which is why M1/M5/M6 front-load it.

### What NOT to do
- **Don't wait for #8675 to merge before filing PRs** — mattwparas/helix is the active target now and remains the plugin host after the experimental merge.
- **Don't chase Steel master or enable jit2** — the JIT is the churniest subsystem and the Helix host runs interpreter-only; perf-critical paths belong in the dylib. Pin `b67efd5`/0.8.2.
- **Don't PR the renames/reorders** (extension.rs→steel_reflect.rs, Cargo.toml) — revert them; churn poisons real PRs.
- **Don't add any new fork-only FFI surface** from today — every addition extends the M8 timeline.
- **Don't attempt a plugin-side viewport workaround** — verified impossible on stock; the PR is the only path.
- **Don't build registry/plugin-manager tooling** — maintainer consensus puts it out of scope; forge + files-on-disk is the blessed path.
- **Don't start the Python kernel or Cap'n Proto IPC** before M8 — they multiply surface area while the existential blocker is still the fork.

# Open Questions for the Owner

1. **Ship-speed vs upstreamability during M5/M6 review:** while the inline-images RFC grinds (plausibly months), do you keep cutting fork-based releases for users from koalazub/helix, or freeze the fork at the M2 state and accept feature stasis? Recommendation is keep shipping fork releases with the pinned-rev automation — but it doubles maintenance during rebase churn, and only you can price that.
2. **The blank-lines contract:** RawContent requires the *plugin* to insert real blank buffer lines under image markers (phantom-row reservation was tried and reverted). This mutates user documents and is the most likely upstream rejection point. Are you willing to reopen that design (e.g., build image placement on the virtual-lines machinery from M6) before the RFC, or do you defend the current contract and risk a rewrite mid-review?
3. **Kitty-only vs protocol-agnostic:** the graphics trait is clean but only Kitty is implemented, and RawContent bakes in Kitty Unicode-placeholder semantics. Investing in a second protocol (iTerm2/Sixel) before the RFC materially improves acceptance odds at real cost. Worth it?
4. **Animation's fate:** the orchestrator is well-built but only meaningful atop RawContent and is the most likely piece reviewers cut. If upstream rejects it, do you keep it as a permanent thin fork patch, or drop animated output as a feature?
5. **ByteVector belief audit:** the claimed April-2026 ByteVector FFI patch does not exist in mattwparas/steel history. Where did the team's patch actually go — is anything in your build silently depending on a local/private Steel patch that needs accounting for before pinning 0.8.2?
6. **Distribution posture for M8:** forge (`pkg install --git`, requires user cargo toolchain — friction for Nix users) vs your curl-sh prebuilt tarball (you own CI + signing burden) — or maintain both indefinitely? This decides how much of install.sh/release.yml to keep investing in.
7. **Lossless round-trip as a hard requirement:** the `@raw` marker and attachment-extraction work (M3) add format surface to the .jl representation. Is byte-faithful nbformat round-trip a hard guarantee you want to advertise (and property-test against a corpus), or is "lossless for code+markdown+png" the spec?
8. **FFI rename break in M2:** consolidating five image adders to one breaks any existing user of the fork FFI (today, effectively only you). Confirm there are no other consumers before the cut, and whether `nothelix-ffi-version` should gate hard-fail vs degrade.
9. **linux-arm64:** detection exists, builds don't ship. In scope for the M3 release, or explicitly deferred?
10. **Release cadence:** on-merge-to-main automatic releases vs manual tags? The April-to-June staleness happened because cutting releases is a human step; pick a policy so M3's release isn't the last one again.
---

## Addendum: Open Question 5 resolved (2026-06-10)

Checked empirically against stock steel-core `b67efd5`:

- Stock HAS `FromFFIArg<'a> for RVec<u8>` (byte args) and the `FFIValue::ByteVector` variant.
- Stock LACKS `From<Vec<u8>> for FFIValue` / `FromFFIArg<'a> for Vec<u8>`. Stock's
  `IntoFFIVal for Vec<T>` would marshal a returned `Vec<u8>` as a Steel *list of
  integers*, not a bytevector — `animation.scm` calls `bytes-length` on tick output,
  so the animation pipeline depends on the local patch.
- The patch lives on the local branch `bytevector-ffi-return` in ~/projects/steel
  (rebased onto b67efd5, never PR'd). libnothelix consumes it via a
  `path = "../../steel/crates/steel-core"` dependency — itself a distribution blocker.
- Action: PR the patch to mattwparas/steel (tiny, ergonomic). Interim fallback:
  construct `FFIValue::ByteVector(RVec::from(v))` directly at return sites (works on
  stock); switch Cargo.toml to a git dependency pinned at b67efd5 either way.
