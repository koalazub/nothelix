{
  pkgs,
  lib,
  config,
  ...
}:
{
  # https://devenv.sh/packages/
  packages = [
    pkgs.bun
    pkgs.tree-sitter
  ];

  # https://devenv.sh/languages/
  languages.javascript.enable = true;

  languages.nix.enable = true; # for working with Nix files

  enterShell = ''
      exec nu
  '';

}
