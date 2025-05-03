{ pkgs, writeShellApplication, stasisTools, ... }:

writeShellApplication {
  name = "stasis-entrypoint";

  runtimeInputs = [
    pkgs.nix
    pkgs.qemu_kvm
    pkgs.coreutils
    stasisTools
  ];

  text = ''
  #!${pkgs.runtimeShell}

  nix run github:nicolaschan/lazytcp -- \
    --command '/bin/stasis-resume' \
    --downstream-addr localhost:25566 \
    --listen-addr localhost:25565 \
    --stdout-ready-pattern 'RCON running' \
    --shutdown-stdin-command stop \
    --debounce-time-millis 10000
  '';
}
