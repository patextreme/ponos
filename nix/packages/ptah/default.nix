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
      pname = "ptah";
      version = (builtins.fromTOML (builtins.readFile ../../../crates/ptah-cli/Cargo.toml)).package.version;
      CARGO_BUILD_RUSTFLAGS = "-C debuginfo=0";
    };

    # Dependencies are built from the cargo-only source so the artifact
    # cache stays insensitive to edits in src/, .ptah/, examples/, ...
    # Keep these arguments byte-identical to the ones in checks.nix: both
    # then evaluate to the same derivation and the deps build once.
    cargoArtifacts = craneLib.buildDepsOnly (commonArgs
      // {
        src = craneLib.cleanCargoSource ../../..;
      });
  in {
    # Full shared source (config.ptahSrc): the crate embeds
    # .ptah/ptah.d.luau at compile time, so a cargo-only source filter
    # breaks this build (while the deps build and tests still pass).
    packages.ptah = craneLib.buildPackage (commonArgs
      // {
        src = config.ptahSrc;
        inherit cargoArtifacts;
        doCheck = false;
        meta.mainProgram = "ptah";
        nativeBuildInputs = [pkgs.installShellFiles];
        # Generate completions from the just-built binary so installed
        # scripts always match the shipped command tree (see the cli spec's
        # completions requirement). `completions` is on the zero-setup,
        # pre-runtime dispatch path: no fs, registry, or agent needed, so
        # it is safe to run in the build sandbox.
        postInstall = ''
          installShellCompletion --cmd ptah \
            --zsh <($out/bin/ptah completions zsh) \
            --bash <($out/bin/ptah completions bash) \
            --fish <($out/bin/ptah completions fish)
        '';
      });

    packages.default = config.packages.ptah;
  };
}
