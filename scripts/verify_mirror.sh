#!/usr/bin/env bash
# verify_mirror.sh — prove the Mac client can decode the device's VNC stream.
#   ./scripts/verify_mirror.sh [/tmp/out.png]
# Needs wayvnc running on the device (`ssh root@172.16.42.1 a21s-mirror start`).
set -euo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")/.."

OUT=${1:-/tmp/a21s-mirror-verify.png}
HOST=${DEVICE:-172.16.42.1}

BIN="$(swift build --package-path tools/a21s-mirror -c release --show-bin-path)/A21sMirror"
[ -x "$BIN" ] || { echo "build first: ./scripts/build_mac_app.sh" >&2; exit 1; }

"$BIN" --selftest --host "$HOST" --out "$OUT" --frames 20 --timeout 25

WIDTH=$(sips -g pixelWidth "$OUT" | awk '/pixelWidth/ {print $2}')
HEIGHT=$(sips -g pixelHeight "$OUT" | awk '/pixelHeight/ {print $2}')
if [ "$WIDTH" != "720" ] || [ "$HEIGHT" != "1600" ]; then
    echo "unexpected framebuffer ${WIDTH}x${HEIGHT} (expected 720x1600)" >&2
    exit 1
fi
echo "mirror verified: ${WIDTH}x${HEIGHT} → $OUT"
