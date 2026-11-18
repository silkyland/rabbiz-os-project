#!/usr/bin/env bash
# install_fex_rootfs.sh — provision the x86 guest rootfs that Valve's
# FEX-Emu compat tool expects at /usr/share/guestos/fex-mesa on the A21s.
#
# Steam's fex-compat-tool hardcodes g_fex_rootfs_with_mesa=/usr/share/guestos/fex-mesa
# (the "OS provided fex+mesa" layout used by SteamOS arm64). We download FEX's
# official ArchLinux.sqsh (glibc + Mesa + graphics_provider.json inside) and
# FUSE-mount it via squashfuse — the A21s kernel lacks squashfs zstd support,
# and extracting ~3GB does not fit the 22GB rootfs.
#
# Usage: ./scripts/install_fex_rootfs.sh [user@host]   (default root@172.16.42.1)
set -euo pipefail

TARGET="${1:-root@172.16.42.1}"
SQSH_URL="https://rootfs.fex-emu.gg/ArchLinux/2026-08-11/ArchLinux.sqsh"
SQSH_SHA256="5d0c1a38590c68e5c2597c2c8a26d2f80170b1b738c857d63e1cdadada5f5f2a"

ssh "${TARGET}" bash -s <<EOF
set -euo pipefail

pacman -S --noconfirm --needed squashfuse

mkdir -p /usr/share/fex-emu/RootFS /usr/share/guestos/fex-mesa
if [ ! -f /usr/share/fex-emu/RootFS/ArchLinux.sqsh ]; then
  echo ">> downloading ArchLinux.sqsh (~1.3GB)..."
  curl -fSL --retry 3 -o /usr/share/fex-emu/RootFS/ArchLinux.sqsh "${SQSH_URL}"
fi
echo "${SQSH_SHA256}  /usr/share/fex-emu/RootFS/ArchLinux.sqsh" | sha256sum -c -

systemctl daemon-reload
systemctl enable --now fex-guestos.service

# ~/.steam/sdkarm64/steam-launch-wrapper is referenced by the arm64 client
# for compat launches, but no arm64 build of the wrapper ships in any depot.
# It only manages systemd scopes/journal redirection, so a passthrough shim
# is sufficient: skip the flags, exec the payload.
mkdir -p /home/rabbiz/.steam/sdkarm64
cat > /home/rabbiz/.steam/sdkarm64/steam-launch-wrapper <<'SHIM'
#!/bin/bash
# Minimal arm64 shim for steam-launch-wrapper (scope/journal mgmt skipped)
while [ \$# -gt 0 ]; do
  case "\$1" in
    --) shift; break ;;
    --app-id|--game-bin|--oom-score-adjust) shift 2 ;;
    --version) echo 'steam-launch-wrapper shim 1.0'; exit 0 ;;
    --*) shift ;;
    *) break ;;
  esac
done
exec "\$@"
SHIM
chmod 755 /home/rabbiz/.steam/sdkarm64/steam-launch-wrapper
chown -R rabbiz:rabbiz /home/rabbiz/.steam/sdkarm64

echo ">> guestos mounted:"
ls /usr/share/guestos/fex-mesa/graphics_provider.json
EOF
