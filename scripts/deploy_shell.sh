#!/usr/bin/env bash
# deploy_shell.sh — push a21s-shell + rootfs-overlay to the Galaxy A21s.
#
# Usage:
#   ./scripts/deploy_shell.sh [user@host]     # default root@172.16.42.1
#   ./scripts/deploy_shell.sh --packages      # only print the pacman line
#
# Layout (see shell/README.md):
#   shell/          -> /usr/share/a21s-shell/
#   rootfs-overlay/ -> /   (merged; NEVER deletes; no /lib anywhere in overlay)
#
# Uses `tar | ssh` instead of rsync so it works on hosts without rsync and
# regardless of the local uid (ownership fixed remotely after extract).

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

# Packages needed on the device for the phone session (see
# docs/a21s-phone-shell.md for notes on availability/naming).
PACKAGES=(
  labwc                  # wlroots compositor (layer-shell + XWayland)
  quickshell             # shell surfaces (QML) — ALARM extra 0.3.2-1 (verified)
  qt6-declarative        # Qt/QML runtime (quickshell pulls qt6-declarative)
  qt6-svg
  qt6-wayland
  xorg-xwayland          # XWayland for gamescope/steam and X11 clients
  gamescope              # nested Steam session
  squeekboard            # on-screen keyboard (optional but recommended)
  noto-fonts
  ttf-nerd-fonts-symbols # icon glyphs used by the shell
  upower                 # battery service consumed by A21s.Services
  brightnessctl          # XF86 brightness keybinds / shade slider
)

print_packages() {
  echo "Install on the device as root:"
  echo
  echo "  pacman -Sy --needed ${PACKAGES[*]}"
  echo
  echo "Notes:"
  echo "  - 'ttf-nerd-fonts-symbols' is unverified against"
  echo "    ALARM aarch64 repos; check before install."
  echo "    copy a Nerd Fonts ttf into /usr/share/fonts/TTF/."
  echo "  - 'steam' itself is not packaged: native ARM64 client lives in"
  echo "    /home/gamer/.local/share/Steam (see docs/a21s-game-results.md)."
}

if [[ "${1:-}" == "--packages" || "${1:-}" == "-p" ]]; then
  print_packages
  exit 0
fi

TARGET="${1:-root@172.16.42.1}"

echo ">> Deploying a21s-shell to ${TARGET}"
echo "   repo: ${REPO_ROOT}"

# --- sanity checks -------------------------------------------------------
[[ -f "${REPO_ROOT}/shell/shell.qml" ]] || {
  echo "ERROR: ${REPO_ROOT}/shell/shell.qml missing — shell/ not populated?" >&2
  exit 1
}
[[ -d "${REPO_ROOT}/rootfs-overlay" ]] || {
  echo "ERROR: ${REPO_ROOT}/rootfs-overlay missing" >&2
  exit 1
}
# Guard rail (AGENTS.md): never ship a lib/ dir in the overlay — /lib is a
# symlink to usr/lib on the device; extracting a real lib/ dir breaks boot.
if [[ -e "${REPO_ROOT}/rootfs-overlay/lib" || -e "${REPO_ROOT}/rootfs-overlay/usr/lib" ]]; then
  echo "ERROR: rootfs-overlay contains lib/ or usr/lib/ — refusing to deploy." >&2
  exit 1
fi

# --- deploy shell QML tree ----------------------------------------------
echo ">> shell/ -> /usr/share/a21s-shell/"
ssh "${TARGET}" "mkdir -p /usr/share/a21s-shell"
tar -C "${REPO_ROOT}/shell" -cf - . \
  | ssh "${TARGET}" "tar -C /usr/share/a21s-shell -xf -"

# --- deploy rootfs overlay (merge into /) --------------------------------
echo ">> rootfs-overlay/ -> /"
tar -C "${REPO_ROOT}/rootfs-overlay" -cf - . \
  | ssh "${TARGET}" "tar -C / -xf -"

# --- fix ownership / permissions remotely --------------------------------
echo ">> Fixing permissions + daemon-reload"
ssh "${TARGET}" "
  set -e
  chown -R root:root /usr/share/a21s-shell
  chown root:root /etc/systemd/system/a21s-phone-session.service
  chmod 644 /etc/systemd/system/a21s-phone-session.service
  chown -R root:root /usr/local/bin/a21s-phone-session /usr/local/bin/a21s-dialer \
    /usr/local/bin/a21s-sms /usr/local/bin/a21s-settings /usr/local/bin/a21s-steam
  chmod 755 /usr/local/bin/a21s-phone-session /usr/local/bin/a21s-dialer \
    /usr/local/bin/a21s-sms /usr/local/bin/a21s-settings /usr/local/bin/a21s-steam
  chmod 644 /usr/share/applications/a21s-dialer.desktop \
    /usr/share/applications/a21s-sms.desktop \
    /usr/share/applications/a21s-settings.desktop \
    /usr/share/applications/steam.desktop
  chown -R gamer:gamer /home/gamer/.config/labwc
  systemctl daemon-reload
"

cat <<'EOF'

>> Deploy complete. On the device:

  1. Install packages (or run './scripts/deploy_shell.sh --packages'):
       pacman -Sy --needed labwc quickshell qt6-declarative qt6-svg \
         qt6-wayland xorg-xwayland gamescope squeekboard noto-fonts \
         ttf-nerd-fonts-symbols upower brightnessctl

  2. Ensure seat access (once):
       usermod -aG seat,input,video,render gamer

  3. Switch sessions (both stay installed; Conflicts= keeps them exclusive):
       systemctl disable steamos-session.service
       systemctl enable --now a21s-phone-session.service

     Revert to SteamOS session any time:
       systemctl disable a21s-phone-session.service
       systemctl enable --now steamos-session.service

  4. Logs:
       journalctl -u a21s-phone-session.service -b -f

EOF
