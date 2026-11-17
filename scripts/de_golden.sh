#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
MANIFEST="$ROOT/tools/rabbiz-de/Cargo.toml"
GOLDEN_DIR="$ROOT/tools/rabbiz-de/tests/golden"

usage() {
    echo "Usage: $0 [--check|--bless]" >&2
}

MODE="${1:-}"
case "$MODE" in
    --check)
        exec cargo run --offline --manifest-path "$MANIFEST" -p de-sim -- \
            --scene all --check "$GOLDEN_DIR"
        ;;
    --bless)
        exec cargo run --offline --manifest-path "$MANIFEST" -p de-sim -- \
            --scene all --bless "$GOLDEN_DIR"
        ;;
    *)
        usage
        exit 2
        ;;
esac
