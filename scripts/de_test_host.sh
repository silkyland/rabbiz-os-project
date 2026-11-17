#!/usr/bin/env bash
# Host checks for the `rabbiz-de` scaffold. No device, no DRM, no Python deps.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
MANIFEST="$ROOT/tools/rabbiz-de/Cargo.toml"

cargo fmt --manifest-path "$MANIFEST" --all --check
cargo clippy --manifest-path "$MANIFEST" \
  -p de-api -p de-design -p de-paint -p de-ui -p de-sim --all-targets -- -D warnings
cargo test --manifest-path "$MANIFEST" -p de-api -p de-design -p de-paint -p de-ui -p de-sim

for SCENE in brand lock; do
  OUT="$ROOT/build/de/$SCENE.png"
  cargo run --quiet --manifest-path "$MANIFEST" -p de-sim -- \
    --scene "$SCENE" --out "$OUT"
  python3 - "$OUT" "$SCENE" <<'PY'
import struct, sys
path, scene = sys.argv[1], sys.argv[2]
data = open(path, "rb").read(24)
assert data[:8] == b"\x89PNG\r\n\x1a\n", "not a PNG"
w, h = struct.unpack(">II", data[16:24])
assert (w, h) == (720, 1600), f"expected 720x1600, got {w}x{h}"
print(f"png ok: {scene} {w}x{h} {path}")
PY
done

python3 "$ROOT/tools/rabbiz-de/scripts/check_seams.py" --manifest-path "$MANIFEST"
echo "de host checks ok"
