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

    # Install dylib (lib name is "nothelix" -> libnothelix.dylib/so)
    # Steel expects the name passed to #%require-dylib without the lib prefix
    if [[ -f target/release/libnothelix.dylib ]]; then
      cp target/release/libnothelix.dylib ~/.steel/native/libnothelix.dylib
    elif [[ -f target/release/libnothelix.so ]]; then
      cp target/release/libnothelix.so ~/.steel/native/libnothelix.so
    else
      echo "Error: Could not find built library"
      echo "Expected: target/release/libnothelix.dylib or .so"
      ls -la target/release/lib* 2>/dev/null || echo "No lib* files found"
      exit 1
    fi

    # Install plugin
    cp plugin/nothelix.scm ~/.config/helix/plugins/

    echo ""
    echo "=== Installed ==="
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
