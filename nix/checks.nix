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

    # Static-analysis gate for the Luau surface: every bundled script
    # (examples, type-definition probe fixture) must pass luau-lsp in
    # strict mode (per-file --!strict directives; no committed .luaurc)
    # against the repo definitions. Keeps examples and types/ponos.d.luau
    # honest in the same direction as the runtime probe test.
    checks.ponos-analyze = pkgs.stdenv.mkDerivation {
      pname = "ponos-analyze";
      version = commonArgs.version;
      src = testSrc;

      nativeBuildInputs = [pkgs.luau-lsp];

      dontBuild = true;
      doCheck = true;

      checkPhase = ''
        runHook preCheck
        luau-lsp analyze --platform=standard \
          --definitions=types/ponos.d.luau \
          examples/*.luau tests/fixtures/*.luau
        runHook postCheck
      '';

      installPhase = ''
        mkdir -p $out
      '';
    };
  };
}
