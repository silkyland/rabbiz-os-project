#!/usr/bin/env bash
# de_build.sh — build rabbiz-de on the host (debug or release).
#
# Usage:
#   ./scripts/de_build.sh [--release] [--target <triple>]
#   default: host debug build of de-bin + de-sim
#
# Device (aarch64) build happens on-device or via the repo's cross toolchain;
# this script never touches /lib anywhere.
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
DE_DIR="$REPO_ROOT/tools/rabbiz-de"

profile="debug"
cargo_args=()
while [[ $# -gt 0 ]]; do
  case "$1" in
    --release) profile="release"; cargo_args+=(--release); shift ;;
    --target)  cargo_args+=(--target "$2"); shift 2 ;;
    *) echo "unknown arg: $1" >&2; exit 2 ;;
  esac
done

cargo build --manifest-path "$DE_DIR/Cargo.toml" -p de-bin -p de-sim "${cargo_args[@]}"
bin_dir="$DE_DIR/target/${profile}"
echo "built: $bin_dir/rabbiz-de  $bin_dir/de-sim"
