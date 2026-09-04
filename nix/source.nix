{
  flake-parts-lib,
  ...
}: {
  # Declare the shared option so other perSystem modules can use it.
  options.perSystem = flake-parts-lib.mkPerSystemOption ({
    lib,
    ...
  }: {
    options.ptahSrc = lib.mkOption {
      # The cleanSourceWith result (outPath + filter); no fitting public
      # lib type, so leave it unspecified.
      description = ''
        Cleaned repo source shared by every derivation that compiles the
        crate (release package, test checks, smoke check). Keeping ONE
        source matters: the workspace embeds non-Rust files at compile
        time (crates/ptah-check/src/defs.rs does
        include_str!("../../../.ptah/ptah.d.luau")), so a
        cargo-only source filter lets the dependency build and the test
        suite pass while the package build fails — exactly how `nix run`
        once regressed with `nix flake check` still green.
      '';
    };
  });

  config.perSystem = {pkgs, ...}: {
    ptahSrc = pkgs.lib.cleanSourceWith {
      src = ../.;
      filter = path: type:
      # Local runtime state and tooling configs — read from the
        # invocation dir at run time, never compile inputs — and nix/,
        # packaging only, so cargo builds stay insensitive to nix edits.
        # Two exceptions inside .ptah/: the checked-in type definitions
        # (a genuine compile input via include_str! in src/cli.rs) and
        # the workflow shims (test-covered code —
        # tests/factory_components.rs runs them against the mock agent,
        # so they must survive in the sandbox source). Keeping the rest
        # out means `nix run .` still does not rebuild when local .ptah
        # scripts/config (or editor/agent scaffolding) change.
        if pkgs.lib.hasSuffix "/.ptah" path
        then type == "directory"
        else if pkgs.lib.hasSuffix "/.ptah/ptah.d.luau" path
        then true
        else if pkgs.lib.hasInfix "/.ptah/workflows/" path
        then pkgs.lib.hasSuffix ".luau" path
        else if pkgs.lib.hasInfix "/.ptah/" path
        then false
        else
          !(pkgs.lib.elem (baseNameOf path) [
            ".git"
            "nix"
            "target"
            ".work"
            ".pi"
            "openspec"
            "result"
            ".direnv"
            "worktrees"
            ".agents"
            ".helix"
          ]);
    };
  };
}
