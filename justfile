# nothelix task runner
#
# `just install` installs the parts that change as you iterate:
#
#   ~/.steel/native/libnothelix.{dylib,so}  — FFI dylib, codesigned on macOS
#   ~/.steel/cogs/nothelix.scm              — plugin entry symlink
#   ~/.steel/cogs/nothelix/                 — plugin module directory symlink
#   ~/.local/share/nothelix/bin/nothelix-slm — SLM helper, best-effort (swiftc)
#
# `just setup-lsp` dev-installs the bundled NothelixMacros package and the
# kernel's JSON3 dependency into Julia's default shared environment (@v#.#),
# which JETLS and the kernel both resolve `using` against.
#
# Why ~/.steel/cogs? Steel's module resolver searches it automatically, so
# `(require "nothelix.scm")` in init.scm finds the plugin without any config
# wiring. This also sidesteps a class of breakage where config managers
# (home-manager, stow, chezmoi, dotfiles scripts) try to install per-file
# symlinks into ~/.config/helix/nothelix/ and walk through a whole-tree
# symlink into the project source, ultimately creating circular store ↔
# project links that loop back on themselves. STEEL_HOME is Steel's own
# territory and no sensible config manager touches it.
#
# macOS note: the kernel invalidates a dylib's code signature when the
# underlying file changes and SIGKILLs the process next time it pages in
# code — but only when Helix actually calls into the dylib, not during
# `hx --version`. Symlinks don't help because codesign stamps the real
# file. The install recipe does rm + cp + codesign so you don't have to
# remember the sequence.

set shell := ["nu", "-c"]

steel_native := env("HOME") / ".steel" / "native"
steel_cogs := env("HOME") / ".steel" / "cogs"
local_bin := env("HOME") / ".local" / "bin"
slm_bin := env("HOME") / ".local" / "share" / "nothelix" / "bin"
nothelix_root := justfile_directory()

# build + install the dylib and plugin symlinks (run after Rust changes)
install profile="release":
    #!/usr/bin/env nu
    if "{{ profile }}" == "debug" {
        print "Building libnothelix (debug)..."
        cargo build -p libnothelix
    } else if "{{ profile }}" == "fast" {
        print "Building libnothelix (fast: release opt, no LTO)..."
        cargo build --profile fast -p libnothelix
    } else {
        print "Building libnothelix (release)..."
        cargo build --release -p libnothelix
    }

    # ── Dylib ─────────────────────────────────────────────────────────────
    mkdir "{{ steel_native }}"

    let lib = if ("target/{{ profile }}/libnothelix.dylib" | path type) == "file" {
        {
            src: "target/{{ profile }}/libnothelix.dylib"
            dest: "{{ steel_native }}/libnothelix.dylib"
        }
    } else if ("target/{{ profile }}/libnothelix.so" | path type) == "file" {
        {
            src: "target/{{ profile }}/libnothelix.so"
            dest: "{{ steel_native }}/libnothelix.so"
        }
    } else {
        print "error: no built library in target/{{ profile }}/"
        exit 1
    }

    rm -f $lib.dest
    cp $lib.src $lib.dest

    if $nu.os-info.name == "macos" {
        codesign --force --sign "-" $lib.dest
    }

    print $"Installed: ($lib.dest)"

    # ── Plugin files ──────────────────────────────────────────────────────
    # Install as direct out-of-store symlinks into ~/.steel/cogs/. Steel's
    # require resolver picks up both the entry file and the module dir
    # automatically via the cogs fallback. Editing plugin sources in-place
    # in the repo reflects immediately in Helix — no rebuild step.
    mkdir "{{ steel_cogs }}"
    rm -f "{{ steel_cogs }}/nothelix.scm"
    rm -f "{{ steel_cogs }}/nothelix"
    ln -s "{{ nothelix_root }}/plugin/nothelix.scm" "{{ steel_cogs }}/nothelix.scm"
    ln -s "{{ nothelix_root }}/plugin/nothelix" "{{ steel_cogs }}/nothelix"
    print "Linked:    {{ steel_cogs }}/nothelix.scm"
    print "Linked:    {{ steel_cogs }}/nothelix/"

    # ── Warn about stale conflicting paths ────────────────────────────────
    # Helix's Steel engine searches ~/.config/helix/ before $STEEL_HOME/cogs.
    # A pre-existing nothelix.scm or nothelix/ directory there will shadow
    # this install and keep loading stale code.
    let config_helix = $"($env.HOME)/.config/helix"
    let stale_entry = ($"($config_helix)/nothelix.scm" | path exists)
    let stale_dir = ($"($config_helix)/nothelix" | path exists)
    if $stale_entry or $stale_dir {
        print ""
        print $"warning: found legacy install under ($config_helix)/"
        print "         Helix searches that directory before ~/.steel/cogs and will"
        print "         load the stale copy. Remove these entries (and any home-manager"
        print "         rules that manage them) before restarting Helix:"
        if $stale_entry { print $"           rm ($config_helix)/nothelix.scm" }
        if $stale_dir { print $"           rm -rf ($config_helix)/nothelix" }
    }

    # ── SLM helper (best-effort; opt-in on-device cell summaries) ───────────
    # Compiles the vendored Swift source once, from the system swiftc. No
    # swiftc on PATH (Linux, no Xcode CLT) just skips this step silently —
    # `slm-summaries` in .nothelix.conf stays a no-op until it's present.
    if (which swiftc | is-not-empty) {
        mkdir "{{ slm_bin }}"
        let slm_src = "{{ nothelix_root }}/tools/nothelix-slm/main.swift"
        let slm_out = "{{ slm_bin }}/nothelix-slm"
        if (swiftc $slm_src -o $slm_out | complete | get exit_code) == 0 {
            print $"Installed: ($slm_out)"
        } else {
            let sdks = (glob "/Library/Developer/CommandLineTools/SDKs/MacOSX*.sdk" | sort)
            let fallback = if ($sdks | is-empty) {
                1
            } else {
                with-env {DEVELOPER_DIR: "/Library/Developer/CommandLineTools"} {
                    swiftc -sdk ($sdks | last) $slm_src -o $slm_out | complete | get exit_code
                }
            }
            if $fallback == 0 {
                print $"Installed: ($slm_out) \(CommandLineTools SDK fallback)"
            } else {
                print "swiftc could not compile the SLM helper — skipping (summaries stay off)"
            }
        }
    } else {
        print "swiftc not found — skipping SLM helper (summaries stay off)"
    }

