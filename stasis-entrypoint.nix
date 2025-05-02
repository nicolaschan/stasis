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

  MEMORY_MIB=2048
  SNAPSHOT_NAME="vm_snapshot_latest"
  VM_IMAGE="''${STASIS_VM_IMAGE:-build/app.qcow2}"
  SOCKET_DIR="''${STASIS_SOCKET_DIR:-/tmp}"

  if [[ "''${STASIS_AUTO_BUILD_IMAGE:-false}" == "true" ]] && [ ! -f "$VM_IMAGE" ]; then
    nix build .#vm
    mkdir -p "$(dirname "$VM_IMAGE")"
    cp -L result/nixos.qcow2 "$VM_IMAGE"
    chmod 644 "$VM_IMAGE"
  fi

  if qemu-img snapshot -l "$VM_IMAGE" | grep -q $SNAPSHOT_NAME; then
    LOADVM_ARG=("-loadvm" "$SNAPSHOT_NAME")
    stasis-tools unfreeze --wait &
  else
    LOADVM_ARG=()
  fi

  echo "VM_IMAGE=$VM_IMAGE"
  echo "SOCKET_DIR=$SOCKET_DIR"
  echo "LOADVM_ARG=''${LOADVM_ARG[*]}"

  mkdir -p "$SOCKET_DIR"
  qemu-system-x86_64 -enable-kvm -m $MEMORY_MIB -cpu host -nographic \
    -drive if=virtio,file="$VM_IMAGE" \
    -netdev user,id=net0,hostfwd=tcp::2222-:22,hostfwd=tcp::25560-:25565 -device virtio-net-pci,netdev=net0 \
    -device virtio-serial \
    -chardev socket,path="$SOCKET_DIR"/qga.sock,wait=off,server=on,id=qga0 \
    -device virtserialport,chardev=qga0,name=org.qemu.guest_agent.0 \
    -qmp unix:"$SOCKET_DIR"/qemu-sock,server,nowait \
    -monitor unix:"$SOCKET_DIR"/qemu_monitor.sock,server,nowait "''${LOADVM_ARG[@]}"
  '';
}
