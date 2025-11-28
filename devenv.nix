{
  pkgs,
  lib,
  config,
  ...
}:
{
  packages = [
    pkgs.bun
    pkgs.tree-sitter
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
