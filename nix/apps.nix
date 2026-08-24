{
  lib,
  ...
}: {
  perSystem = {
    config,
    ...
  }: {
    apps.default = {
      type = "app";
      program = lib.getExe config.packages.default;
    };
  };
}
