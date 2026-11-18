#!/usr/bin/env bash
# de_golden.sh — bless or check golden frames for rabbiz-de scenes.
#
#   ./scripts/de_golden.sh --bless   # regenerate tests/golden/*
#   ./scripts/de_golden.sh --check   # verify (default)
#
# Golden matrix: every scene × {720x1600, 1080x2400} × {en, th}.
# File names: <scene>-<W>x<H>-<locale>.png
set -euo pipefail
REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
DE_DIR="$REPO_ROOT/tools/rabbiz-de"
GOLDEN_DIR="$DE_DIR/tests/golden"
mode="--check"
[[ "${1:-}" == "--bless" ]] && mode="--bless"

cargo build --manifest-path "$DE_DIR/Cargo.toml" -p de-sim >/dev/null
SIM="$DE_DIR/target/debug/de-sim"
fail=0
for size in 720x1600 1080x2400; do
  for loc in en th; do
    "$SIM" --scene all "$mode" --golden-dir "$GOLDEN_DIR" --size "$size" --locale "$loc" || fail=1
  done
done
exit $fail
