{
  description = "Nothelix — Jupyter notebooks in Helix";

  # Toolchain inputs are pinned to explicit known-good revs so `nix flake
  # update` can't silently move them out from under the build — the dev
  # env only changes when a rev below is bumped on purpose (bump the rev,
  # run `nix flake lock`). This is what stops the periodic "toolchain
  # rotted" breakage. `helix-fork` stays on its branch ref because it's
  # actively developed and meant to track HEAD.
  #
  # NOTE: the macOS Julia is a dated nightly tarball pinned by url+hash in
  # the body below — it is the one input that can still rot, because Julia
  # prunes old nightlies from S3. Once built it lives in the nix store and
  # only refetches after GC; bump url+hash when it 404s (or switch to a
  # stable Julia once one ships the macOS-27 CSL fix).
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/89570f24e97e614aa34aa9ab1c927b6578a43775";
    fenix = {
      url = "github:nix-community/fenix/bd1a9586894a7702d9fbd0da7f6e3f09d6510c36";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    flake-utils.url = "github:numtide/flake-utils/11707dc2f618dd54ca8739b309ec4fc024de578b";
    rust-overlay = {
      url = "github:oxalica/rust-overlay/b7286019daa89e4e192c8913c3ec7002976034d0";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    helix-fork = {
      url = "github:koalazub/helix/feature/inline-image-rendering";
      flake = false;
    };
  };

  outputs =
    {
      self,
      nixpkgs,
      fenix,
      flake-utils,
      rust-overlay,
      helix-fork,
    }:
    let
      forAllSystems = flake-utils.lib.eachDefaultSystem;
    in
    forAllSystems (
      system:
      let
        overlays = [
          (import rust-overlay)
          fenix.overlays.default
        ];
        pkgs = import nixpkgs { inherit system overlays; };
        isDarwin = pkgs.stdenv.isDarwin;

        # devShell pins a dated nightly (not `.latest`) so it doesn't churn
        # on every `nix flake update` or rot after GC; build packages use
        # fenixStable. Bump this date deliberately when you want a newer
        # nightly. Two providers stay separate on purpose.
        rustToolchain = pkgs.rust-bin.nightly."2026-06-24".default.override {
          extensions = [
            "rustc"
            "cargo"
            "clippy"
            "rustfmt"
            "rust-analyzer"
          ];
          targets = [ "wasm32-unknown-unknown" ];
        };

        fenixStable = fenix.packages.${system}.stable.toolchain;

        # ─── Julia ───────────────────────────────────────────────────
        # macOS 27 (Tahoe) breaks BOTH stable Julia paths:
        #   • julia-bin 1.12.x bundles a libgcc_s.1.1.dylib whose
        #     LINKEDIT is mis-aligned; dyld4 refuses to dlopen it.
        #   • julia_112 (source, USE_BINARYBUILDER=0) no longer compiles
        #     cleanly against the macOS 27 SDK/toolchain.
        # The fix (CSL 1.5.3, JuliaLang/julia#62089, merged 2026-06-15)
        # only rides in master nightlies so far, so we pin a dated nightly
        # *binary* tarball. Bump url+hash deliberately for a newer nightly;
        # until a stable release ships the fix this is the only Julia that
        # loads on macOS 27.  https://github.com/JuliaLang/julia/issues/62044
        julia-nightly-darwin = pkgs.stdenvNoCC.mkDerivation {
          pname = "julia-nightly-bin";
          version = "1.14.0-DEV-2e0a778b56";
          src = pkgs.fetchurl {
            url = "https://julialangnightlies-s3.julialang.org/bin/macos/aarch64/1.14/julia-2e0a778b56-macos-aarch64.tar.gz";
            hash = "sha256-mrMJnS+3lSDAPVwu14C5JqvT+IRws91gfKt6X6xyXjY=";
          };
          dontConfigure = true;
          dontBuild = true;
          dontFixup = true;
          installPhase = ''
            runHook preInstall
            mkdir -p $out
            cp -R . $out
            runHook postInstall
          '';
          meta.mainProgram = "julia";
        };

        # Only aarch64-darwin needs the nightly today; Linux has no dyld
        # issue and Intel macs aren't in use, so keep the source build there.
        juliaPkg =
          if system == "aarch64-darwin" then julia-nightly-darwin else pkgs.julia_112;

        # ─── hx-nothelix (the fork binary) ───────────────────────────
        hx-nothelix = pkgs.rustPlatform.buildRustPackage {
          pname = "hx-nothelix";
          version = "0.1.0";
          src = helix-fork;

          cargoLock = {
            lockFile = "${helix-fork}/Cargo.lock";
            outputHashes = {
              "steel-core-0.8.2" = "sha256-qPDz0ax290E7UEFTDfrmLmsn1r9dIuOxMiRmNrDkfZo=";
            };
          };

          buildFeatures = [ "steel" ];
          cargoBuildFlags = [ "--features" "steel" ];

          nativeBuildInputs = with pkgs; [
            pkg-config
            fenixStable
            git
          ];

          buildInputs = with pkgs;
            [ zlib ]
            ++ pkgs.lib.optionals isDarwin [
              apple-sdk_15
            ];

          env = {
            HELIX_DEFAULT_RUNTIME = "${helix-fork}/runtime";
            HELIX_DISABLE_AUTO_GRAMMAR_BUILD = "1";
          };

          postInstall = ''
            mv $out/bin/hx $out/bin/hx-nothelix
            mkdir -p $out/lib
            cp -r ${helix-fork}/runtime $out/lib/runtime
          '';

          doCheck = false;

          meta = {
            description = "Helix fork with Steel + RawContent for nothelix";
            mainProgram = "hx-nothelix";
          };
        };

        # ─── libnothelix (the FFI dylib) ─────────────────────────────
        libnothelix = pkgs.rustPlatform.buildRustPackage {
          pname = "libnothelix";
          version = "0.1.0";
          src = ./.;

          cargoLock = {
            lockFile = ./Cargo.lock;
            outputHashes = {
              # Same hash as nixoala/packages/helix — same steel rev
              "steel-core-0.8.2" = "sha256-qPDz0ax290E7UEFTDfrmLmsn1r9dIuOxMiRmNrDkfZo=";
            };
          };

          cargoBuildFlags = [ "-p" "libnothelix" ];

          nativeBuildInputs = with pkgs; [
            pkg-config
            fenixStable
            git
          ];

          buildInputs = with pkgs;
            [ zlib ]
            ++ pkgs.lib.optionals isDarwin [
              apple-sdk_15
            ];

          env = {
            NOTHELIX_CI_BUILD = "1";
            NOTHELIX_BUILD_DATE = self.shortRev or self.dirtyShortRev or "local";
          };

          postInstall = ''
            mkdir -p $out/lib $out/bin
            # cargoInstallHook places binaries but not cdylib outputs.
            # Find the dylib wherever cargo left it and copy to $out/lib.
            dylib=$(find target -name 'libnothelix.dylib' -o -name 'libnothelix.so' 2>/dev/null | head -1)
            if [ -n "$dylib" ] && [ ! -f "$out/lib/$(basename "$dylib")" ]; then
              cp "$dylib" "$out/lib/"
            fi
            if [ -f $out/bin/nothelix-meta ]; then
              $out/bin/nothelix-meta > $out/lib/libnothelix.meta
            fi
          '';

          doCheck = false;

          meta.description = "nothelix Rust FFI library";
        };

        # ─── nothelix release tarball ─────────────────────────────────
        nothelix-tarball = pkgs.stdenvNoCC.mkDerivation {
          pname = "nothelix-tarball";
          version = "0.1.0";
          src = ./.;

          dontBuild = true;
          dontConfigure = true;
          dontPatchShebangs = true;
          dontFixup = true;

          buildInputs = [
            hx-nothelix
            libnothelix
          ];

          installPhase = ''
            runHook preInstall

            staging=$out/nothelix-v0.1.0-${
              if isDarwin then
                (
                  if system == "aarch64-darwin" then
                    "darwin-arm64"
                  else
                    "darwin-x86_64"
                )
              else
                (
                  if system == "aarch64-linux" then
                    "linux-arm64"
                  else
                    "linux-x86_64"
                )
            }

            mkdir -p $staging/bin $staging/lib
            mkdir -p $staging/share/nothelix/runtime
            mkdir -p $staging/share/nothelix/examples
            mkdir -p $staging/share/nothelix/plugin
            mkdir -p $staging/share/nothelix/lsp
            mkdir -p $staging/share/nothelix/kernel-scripts
            mkdir -p $staging/share/nothelix/dist

            # Binaries
            cp ${hx-nothelix}/bin/hx-nothelix $staging/bin/
            cp $src/dist/nothelix $staging/bin/nothelix
            chmod +x $staging/bin/*

            # Dylib + meta
            if [ -f ${libnothelix}/lib/libnothelix.dylib ]; then
              cp ${libnothelix}/lib/libnothelix.dylib $staging/lib/
            elif [ -f ${libnothelix}/lib/libnothelix.so ]; then
              cp ${libnothelix}/lib/libnothelix.so $staging/lib/
            fi
            if [ -f ${libnothelix}/lib/libnothelix.meta ]; then
              cp ${libnothelix}/lib/libnothelix.meta $staging/lib/
            fi

            # Runtime (from the fork)
            cp -r ${hx-nothelix}/lib/runtime/* $staging/share/nothelix/runtime/

            # Plugin
            cp $src/plugin/nothelix.scm $staging/share/nothelix/plugin/
            cp -r $src/plugin/nothelix $staging/share/nothelix/plugin/

            # Examples
            cp $src/examples/demo.jl $staging/share/nothelix/examples/

            # Kernel scripts
            cp $src/kernel/*.jl $staging/share/nothelix/kernel-scripts/

            # Doctor helpers
            cp -r $src/dist/doctor $staging/share/nothelix/dist/
            cp $src/dist/config.sh $src/dist/reset.sh $src/dist/uninstall.sh $staging/share/nothelix/dist/

            # Installer
            cp $src/dist/install-local.sh $staging/install-local.sh

            # VERSION — single source of truth from libnothelix.meta
            BUILD_ID=$(grep '^BUILD_ID=' $staging/lib/libnothelix.meta 2>/dev/null | head -1 | cut -d= -f2 || echo "nix-${self.shortRev or "local"}")
            FORK_SHA="${helix-fork.rev or "unknown"}"
            cat > $staging/VERSION <<VEOF
            NOTHELIX_VERSION=v0.1.0
            BUILD_ID=$BUILD_ID
            FORK_SHA=$FORK_SHA
            FORK_BRANCH=feature/inline-image-rendering
            LIBNOTHELIX_VERSION=0.1.0
            INSTALL_DATE=$(date -u +%Y-%m-%dT%H:%M:%SZ 2>/dev/null || echo unknown)
            VEOF

            runHook postInstall
          '';

          meta.description = "nothelix pre-built release tarball";
        };

      in
      {
        packages = {
          inherit hx-nothelix libnothelix nothelix-tarball;
          default = nothelix-tarball;
        };

        devShells.default = pkgs.mkShell {
          buildInputs = [
            rustToolchain
            pkgs.bun
            pkgs.tree-sitter
            pkgs.git
            juliaPkg
            pkgs.nixfmt
            pkgs.nil
            pkgs.nushell
            pkgs.just
            pkgs.wasm-bindgen-cli
            pkgs.binaryen
          ];

          shellHook = ''
            echo ""
            echo "nothelix dev shell"
            echo ""
            echo "  just install       build, install, and codesign the dylib"
            echo "  just test          run libnothelix tests"
            echo "  just check         clippy + nextest + plugin load gate"
            echo "  nix build          build the release tarball"
            echo ""
            # Only exec nu for interactive shells — `nix develop -c <cmd>`
            # sets arguments, so [ -t 0 ] guards non-interactive invocations.
            if [ -z "''${CI:-}" ] && [ -t 0 ] && [ $# -eq 0 ]; then
              exec nu
            fi
          '';
        };
      }
    );
}
