{
  inputs,
  lib,
  ...
}: {
  # Offline test suite: the integration tests drive the in-repo mock agent
  # only (no network). The source includes the Luau examples so the
  # example tests can run them from the sandbox.
  perSystem = {
    config,
    pkgs,
    ...
  }: let
    craneLib = (inputs.crane.mkLib pkgs).overrideToolchain config.rustToolchain;

    commonArgs = {
      pname = "ponos";
      version = (builtins.fromTOML (builtins.readFile ../Cargo.toml)).package.version;
      CARGO_BUILD_RUSTFLAGS = "-C debuginfo=0";
    };

    cargoArtifacts = craneLib.buildDepsOnly (commonArgs
      // {
        src = craneLib.cleanCargoSource ../.;
      });

    testSrc = pkgs.lib.cleanSourceWith {
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
        ]);
    };
  in {
    checks.ponos-tests = craneLib.cargoTest (commonArgs
      // {
        src = testSrc;
        inherit cargoArtifacts;
      });
  };
}
