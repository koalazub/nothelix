{
  pkgs,
  lib,
  config,
  ...
}:
let
  beads = pkgs.buildGoModule {
    pname = "beads";
    version = "0.24.4";
    
    src = pkgs.fetchFromGitHub {
      owner = "steveyegge";
      repo = "beads";
      rev = "main";
      sha256 = "sha256-PCsU2GwrPYFAE2KTCdk60E6ZTPRtgNdbPF+bzf8qi3Q=";
    };
    
    subPackages = [ "cmd/bd" ];
    doCheck = false;
    vendorHash = "sha256-iTPi8+pbKr2Q352hzvIOGL2EneF9agrDmBwTLMUjDBE=";
    
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
  packages = with pkgs; [
    bun
    tree-sitter
    git
    beads
    beads-viewer
    bacon  # Background Rust checker for better error visibility
  ];

  languages.javascript.enable = true;
  languages.nix.enable = true;

  languages.rust = {
    enable = true;
    channel = "nightly";
    components = [ "rustc" "cargo" "clippy" "rustfmt" "rust-analyzer" ];
  };
  languages.julia.enable = true;

  scripts.nothelix-build.exec = ''
    cargo build --release -p libnothelix
  '';

  scripts.nothelix-install.exec = ''
    echo "=== Installing Nothelix ==="

    # Build
    cargo build --release -p libnothelix

    # Create directories
    mkdir -p ~/.steel/native
    mkdir -p ~/.config/helix/plugins

    # Get absolute path to project
    PROJECT_DIR="$(pwd)"

    # Symlink dylib to both locations (Steel native + Helix plugins)
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

    # Symlink plugin files
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

  scripts.nothelix-uninstall.exec = ''
    rm -f ~/.steel/native/libnothelix.dylib
    rm -f ~/.steel/native/libnothelix.so
    rm -f ~/.config/helix/plugins/nothelix.scm
    echo "Uninstalled nothelix"
  '';

  enterShell = ''
    exec nu
  '';
}
