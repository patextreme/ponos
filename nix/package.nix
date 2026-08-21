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
      src = craneLib.cleanCargoSource ../.;
      pname = "ponos";
      version = (builtins.fromTOML (builtins.readFile ../Cargo.toml)).package.version;
      CARGO_BUILD_RUSTFLAGS = "-C debuginfo=0";
    };
  in {
    packages.ponos = craneLib.buildPackage (commonArgs
      // {
        cargoArtifacts = craneLib.buildDepsOnly commonArgs;
        doCheck = false;
        meta.mainProgram = "ponos";
      });

    packages.default = config.packages.ponos;
  };
}
