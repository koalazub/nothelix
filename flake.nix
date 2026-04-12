{
  description = "Nothelix — Jupyter notebooks in Helix";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
    fenix = {
      url = "github:nix-community/fenix";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    flake-utils.url = "github:numtide/flake-utils";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
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

        rustToolchain = pkgs.rust-bin.nightly.latest.default.override {
          extensions = [
            "rustc"
            "cargo"
            "clippy"
            "rustfmt"
            "rust-analyzer"
          ];
        };

        fenixStable = fenix.packages.${system}.stable.toolchain;

        # ─── hx-nothelix (the fork binary) ───────────────────────────
        hx-nothelix = pkgs.rustPlatform.buildRustPackage {
          pname = "hx-nothelix";
          version = "0.1.0";
          src = helix-fork;

          cargoLock = {
            lockFile = "${helix-fork}/Cargo.lock";
            outputHashes = {
              "steel-core-0.8.2" = "sha256-lqtx1q/AHntbZvF3rpWbicvxE3NGZU+VPMueECaVdSA=";
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
              "steel-core-0.8.2" = "sha256-lqtx1q/AHntbZvF3rpWbicvxE3NGZU+VPMueECaVdSA=";
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
            cp $src/lsp/julia-lsp $staging/bin/julia-lsp
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

            # LSP
            cp $src/lsp/Project.toml $src/lsp/Manifest.toml $staging/share/nothelix/lsp/

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
            pkgs.bacon
            pkgs.julia-bin
            pkgs.nixfmt
            pkgs.nil
            pkgs.nushell
            pkgs.just
          ];

          shellHook = ''
            echo ""
            echo "nothelix dev shell"
            echo ""
            echo "  just install       build, install, and codesign the dylib"
            echo "  just test          run libnothelix tests"
            echo "  nix build          build the release tarball"
            echo ""
            exec nu
          '';
        };
      }
    );
}