# Julia bootstrap: dev NothelixMacros + JSON3 into the default env (re-run after a Julia version change)
setup-lsp:
    #!/usr/bin/env nu
    if (which julia | is-not-empty) {
        # JSON3 is the kernel's runtime dependency, resolved against the user's
        # default env. A Julia version bump gives a fresh empty env, so ensure
        # it here. (@cell/@markdown markers no longer need a package — JETLS
        # masks them, so there is nothing else to install.)
        print "Ensuring default-env deps (JSON3)..."
        let ensure_json3 = 'using Pkg
        haskey(Pkg.project().dependencies, "NothelixMacros") && Pkg.rm("NothelixMacros")
        haskey(Pkg.project().dependencies, "JSON3") || Pkg.add("JSON3")'
        julia --startup-file=no --history-file=no --quiet "--project=@v#.#" -e $ensure_json3
    } else {
        print "julia not on PATH — skipping Julia env setup (re-run setup-lsp after installing Julia)"
    }

# build without installing
build profile="release":
    {{ if profile == "debug" { "cargo build -p libnothelix" } else { "cargo build --release -p libnothelix" } }}

# build the docs-site WebAssembly bundle into docs/assets/eng/wasm
build-wasm:
    #!/usr/bin/env nu
    let out = "{{ nothelix_root }}/docs/assets/eng/wasm"
    let bundle = $"($out)/nothelix_bg.wasm"
    let optimised = $"($bundle).opt"
    let opt_flags = [
        "-Oz"
        "--enable-reference-types"
        "--enable-multivalue"
        "--enable-sign-ext"
        "--enable-mutable-globals"
        "--enable-nontrapping-float-to-int"
        "--enable-bulk-memory"
    ]
    cargo build -p libnothelix --no-default-features --features wasm --target wasm32-unknown-unknown --release
    mkdir $out
    wasm-bindgen "{{ nothelix_root }}/target/wasm32-unknown-unknown/release/nothelix.wasm" --out-dir $out --out-name nothelix --target web --no-typescript
    wasm-opt ...$opt_flags $bundle -o $optimised
    mv $optimised $bundle
    let size = (ls $bundle | get 0.size | into int)
    print $"wasm: ($size) bytes"

# regenerate docs/_includes/engine/ and the README gallery regions from the shared snapshot fixtures
gallery:
    cargo run -p libnothelix --bin gen-gallery

# run tests
test:
    cargo nextest run -p libnothelix

# run the Julia kernel tests (AST sets + registry provenance notes + classifier)
test-kernel:
    julia "{{ nothelix_root }}/kernel/provenance_test.jl"
    julia "{{ nothelix_root }}/kernel/classify_test.jl"
    julia "{{ nothelix_root }}/kernel/duration_test.jl"
    julia "{{ nothelix_root }}/kernel/wavplay_test.jl"
    julia "{{ nothelix_root }}/kernel/widget_test.jl"
    julia "{{ nothelix_root }}/kernel/capture_test.jl"

# static gate: run before committing. Rust lints + tests, then load the
# plugin in a REAL hx binary to catch Steel load errors (FreeIdentifier,
# BadSyntax, ArityMismatch). Standalone `steel` can't do the Steel half —
# it lacks helix's native builtins, so it can't resolve `require-builtin
# helix/core/*` or check `helix.static.*` arities.
check:
    #!/usr/bin/env nu
    print "── clippy ──"
    cargo clippy -p libnothelix --all-targets -- -D warnings
    print "── nextest ──"
    if (which cargo-nextest | is-empty) { cargo install --locked cargo-nextest }
    cargo nextest run -p libnothelix
    print "── plugin load ──"
    ^"{{ nothelix_root }}/scripts/check-plugin.sh"

# remove the installed dylib and plugin symlinks
uninstall:
    rm -f "{{ steel_native }}/libnothelix.dylib"
    rm -f "{{ steel_native }}/libnothelix.so"
    rm -f "{{ steel_cogs }}/nothelix.scm"
    rm -f "{{ steel_cogs }}/nothelix"
    @print "Uninstalled nothelix"

# list available recipes
default:
    @just --list
