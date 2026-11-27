#!/usr/bin/env bash
# verify_mirror.sh — prove the Mac client can decode the device's VNC stream.
#   ./scripts/verify_mirror.sh [/tmp/out.png]
# Needs wayvnc running on the device (`ssh root@172.16.42.1 rabbiz-mirror start`).
set -euo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")/.."

OUT=${1:-/tmp/rabbiz-mirror-verify.png}
HOST=${DEVICE:-172.16.42.1}

if [ "$(uname -s)" = "Darwin" ]; then
    BIN="$(swift build --package-path tools/rabbiz-mirror -c release --show-bin-path)/RabbizMirror"
    [ -x "$BIN" ] || { echo "build first: ./scripts/build_mac_app.sh" >&2; exit 1; }
else
    BIN="tools/rabbiz-mirror/linux/target/release/rabbiz-mirror"
    [ -x "$BIN" ] || cargo build --release --manifest-path tools/rabbiz-mirror/linux/Cargo.toml
fi

"$BIN" --selftest --host "$HOST" --out "$OUT" --frames 20 --timeout 25

if command -v sips >/dev/null 2>&1; then
    WIDTH=$(sips -g pixelWidth "$OUT" | awk '/pixelWidth/ {print $2}')
    HEIGHT=$(sips -g pixelHeight "$OUT" | awk '/pixelHeight/ {print $2}')
else
    read -r WIDTH HEIGHT < <(python3 -c 'import sys; from PIL import Image; im = Image.open(sys.argv[1]); print(im.width, im.height)' "$OUT")
fi
if [ "$WIDTH" != "720" ] || [ "$HEIGHT" != "1600" ]; then
    echo "unexpected framebuffer ${WIDTH}x${HEIGHT} (expected 720x1600)" >&2
    exit 1
fi
echo "mirror verified: ${WIDTH}x${HEIGHT} → $OUT"
