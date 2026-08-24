{
  ...
}: {
  perSystem = {
    config,
    pkgs,
    ...
  }: {
    devShells.default = pkgs.mkShell {
      packages = with pkgs; [
        config.rustToolchain
        luau-lsp
        luau
      ];

      RUST_SRC_PATH = "${config.rustToolchain}/lib/rustlib/src/rust/library";
      RUST_BACKTRACE = 1;

      shellHook = ''
        echo "ponos devshell: $(rustc --version)"
      '';
    };
  };
}
