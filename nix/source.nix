{
  flake-parts-lib,
  ...
}: {
  # Declare the shared option so other perSystem modules can use it.
  options.perSystem = flake-parts-lib.mkPerSystemOption ({
    lib,
    ...
  }: {
    options.ponosSrc = lib.mkOption {
      # The cleanSourceWith result (outPath + filter); no fitting public
      # lib type, so leave it unspecified.
      description = ''
        Cleaned repo source shared by every derivation that compiles the
        crate (release package, test checks, smoke check). Keeping ONE
        source matters: the crate embeds non-Rust files at compile time
        (src/cli.rs does include_str!("../types/ponos.d.luau")), so a
        cargo-only source filter lets the dependency build and the test
        suite pass while the package build fails — exactly how `nix run`
        once regressed with `nix flake check` still green.
      '';
    };
  });

  config.perSystem = {pkgs, ...}: {
    ponosSrc = pkgs.lib.cleanSourceWith {
      src = ../.;
      filter = path: type:
        !(pkgs.lib.elem (baseNameOf path) [
          ".git"
          "target"
          ".work"
          ".pi"
          "openspec"
          "result"
          ".direnv"
          "worktrees"
        ]);
    };
  };
}
