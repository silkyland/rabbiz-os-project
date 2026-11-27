#!/usr/bin/env bash
# install_chrome_arm.sh — install official Google Chrome ARM64 on the A21s.
#
# Google ships a real aarch64 Linux build at dl.google.com (Debian .deb).
# This script downloads it ON THE DEVICE and extracts with --no-same-owner:
# a plain `tar -x -C /` applies the deb's './' entry uid/mode to / and
# transiently breaks every non-root exec (runuser/su/libc load).
#
# Usage:
#   CHROME_DEB_SHA256=<64-hex-sha256> ./scripts/install_chrome_arm.sh [user@host]
#   (default host: root@172.16.42.1)
#
# Result: /opt/google/chrome + /usr/bin/google-chrome-stable + the deb's own
# google-chrome.desktop; session integration comes from rootfs-overlay's
# rabbiz-browser wrapper/.desktop (--disable-gpu is mandatory on exynos-drm).

set -euo pipefail
TARGET="${1:-root@172.16.42.1}"
DEB_URL="${CHROME_DEB_URL:-https://dl.google.com/linux/direct/google-chrome-stable_current_arm64.deb}"
DEB_SHA256="${CHROME_DEB_SHA256:-}"

if [[ ! "$DEB_SHA256" =~ ^[0-9a-fA-F]{64}$ ]]; then
  echo "ERROR: CHROME_DEB_SHA256 must be set to the expected 64-char SHA-256 hex digest of the Chrome .deb" >&2
  echo "       Obtain the verified digest for ${DEB_URL} and re-run:" >&2
  echo "         CHROME_DEB_SHA256=<sha256> $0 ${TARGET}" >&2
  exit 2
fi

echo ">> Installing Google Chrome ARM64 on ${TARGET}"
ssh "${TARGET}" 'bash -se' -- "${DEB_URL}" "${DEB_SHA256}" <<'REMOTE'
set -euo pipefail
deb_url="$1"
deb_sha256="$2"
command -v bsdtar >/dev/null || pacman -S --needed --noconfirm libarchive
command -v curl >/dev/null || pacman -S --needed --noconfirm curl

umask 077
stage="$(mktemp -d /root/.deploy.XXXXXX)"
trap 'rm -rf -- "$stage"' EXIT

curl -fsSL --max-time 300 -o "$stage/chrome-arm64.deb" "$deb_url"
echo "${deb_sha256}  ${stage}/chrome-arm64.deb" | sha256sum -c -

mkdir -p "$stage/extract"
bsdtar -C "$stage/extract" -xf "$stage/chrome-arm64.deb" data.tar.xz
tar -xJf "$stage/extract/data.tar.xz" --no-same-owner --no-same-permissions --no-overwrite-dir -C /
ln -sf /opt/google/chrome/google-chrome /usr/local/bin/google-chrome
rm -f /etc/cron.daily/google-chrome 2>/dev/null || true
echo "chrome version: $(/opt/google/chrome/chrome --version 2>/dev/null || echo unknown)"
REMOTE
echo ">> Done. Launch via 'rabbiz-browser' or the 'Browser' icon."
