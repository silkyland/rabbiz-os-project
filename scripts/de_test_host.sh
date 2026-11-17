#!/usr/bin/env bash
# Host checks for the `rabbiz-de` scaffold. No device, no DRM, no Python deps.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
MANIFEST="$ROOT/tools/rabbiz-de/Cargo.toml"
OUT="$ROOT/build/de/brand.png"

cargo fmt --manifest-path "$MANIFEST" --all --check
cargo clippy --manifest-path "$MANIFEST" \
  -p de-api -p de-design -p de-paint -p de-ui -p de-sim --all-targets -- -D warnings
cargo test --manifest-path "$MANIFEST" -p de-api -p de-design -p de-paint -p de-ui -p de-sim

cargo run --quiet --manifest-path "$MANIFEST" -p de-sim -- --scene brand --out "$OUT"
python3 - "$OUT" <<'PY'
import struct, sys
path = sys.argv[1]
data = open(path, "rb").read(24)
assert data[:8] == b"\x89PNG\r\n\x1a\n", "not a PNG"
w, h = struct.unpack(">II", data[16:24])
assert (w, h) == (720, 1600), f"expected 720x1600, got {w}x{h}"
print(f"png ok: {w}x{h} {path}")
PY

python3 "$ROOT/tools/rabbiz-de/scripts/check_seams.py" --manifest-path "$MANIFEST"
echo "de host checks ok"
