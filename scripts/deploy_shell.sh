#!/usr/bin/env bash
# deploy_shell.sh — push rabbiz-shell + rootfs-overlay to the Galaxy A21s.
#
# Usage:
#   ./scripts/deploy_shell.sh [user@host]     # default root@172.16.42.1
#   ./scripts/deploy_shell.sh --packages      # only print the pacman line
#
# Layout (see shell/README.md):
#   shell/          -> /usr/share/rabbiz-shell/
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
  dolphin                # KDE file manager (phone UI via PLASMA_PLATFORM=phone)
  index-fm               # Maui Index — touch-first file manager (rabbiz-files prefers it)
  plasma-integration     # 'kde' Qt platformtheme: kdeglobals colors/fonts/single-click in Dolphin
  breeze                 # Breeze widget style + BreezeDark color scheme for KDE apps
  merkuro                # KDE calendar (replaces calindori — upstream successor, much better UI)
  gnome-calls            # dialer UI (ModemManager) — ready for M6
  chatty                 # SMS/MMS + Matrix (ModemManager) — ready for M6
  gnome-contacts         # contacts (EDS — shared with calls/chatty)
  plasma-settings        # Kirigami mobile settings panel
  maui-pix               # Maui touch gallery
  vvave                  # Maui touch music player
  buho                   # Maui touch notes/todo
  maui-station           # Maui touch terminal
  clapper                # adaptive GTK video player
  papers                 # libadwaita PDF/doc viewer (evince successor)
  loupe                  # libadwaita image viewer (pinch zoom)
  gnome-maps             # adaptive maps
  geary                  # adaptive email
  feedbackd              # haptic/sound/LED feedback — needed by calls/chatty alerts
  callaudiod             # call audio routing on PipeWire — needed by calls
  swappy                 # screenshot annotation (grim companion)
  squeekboard            # Wayland OSK; Thai layout ('th') + emoji built in
  gsettings-desktop-schemas # gsettings keys for OSK auto-show + GTK apps
  dconf                  # gsettings backend
  calindori              # Plasma Mobile calendar (kept as fallback; merkuro preferred)
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
  mpv                    # Media player for local/HTTP audio streams
  angelfish              # Plasma Mobile QtWebEngine browser (Spotify Web)
  grim                   # Wayland screenshot capture utility
  python-evdev           # volume-keys.py (hardware keys + screenshot combo)
  gcc                    # setup_qt_scanout.sh / setup_dumb_destroy_fix.sh compile their shims on the device
  xdg-utils              # xdg-open (screenshot toast -> default image app)
  koko                   # KDE Plasma Mobile image gallery
  xorg-xwayland          # XWayland for gamescope/steam and X11 clients
  squashfuse             # FUSE-mounts the FEX x86 guestos .sqsh (kernel lacks squashfs zstd)
  retroarch              # emulator frontend (GBA/NDS via libretro cores)
  libretro-mgba          # GBA core
  libretro-melonds       # NDS core (has ARM64 JIT)
  libretro-desmume       # NDS core (fallback)
  libretro-core-info     # core metadata for the RetroArch menu
  mgba-qt                # standalone touch-friendly GBA emulator
  noto-fonts
  ttf-nerd-fonts-symbols # icon glyphs used by the shell
  upower                 # battery service consumed by Rabbiz.Services
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
}

if [[ "${1:-}" == "--packages" || "${1:-}" == "-p" ]]; then
  print_packages
  exit 0
fi

TARGET="${1:-root@172.16.42.1}"

echo ">> Deploying rabbiz-shell to ${TARGET}"
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
echo ">> shell/ -> /usr/share/rabbiz-shell/"
ssh "${TARGET}" "mkdir -p /usr/share/rabbiz-shell"
COPYFILE_DISABLE=1 tar -C "${REPO_ROOT}/shell" -cf - . \
  | ssh "${TARGET}" "tar -C /usr/share/rabbiz-shell -xf -"

# --- prune files that were replaced by real Linux apps (tar never deletes) ---
echo ">> pruning replaced apps"
# calculator, clock and notes are in-house base apps again (apps/), so only the really removed
# wrappers are pruned here.
ssh "${TARGET}" "rm -rf /usr/share/rabbiz-shell/apps/{calculator,clock,notes,gallery,files,spotify} \
  /usr/share/rabbiz-shell/Rabbiz/Components \
  /usr/local/bin/a21s-{gallery,files,spotify} \
  /usr/share/applications/a21s-{gallery,files,spotify}.desktop"

