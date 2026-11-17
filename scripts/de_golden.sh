#!/usr/bin/env bash
# de_golden.sh — bless or check golden frames for rabbiz-de scenes.
#
#   ./scripts/de_golden.sh --bless   # regenerate tests/golden/*
#   ./scripts/de_golden.sh --check   # verify (default)
set -euo pipefail
REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
DE_DIR="$REPO_ROOT/tools/rabbiz-de"
GOLDEN_DIR="$DE_DIR/tests/golden"
mode="--check"
[[ "${1:-}" == "--bless" ]] && mode="--bless"

cargo build --manifest-path "$DE_DIR/Cargo.toml" -p de-sim >/dev/null
"$DE_DIR/target/debug/de-sim" --scene all "$mode" --golden-dir "$GOLDEN_DIR"
