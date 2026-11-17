#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
DE_DIR="$ROOT_DIR/tools/rabbiz-de"

cargo test --offline --manifest-path "$DE_DIR/Cargo.toml" \
  -p de-api -p de-design -p de-paint -p de-ui -p de-config -p de-apps -p de-sim

"$ROOT_DIR/scripts/de_golden.sh" --check
