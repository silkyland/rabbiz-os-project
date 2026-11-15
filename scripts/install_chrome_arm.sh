#!/usr/bin/env bash
# install_chrome_arm.sh — install official Google Chrome ARM64 on the A21s.
#
# Google ships a real aarch64 Linux build at dl.google.com (Debian .deb).
# This script downloads it ON THE DEVICE and extracts with --no-same-owner:
# a plain `tar -x -C /` applies the deb's './' entry uid/mode to / and
# transiently breaks every non-root exec (runuser/su/libc load).
#
# Usage:  ./scripts/install_chrome_arm.sh [user@host]   # default root@172.16.42.1
#
# Result: /opt/google/chrome + /usr/bin/google-chrome-stable + the deb's own
# google-chrome.desktop; session integration comes from rootfs-overlay's
# a21s-browser wrapper/.desktop (--disable-gpu is mandatory on exynos-drm).

set -euo pipefail
TARGET="${1:-root@172.16.42.1}"
DEB_URL="https://dl.google.com/linux/direct/google-chrome-stable_current_arm64.deb"

echo ">> Installing Google Chrome ARM64 on ${TARGET}"
ssh "${TARGET}" 'set -e
  command -v bsdtar >/dev/null || pacman -S --needed --noconfirm libarchive wget
  cd /tmp
  wget -q --show-progress -O chrome-arm64.deb "'"${DEB_URL}"'"
  mkdir -p /tmp/chrome-extract && cd /tmp/chrome-extract
  bsdtar -xf /tmp/chrome-arm64.deb data.tar.xz
  tar -xJf data.tar.xz --no-same-owner --no-same-permissions -C /
  ln -sf /opt/google/chrome/google-chrome /usr/local/bin/google-chrome
  rm -rf /tmp/chrome-extract /tmp/chrome-arm64.deb /etc/cron.daily/google-chrome 2>/dev/null || true
  echo "chrome version: $(/opt/google/chrome/chrome --version 2>/dev/null || echo unknown)"
'
echo ">> Done. Launch via 'a21s-browser' or the 'Browser' icon."
