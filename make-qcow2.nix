{ lib, config, pkgs, modulesPath, ... }:

{
  # This defines a derivation that produces a qcow2 image
  system.build.qcow2 = import "${toString modulesPath}/../lib/make-disk-image.nix" {
    inherit lib config pkgs;
    format = "qcow2";
    diskSize = 8192; # Size in MiB
    memSize = 1024;
    configFile = pkgs.writeText "configuration.nix"
      ''
        { config, lib, pkgs, ... }:
        {
          imports = [ ${toString config.system.build.toplevel}/etc/nixos/configuration.nix ];
        }
      '';
  };
}
