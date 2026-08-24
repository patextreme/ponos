{
  inputs,
  ...
}: {
  # Offline test suite: the integration tests drive the in-repo mock agent
  # only (no network). The source is the shared config.ponosSrc so the
  # examples and fixtures the tests run are the same tree the package is
  # built from.
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

    # Same derivation as in package.nix (keep the arguments
    # byte-identical): dependencies build once and are shared between
    # the release package and the test suite.
    cargoArtifacts = craneLib.buildDepsOnly (commonArgs
      // {
        src = craneLib.cleanCargoSource ../.;
      });
  in {
    checks.ponos-tests = craneLib.cargoTest (commonArgs
      // {
        src = config.ponosSrc;
        inherit cargoArtifacts;
        # tests/analyze.rs runs the *real* luau-lsp through `ponos check`
        # (the embedded definitions under test) and discovers it via
        # PATH; PONOS_REQUIRE_REAL_LSP makes its absence a hard failure
        # here so the sandbox can never silently skip that contract.
        nativeBuildInputs = [pkgs.luau-lsp];
        env.PONOS_REQUIRE_REAL_LSP = "1";
      });

    # `nix flake check` evaluates packages but does not build them, so a
    # broken release build only surfaced on `nix build`/`nix run`. This
    # check closes that gap: it builds the actual flake package and
    # drives the same binary `nix run` would start — CLI entry points,
    # the compile-time-embedded type definitions, and one bundled
    # example round-tripped through the in-repo mock agent (mirrors
    # tests/examples.rs; fully offline).
    checks.ponos-smoke = pkgs.runCommand "ponos-smoke" {
      nativeBuildInputs = [config.packages.ponos];
    } ''
      set -e
      ponos --version
      ponos --help > /dev/null

      # The embedded definitions (include_str! of types/ponos.d.luau in
      # src/cli.rs) must actually be in the release binary.
      ponos types | head -n1 | grep -q "type definitions"

      # End-to-end: run a bundled example against the mock agent with a
      # generated project registry, exactly like tests/examples.rs.
      work=$(mktemp -d)
      mkdir -p "$work/.ponos"
      cat > "$work/.ponos/config.toml" <<EOF
      [agents.demo]
      command = "${config.packages.ponos}/bin/mock-agent"
      args = []
      EOF
      (cd "$work" && ponos run "${config.ponosSrc}/examples/fanout.luau") > /dev/null

      touch $out
    '';

    # Static-analysis gate for the Luau surface: every bundled script
    # (examples, type-definition probe fixture) must pass luau-lsp in
    # strict mode (per-file --!strict directives; no committed .luaurc)
    # against the repo definitions. Keeps examples and types/ponos.d.luau
    # honest in the same direction as the runtime probe test.
    checks.ponos-analyze = pkgs.stdenv.mkDerivation {
      pname = "ponos-analyze";
      version = commonArgs.version;
      src = config.ponosSrc;

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