# --- rename migration: a21s-* -> rabbiz-* (tar never deletes, so drop the
# superseded names explicitly; hardware-tied a21s-* stays) --------------
echo ">> dropping superseded a21s-* session/shell/app names"
ssh "${TARGET}" "
  rm -f /usr/local/bin/a21s-session /usr/local/bin/a21s-phone-session \
    /usr/local/bin/a21s-shell /usr/local/bin/a21s-splash \
    /usr/local/bin/a21s-spotify-ctl \
    /usr/local/bin/a21s-{browser,calculator,clock,dialer,gallery,music,notes,photos,settings,sms,terminal,files,calendar} \
    /usr/share/applications/a21s-*.desktop \
    /usr/share/applications/google-chrome.desktop \
    /usr/share/applications/com.google.Chrome.desktop \
    /etc/systemd/system/a21s-splash.service \
    /etc/polkit-1/rules.d/50-a21s-session.rules \
    /etc/polkit-1/rules.d/51-a21s-shell.rules
  find /usr/share/icons -name 'a21s-*.svg' -delete 2>/dev/null || true
  rm -rf /usr/share/a21s-shell /usr/share/a21s-apps /usr/share/a21s
"

# --- deploy the in-house base apps (docs/BASE-APPS.md) --------------------
echo ">> apps/ -> /usr/share/rabbiz-apps/"
ssh "${TARGET}" "mkdir -p /usr/share/rabbiz-apps"
COPYFILE_DISABLE=1 tar -C "${REPO_ROOT}/apps" -cf - . \
  | ssh "${TARGET}" "tar -C /usr/share/rabbiz-apps --no-same-owner -xf - && chmod -R a+rX /usr/share/rabbiz-apps"

# --- deploy rootfs overlay (merge into /) --------------------------------
echo ">> rootfs-overlay/ -> /"
COPYFILE_DISABLE=1 tar -C "${REPO_ROOT}/rootfs-overlay" -cf - . \
  | ssh "${TARGET}" "tar -C / -xf -"

