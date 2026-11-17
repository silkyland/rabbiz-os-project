#!/usr/bin/env bash
# de_test_host.sh — M0 host gate: unit tests + golden check.
set -euo pipefail
REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
DE_DIR="$REPO_ROOT/tools/rabbiz-de"

cargo test --manifest-path "$DE_DIR/Cargo.toml" --workspace
"$DE_DIR/target/debug/de-sim" --scene all --check --golden-dir "$DE_DIR/tests/golden"
echo "host gate: PASS"
