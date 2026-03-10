{
  description = "Nothelix development environment";

  inputs = {
    nixpkgs.url = "github:cachix/devenv-nixpkgs/rolling";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs =
    { self, nixpkgs, rust-overlay }:
    let
      systems = [
        "x86_64-linux"
        "aarch64-linux"
        "x86_64-darwin"
        "aarch64-darwin"
      ];
      forAllSystems = nixpkgs.lib.genAttrs systems;
    in
    {
      devShells = forAllSystems (
        system:
        let
          overlays = [ (import rust-overlay) ];
          pkgs = import nixpkgs { inherit system overlays; };

          rustToolchain = pkgs.rust-bin.nightly.latest.default.override {
            extensions = [
              "rustc"
              "cargo"
              "clippy"
              "rustfmt"
              "rust-analyzer"
            ];
          };

          beads = pkgs.buildGoModule {
            pname = "beads";
            version = "0.24.4";
            src = pkgs.fetchFromGitHub {
              owner = "steveyegge";
              repo = "beads";
              rev = "main";
              sha256 = "sha256-kHZXx+kygpVholOBsuQocCtksHo5ZWYskP64qK2Kjh0=";
            };
            subPackages = [ "cmd/bd" ];
            doCheck = false;
            vendorHash = "sha256-XAhe4yuLzP9vQ3IFhWAO5fN/3OOfokcRxfeGKaRYEws=";
            nativeBuildInputs = [ pkgs.git ];
            meta = with pkgs.lib; {
              description = "beads (bd) - An issue tracker designed for AI-supervised coding workflows";
              homepage = "https://github.com/steveyegge/beads";
              license = licenses.mit;
              mainProgram = "bd";
            };
          };

          beads-viewer = pkgs.buildGoModule {
            pname = "beads-viewer";
            version = "0.10.2";
            src = pkgs.fetchFromGitHub {
              owner = "Dicklesworthstone";
              repo = "beads_viewer";
              rev = "v0.10.2";
              sha256 = "sha256-GteCe909fpjjiFzjVKUY9dgfU7ubzue8vDOxn0NEt/A=";
            };
            subPackages = [ "cmd/bv" ];
            doCheck = false;
            proxyVendor = true;
            vendorHash = "sha256-eVGiZI2Bha+o+n3YletLzg05TGIwqCkfwMVBZhFn6qw=";
            meta = with pkgs.lib; {
              description = "beads-viewer (bv) - TUI for viewing beads issues";
              homepage = "https://github.com/Dicklesworthstone/beads_viewer";
              license = licenses.mit;
              mainProgram = "bv";
            };
          };

          nothelix-build = pkgs.writeShellScriptBin "nothelix-build" ''
            cargo build --release -p libnothelix
          '';

          nothelix-install = pkgs.writeShellScriptBin "nothelix-install" ''
            echo "=== Installing Nothelix ==="

            cargo build --release -p libnothelix

            mkdir -p ~/.steel/native
            mkdir -p ~/.config/helix/plugins

            PROJECT_DIR="$(pwd)"

            if [[ -f target/release/libnothelix.dylib ]]; then
              ln -sf "$PROJECT_DIR/target/release/libnothelix.dylib" ~/.steel/native/libnothelix.dylib
              ln -sf "$PROJECT_DIR/target/release/libnothelix.dylib" ~/.config/helix/plugins/libnothelix.dylib
            elif [[ -f target/release/libnothelix.so ]]; then
              ln -sf "$PROJECT_DIR/target/release/libnothelix.so" ~/.steel/native/libnothelix.so
              ln -sf "$PROJECT_DIR/target/release/libnothelix.so" ~/.config/helix/plugins/libnothelix.so
            else
              echo "Error: Could not find built library"
              echo "Expected: target/release/libnothelix.dylib or .so"
              ls -la target/release/lib* 2>/dev/null || echo "No lib* files found"
              exit 1
            fi

            ln -sf "$PROJECT_DIR/plugin/nothelix.scm" ~/.config/helix/plugins/nothelix.scm
            ln -sf "$PROJECT_DIR/plugin/nothelix" ~/.config/helix/plugins/nothelix

            echo ""
            echo "=== Installed (symlinked) ==="
            echo "Library: ~/.steel/native/libnothelix.dylib -> $PROJECT_DIR/target/release/libnothelix.dylib"
            echo "Plugin:  ~/.config/helix/plugins/nothelix.scm -> $PROJECT_DIR/plugin/nothelix.scm"
            echo "Modules: ~/.config/helix/plugins/nothelix -> $PROJECT_DIR/plugin/nothelix"
            echo ""
            echo "Add to ~/.config/helix/init.scm:"
            echo '  (require "nothelix.scm")'
          '';

          nothelix-uninstall = pkgs.writeShellScriptBin "nothelix-uninstall" ''
            rm -f ~/.steel/native/libnothelix.dylib
            rm -f ~/.steel/native/libnothelix.so
            rm -f ~/.config/helix/plugins/nothelix.scm
            echo "Uninstalled nothelix"
          '';

        in
        {
          default = pkgs.mkShell {
            buildInputs = [
              # Rust nightly toolchain (rustc, cargo, clippy, rustfmt, rust-analyzer)
              rustToolchain

              # Core project tools
              pkgs.bun
              pkgs.tree-sitter
              pkgs.git
              pkgs.bacon

              # Julia runtime (languages.julia.enable)
              pkgs.julia-bin

              # Nix language tooling (languages.nix.enable)
              pkgs.nixfmt-rfc-style
              pkgs.nil

              # Shell (exec nu in enterShell)
              pkgs.nushell

              # Custom Go tools
              beads
              beads-viewer

              # Project scripts
              nothelix-build
              nothelix-install
              nothelix-uninstall
            ];

            shellHook = ''
              exec nu
            '';
          };
        }
      );
    };
}
