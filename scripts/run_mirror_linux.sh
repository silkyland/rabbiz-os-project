#!/usr/bin/env bash
# Build and launch Rabbiz Mirror (Linux GTK4 client).
#   ./scripts/run_mirror_linux.sh [--host 172.16.42.1] [--no-autoconnect]
set -euo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")/.."

MANIFEST="tools/rabbiz-mirror/linux/Cargo.toml"
BIN="tools/rabbiz-mirror/linux/target/release/rabbiz-mirror"

if [ ! -x "$BIN" ] || [ -n "$(find tools/rabbiz-mirror/linux/src tools/rabbiz-mirror/linux/Cargo.toml -newer "$BIN" 2>/dev/null)" ]; then
    cargo build --release --manifest-path "$MANIFEST"
fi

exec "$BIN" "$@"
