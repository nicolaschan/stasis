({
  pkgs,
  modulesPath,
  lib,
  config,
  ...
}: {
  imports = [(modulesPath + "/profiles/qemu-guest.nix")];
  environment.systemPackages = [pkgs.neovim];

  # Basic system configuration
  users.users.root.initialPassword = "nixos";

  users.users.nixos = {
    isNormalUser = true;
    initialPassword = "nixos";
    extraGroups = ["wheel" "docker"];
    packages = [pkgs.busybox];
  };

  # Enable SSH for remote access
  services.openssh.enable = true;

  services.qemuGuest.enable = true;

  # Set hostname
  networking.hostName = "nixos-vm";

  nix = {
    extraOptions = ''
      experimental-features = nix-command flakes
    '';
  };

  # Bootloader configuration
  boot.loader.timeout = 5; # seconds
  boot.loader.grub.enable = true;
  boot.loader.grub.device = "/dev/vda";

  # Filesystem configuration
  fileSystems."/" = {
    device = "/dev/vda1";
    fsType = "ext4";
  };

  virtualisation.docker = {
    enable = true;
    enableOnBoot = true;
  };

  systemd.services.app-container = {
    description = "App Container";
    wantedBy = ["multi-user.target"];
    requires = ["docker.service"];
    after = ["docker.service"];

    serviceConfig = {
      Restart = "always";
      ExecStartPre = [
        "-${pkgs.docker}/bin/docker stop app"
        "-${pkgs.docker}/bin/docker rm app"
      ];
      ExecStart = ''
        ${pkgs.docker}/bin/docker run \
              --name app \
              --rm \
              -p 25565:25565 \
              -v /data:/data \
              -e EULA=true \
              itzg/minecraft-server:latest
      '';
      ExecStop = "${pkgs.docker}/bin/docker stop app";
    };
  };

  system.stateVersion = "25.05";
})
