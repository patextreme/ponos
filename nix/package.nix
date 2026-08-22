{
  inputs,
  ...
}: {
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

    # Dependencies are built from the cargo-only source so the artifact
    # cache stays insensitive to edits in src/, types/, examples/, ...
    # Keep these arguments byte-identical to the ones in checks.nix: both
    # then evaluate to the same derivation and the deps build once.
    cargoArtifacts = craneLib.buildDepsOnly (commonArgs
      // {
        src = craneLib.cleanCargoSource ../.;
      });
  in {
    # Full shared source (config.ponosSrc): the crate embeds
    # types/ponos.d.luau at compile time, so a cargo-only source filter
    # breaks this build (while the deps build and tests still pass).
    packages.ponos = craneLib.buildPackage (commonArgs
      // {
        src = config.ponosSrc;
        inherit cargoArtifacts;
        doCheck = false;
        meta.mainProgram = "ponos";
      });

    packages.default = config.packages.ponos;
  };
}
