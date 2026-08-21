{
  inputs,
  ...
}: {
  perSystem = {
    config,
    pkgs,
    ...
  }: {
    devShells.default = pkgs.mkShell {
      packages = [
        config.rustToolchain
        # Same analyzer the ponos-analyze check pins, for local editors
        # and `luau-lsp analyze` runs (see types/ponos.d.luau).
        pkgs.luau-lsp
      ];

      RUST_SRC_PATH = "${config.rustToolchain}/lib/rustlib/src/rust/library";
      RUST_BACKTRACE = 1;

      shellHook = ''
        echo "ponos devshell: $(rustc --version)"
      '';
    };
  };
}