# --- fix ownership / permissions remotely --------------------------------
echo ">> Fixing permissions + daemon-reload"
ssh "${TARGET}" "
  set -e
  find /usr/share/rabbiz-shell /home/rabbiz/.config/labwc /home/rabbiz/.config/wireplumber \
    /usr/share/applications /usr/local/bin /etc/systemd/system \
    /etc/udev/rules.d /etc/modules-load.d /etc/systemd/logind.conf.d -maxdepth 3 -name '._*' -delete 2>/dev/null || true
  chown -R root:root /usr/share/rabbiz-shell
  chown root:root /etc/systemd/system/a21s-*.service /etc/systemd/system/rabbiz-*.service /etc/systemd/system/abox-audio.service /etc/systemd/system/fex-guestos.service 2>/dev/null || true
  chmod 644 /etc/systemd/system/a21s-*.service /etc/systemd/system/rabbiz-*.service /etc/systemd/system/abox-audio.service /etc/systemd/system/fex-guestos.service 2>/dev/null || true
  systemctl enable a21s-display-pin.service 2>/dev/null || true
  systemctl enable a21s-drm-keeper.service 2>/dev/null || true
  chown root:root /etc/polkit-1/rules.d/50-rabbiz-session.rules /etc/polkit-1/rules.d/51-rabbiz-shell.rules 2>/dev/null || true
  chmod 644 /etc/polkit-1/rules.d/50-rabbiz-session.rules /etc/polkit-1/rules.d/51-rabbiz-shell.rules 2>/dev/null || true
  chown root:root /etc/udev/rules.d/90-a21s-backlight.rules /etc/udev/rules.d/99-a21s-modem-fp.rules /etc/udev/rules.d/99-hide-system-partitions.rules /etc/systemd/logind.conf.d/10-a21s-powerkey.conf /etc/modules-load.d/a21s-hardware.conf
  chmod 644 /etc/udev/rules.d/90-a21s-backlight.rules /etc/udev/rules.d/99-a21s-modem-fp.rules /etc/udev/rules.d/99-hide-system-partitions.rules /etc/systemd/logind.conf.d/10-a21s-powerkey.conf /etc/modules-load.d/a21s-hardware.conf
  chown root:root /usr/local/bin/a21s-* /usr/local/bin/rabbiz-* /usr/local/bin/a21s_holdmenu.py /usr/local/bin/volume-keys.py 2>/dev/null || true
  chmod 755 /usr/local/bin/a21s-* /usr/local/bin/rabbiz-* /usr/local/bin/a21s_holdmenu.py /usr/local/bin/volume-keys.py 2>/dev/null || true
  chown root:root /usr/share/applications/rabbiz-*.desktop 2>/dev/null || true
  chmod 644 /usr/share/applications/rabbiz-*.desktop 2>/dev/null || true
  # tar preserves the macOS uids baked into the archive (501/20), which
  # leaves /, /home and /home/rabbiz owned by a non-existent uid and breaks
  # runuser/su + systemd path-transition checks. Restore real owners.
  chown root:root / /home 2>/dev/null || true
  chown rabbiz:rabbiz /home/rabbiz /home/rabbiz/.local /home/rabbiz/.local/share 2>/dev/null || true
  chown -R rabbiz:rabbiz /home/rabbiz/.config
  # squeekboard user layouts override the built-ins; keep rabbiz-owned.
  chown -R rabbiz:rabbiz /home/rabbiz/.local/share/squeekboard 2>/dev/null || true
  chgrp video /sys/class/backlight/*/brightness /sys/class/backlight/*/bl_power 2>/dev/null || true
  chmod g+w /sys/class/backlight/*/brightness /sys/class/backlight/*/bl_power 2>/dev/null || true
  chgrp video /sys/devices/system/cpu/cpu*/cpufreq/scaling_governor 2>/dev/null || true
  chmod g+w /sys/devices/system/cpu/cpu*/cpufreq/scaling_governor 2>/dev/null || true
  chmod 666 /sys/kernel/a21s_fp/trigger_unlock /sys/kernel/a21s_fp/sleep_pin /dev/esfp0 2>/dev/null || true
  chgrp video /dev/umts_* 2>/dev/null || true
  chmod 660 /dev/umts_* 2>/dev/null || true
  chown rabbiz:rabbiz /run/user/\$(id -u rabbiz)/a21s-power.cmd 2>/dev/null || true
  systemctl daemon-reload
  # Unit rename migration: a21s-phone-session.service -> rabbiz-session.service.
  # Disable under the old name first (clears its wants symlink), then drop the
  # file; the new unit's [Install] Alias keeps the old name resolving.
  if systemctl is-enabled a21s-phone-session.service >/dev/null 2>&1; then
    systemctl disable a21s-phone-session.service
    rm -f /etc/systemd/system/a21s-phone-session.service
    systemctl enable rabbiz-session.service
  else
    rm -f /etc/systemd/system/a21s-phone-session.service
  fi
  # User config rename: ~/.config/a21s-shell -> ~/.config/rabbiz-shell
  # (session scripts repeat this; doing it here makes the first boot clean).
  if [ -d /home/rabbiz/.config/a21s-shell ] && [ ! -e /home/rabbiz/.config/rabbiz-shell ]; then
    mv /home/rabbiz/.config/a21s-shell /home/rabbiz/.config/rabbiz-shell
  fi
  systemctl enable a21s-modem-init.service 2>/dev/null || true
  # Boot splash + hardware volume/screenshot keys; no login prompt on the panel (getty@tty1 is a /dev/null symlink in the overlay).
  systemctl enable rabbiz-splash.service volume-keys.service 2>/dev/null || true
  sysctl -q --system 2>/dev/null || true
"

# --- GPU rendering support (built on the device from the checkout; both fail closed) ----------------
echo ">> Qt/Panfrost scanout path + compositor CMA-leak fix"
"${REPO_ROOT}/scripts/setup_qt_scanout.sh" "${TARGET}" || echo "   (a21s-qt-gpu unavailable: apps fall back to software rendering)"
"${REPO_ROOT}/scripts/setup_dumb_destroy_fix.sh" "${TARGET}" || echo "   (dumb-destroy.so not installed: GPU apps will leak CMA!)"


cat <<'EOF'

>> Deploy complete. On the device:

  1. Install packages (or run './scripts/deploy_shell.sh --packages'):
       pacman -Sy --needed labwc quickshell qt6-declarative qt6-svg \
         qt6-wayland dolphin calindori kclock kalk kweather krecorder \
         marknote qmlkonsole elisa xorg-xwayland squeekboard \
         noto-fonts ttf-nerd-fonts-symbols upower brightnessctl

  2. Ensure seat access (once):
       usermod -aG seat,input,video,render rabbiz

  3. Start the phone session:
       systemctl enable --now rabbiz-session.service

  4. Logs:
       journalctl -u rabbiz-session.service -b -f

EOF
