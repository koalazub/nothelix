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
    {
      self,
      nixpkgs,
      rust-overlay,
    }:
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
        in
        {
          default = pkgs.mkShell {
            buildInputs = [
              rustToolchain
              pkgs.bun
              pkgs.tree-sitter
              pkgs.git
              pkgs.bacon
              pkgs.julia-bin
              pkgs.nixfmt-rfc-style
              pkgs.nil
              pkgs.nushell
              pkgs.just
              beads
              beads-viewer
            ];

            shellHook = ''
              echo ""
              echo "nothelix dev shell"
              echo ""
              echo "  just install       build, install, and codesign the dylib"
              echo "  just install debug  same but with debug profile"
              echo "  just test          run libnothelix tests"
              echo "  just uninstall     remove the installed dylib"
              echo ""
              exec nu
            '';
          };
        }
      );
    };
}
