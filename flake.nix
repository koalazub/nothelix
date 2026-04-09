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
              pkgs.nixfmt
              pkgs.nil
              pkgs.nushell
              pkgs.just
            ];

            shellHook = ''
              # Ensure Julia LanguageServer.jl is installed (one-time, persists in ~/.julia)
              if ! julia -e 'using LanguageServer' 2>/dev/null; then
                echo "Installing LanguageServer.jl (one-time)..."
                julia -e 'import Pkg; Pkg.add("LanguageServer")'
              fi

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
