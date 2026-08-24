{
  description = "Nothelix — Jupyter notebooks in Helix";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/2c423e03bbafcff28bfadc6781a4a8257f205cb5";
    fenix = {
      url = "github:nix-community/fenix/768d40d3acd4301a4ffb228fda12d388be24e97e";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    flake-utils.url = "github:numtide/flake-utils/11707dc2f618dd54ca8739b309ec4fc024de578b";
    rust-overlay = {
      url = "github:oxalica/rust-overlay/f60c1b57ff805a46b5175c76fc981fb4f81efbcc";
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
        isDarwin = pkgs.stdenv.hostPlatform.isDarwin;

        rustToolchain = pkgs.rust-bin.nightly."2026-08-23".default.override {
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

        julia-nightly-darwin = pkgs.stdenvNoCC.mkDerivation {
          pname = "julia-nightly-bin";
          version = "1.14.0-DEV-08eef7536a";
          src = pkgs.fetchurl {
            url = "https://julialangnightlies-s3.julialang.org/bin/macos/aarch64/1.14/julia-08eef7536a-macos-aarch64.tar.gz";
            hash = "sha256-N7ka4Ot/L9osvFQJFD5S+rwtuiqkPZbupxFAHoNIVyo=";
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

        juliaPkg =
          if system == "aarch64-darwin" then julia-nightly-darwin else pkgs.julia_112;

        hx-nothelix = pkgs.rustPlatform.buildRustPackage {
          pname = "hx-nothelix";
          version = "0.1.0";
          src = helix-fork;

          cargoLock = {
            lockFile = "${helix-fork}/Cargo.lock";
            outputHashes = {
              "steel-core-0.8.2" = "sha256-x+/tqIcuD/1orXOnAg97UUbeeuYitr0ke3F/OY0lM+U=";
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
              apple-sdk_26
            ];

          env = {
            HELIX_DEFAULT_RUNTIME = "${helix-fork}/runtime";
            HELIX_DISABLE_AUTO_GRAMMAR_BUILD = "1";
          } // pkgs.lib.optionalAttrs isDarwin {
            MACOSX_DEPLOYMENT_TARGET = "26.0";
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

        libnothelix = pkgs.rustPlatform.buildRustPackage {
          pname = "libnothelix";
          version = "0.1.0";
          src = ./.;

          cargoLock = {
            lockFile = ./Cargo.lock;
            outputHashes = {
              "steel-core-0.8.2" = "sha256-x+/tqIcuD/1orXOnAg97UUbeeuYitr0ke3F/OY0lM+U=";
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
              apple-sdk_26
            ];

          env = {
            NOTHELIX_CI_BUILD = "1";
            NOTHELIX_BUILD_DATE = self.shortRev or self.dirtyShortRev or "local";
          } // pkgs.lib.optionalAttrs isDarwin {
            MACOSX_DEPLOYMENT_TARGET = "26.0";
          };

          postInstall = ''
            mkdir -p $out/lib $out/bin
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

            cp ${hx-nothelix}/bin/hx-nothelix $staging/bin/
            cp $src/dist/nothelix $staging/bin/nothelix
            chmod +x $staging/bin/*

            if [ -f ${libnothelix}/lib/libnothelix.dylib ]; then
              cp ${libnothelix}/lib/libnothelix.dylib $staging/lib/
            elif [ -f ${libnothelix}/lib/libnothelix.so ]; then
              cp ${libnothelix}/lib/libnothelix.so $staging/lib/
            fi
            if [ -f ${libnothelix}/lib/libnothelix.meta ]; then
              cp ${libnothelix}/lib/libnothelix.meta $staging/lib/
            fi

            cp -r ${hx-nothelix}/lib/runtime/* $staging/share/nothelix/runtime/

            cp $src/plugin/nothelix.scm $staging/share/nothelix/plugin/
            cp -r $src/plugin/nothelix $staging/share/nothelix/plugin/

            cp $src/examples/demo.jl $staging/share/nothelix/examples/

            cp $src/kernel/*.jl $staging/share/nothelix/kernel-scripts/

            cp -r $src/dist/doctor $staging/share/nothelix/dist/
            cp $src/dist/config.sh $src/dist/reset.sh $src/dist/uninstall.sh $staging/share/nothelix/dist/

            cp $src/dist/install-local.sh $staging/install-local.sh

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
          ]
          ++ pkgs.lib.optionals isDarwin [ pkgs.apple-sdk_26 ];

          env = pkgs.lib.optionalAttrs isDarwin {
            MACOSX_DEPLOYMENT_TARGET = "26.0";
          };

          shellHook = ''
            echo ""
            echo "nothelix dev shell"
            echo ""
            echo "  just install       build, install, and codesign the dylib"
            echo "  just test          run libnothelix tests"
            echo "  just check         clippy + nextest + plugin load gate"
            echo "  nix build          build the release tarball"
            echo ""
            if [ -z "''${CI:-}" ] && [ -t 0 ] && [ $# -eq 0 ]; then
              exec nu
            fi
          '';
        };
      }
    );
}
