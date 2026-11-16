#!/usr/bin/env bash
# verify_apps.sh — which target base apps exist in the Arch ARM repos / on the device (spike S4, M7.4).
# Usage: ./scripts/verify_apps.sh [user@host]   (default root@172.16.42.1). Read-only.
set -euo pipefail
HOST="${1:-root@172.16.42.1}"
PKGS=(kalk kclock marknote buho index-fm dolphin maui-pix vvave merkuro plasma-settings plasma-dialer spacebar gnome-calls chatty xdg-desktop-portal xdg-desktop-portal-kde)
ssh "$HOST" 'df -h / | tail -1'
for p in "${PKGS[@]}"; do
  inst=$(ssh "$HOST" "pacman -Q $p 2>/dev/null" || true)
  if [ -n "$inst" ]; then echo "installed  $inst"; continue; fi
  size=$(ssh "$HOST" "pacman -Si $p 2>/dev/null | awk -F': *' '/Installed Size/{print \$2}'" || true)
  echo "${size:+available  $p ($size)}${size:-MISSING    $p}"
done
