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
  dolphin                # KDE file manager (launched by a21s-files)
  calindori              # Plasma Mobile calendar
  kclock                 # Plasma Mobile clock & alarms
  kalk                   # Plasma Mobile calculator & converter
  kweather               # Plasma Mobile weather
  krecorder              # Plasma Mobile voice recorder
  marknote               # KDE Kirigami markdown notes
  qmlkonsole             # Plasma Mobile touch terminal
  elisa                  # KDE Kirigami music player
  librespot              # Native ARM64 Spotify Connect receiver
  spotifyd               # Lightweight Spotify Connect daemon (MPRIS)
  spotify-player         # Fast native ARM64 Spotify TUI/CLI client
  playerctl              # MPRIS playback controller
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
COPYFILE_DISABLE=1 tar -C "${REPO_ROOT}/shell" -cf - . \
  | ssh "${TARGET}" "tar -C /usr/share/a21s-shell -xf -"

# --- deploy rootfs overlay (merge into /) --------------------------------
echo ">> rootfs-overlay/ -> /"
COPYFILE_DISABLE=1 tar -C "${REPO_ROOT}/rootfs-overlay" -cf - . \
  | ssh "${TARGET}" "tar -C / -xf -"

# --- fix ownership / permissions remotely --------------------------------
echo ">> Fixing permissions + daemon-reload"
ssh "${TARGET}" "
  set -e
  find /usr/share/a21s-shell /home/gamer/.config/labwc /home/gamer/.config/wireplumber \
    /usr/share/applications /usr/local/bin /etc/systemd/system \
    /etc/udev/rules.d /etc/modules-load.d /etc/systemd/logind.conf.d -maxdepth 3 -name '._*' -delete 2>/dev/null || true
  chown -R root:root /usr/share/a21s-shell
  chown root:root /etc/systemd/system/a21s-phone-session.service /etc/systemd/system/abox-audio.service /etc/systemd/system/a21s-modem-init.service
  chmod 644 /etc/systemd/system/a21s-phone-session.service /etc/systemd/system/abox-audio.service /etc/systemd/system/a21s-modem-init.service
  chown root:root /etc/udev/rules.d/90-a21s-backlight.rules /etc/udev/rules.d/99-a21s-modem-fp.rules /etc/systemd/logind.conf.d/10-a21s-powerkey.conf /etc/modules-load.d/a21s-hardware.conf
  chmod 644 /etc/udev/rules.d/90-a21s-backlight.rules /etc/udev/rules.d/99-a21s-modem-fp.rules /etc/systemd/logind.conf.d/10-a21s-powerkey.conf /etc/modules-load.d/a21s-hardware.conf
  chown root:root /usr/local/bin/a21s-*
  chmod 755 /usr/local/bin/a21s-*
  chown root:root /usr/share/applications/a21s-*.desktop /usr/share/applications/steam.desktop
  chmod 644 /usr/share/applications/a21s-*.desktop /usr/share/applications/steam.desktop
  chown -R gamer:gamer /home/gamer/.config
  chgrp video /sys/class/backlight/*/brightness /sys/class/backlight/*/bl_power 2>/dev/null || true
  chmod g+w /sys/class/backlight/*/brightness /sys/class/backlight/*/bl_power 2>/dev/null || true
  chgrp video /sys/devices/system/cpu/cpu*/cpufreq/scaling_governor 2>/dev/null || true
  chmod g+w /sys/devices/system/cpu/cpu*/cpufreq/scaling_governor 2>/dev/null || true
  chmod 666 /sys/kernel/a21s_fp/trigger_unlock /sys/kernel/a21s_fp/sleep_pin /dev/esfp0 2>/dev/null || true
  chgrp video /dev/umts_* 2>/dev/null || true
  chmod 660 /dev/umts_* 2>/dev/null || true
  chown gamer:gamer /run/user/1001/a21s-power.cmd 2>/dev/null || true
  systemctl daemon-reload
  systemctl enable a21s-modem-init.service 2>/dev/null || true
"

cat <<'EOF'

>> Deploy complete. On the device:

  1. Install packages (or run './scripts/deploy_shell.sh --packages'):
       pacman -Sy --needed labwc quickshell qt6-declarative qt6-svg \
         qt6-wayland dolphin calindori kclock kalk kweather krecorder \
         marknote qmlkonsole elisa xorg-xwayland gamescope squeekboard \
         noto-fonts ttf-nerd-fonts-symbols upower brightnessctl

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
