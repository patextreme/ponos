{
  config,
  inputs,
  flake-parts-lib,
  ...
}: {
  # Declare the shared option so other perSystem modules can use it.
  options.perSystem = flake-parts-lib.mkPerSystemOption ({
    lib,
    ...
  }: {
    options.rustToolchain = lib.mkOption {
      type = lib.types.package;
      description = "Rust toolchain pinned by rust-toolchain.toml (oxalica overlay)";
    };
  });

  # Bring the oxalica rust overlay into scope for all perSystem modules,
  # then derive the pinned toolchain from rust-toolchain.toml so nix and
  # plain rustup agree on the exact nightly.
  config = {
    perSystem = {
      system,
      ...
    }: let
      overlayPkgs = import inputs.nixpkgs {
        inherit system;
        overlays = [inputs.rust-overlay.overlays.default];
      };
    in {
      _module.args.pkgs = overlayPkgs;

      rustToolchain = overlayPkgs.rust-bin.fromRustupToolchainFile ../rust-toolchain.toml;
    };
  };
}
