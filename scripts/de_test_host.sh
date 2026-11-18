#!/usr/bin/env bash
# de_test_host.sh — M0 host gate: unit tests + golden check.
set -euo pipefail
REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
DE_DIR="$REPO_ROOT/tools/rabbiz-de"

# Plan §10: on macOS the smithay/Linux crates cannot build (libudev, libseat,
# gbm). Only the platform-neutral crates are host-testable; the full workspace
# is tested in the arm64 container.
HOST_CRATES=(-p de-api -p de-design -p de-paint -p de-ui -p de-apps -p de-config -p de-sim)
case "$(uname -s)" in
  Darwin) cargo test --manifest-path "$DE_DIR/Cargo.toml" "${HOST_CRATES[@]}" ;;
  *)      cargo test --manifest-path "$DE_DIR/Cargo.toml" --workspace ;;
esac
cargo build --manifest-path "$DE_DIR/Cargo.toml" -p de-sim
"$DE_DIR/target/debug/de-sim" --scene all --check --golden-dir "$DE_DIR/tests/golden"
echo "host gate: PASS"
