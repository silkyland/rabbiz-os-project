#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
DE_DIR="$ROOT_DIR/tools/rabbiz-de"
IMAGE="${RABBIZ_DE_DOCKER_IMAGE:-rabbiz-de-build:arm64}"

if ! docker image inspect "$IMAGE" >/dev/null 2>&1; then
  docker build -t "$IMAGE" "$DE_DIR"
fi

TARGET_DIR_ARG=()
if [[ -n "${RABBIZ_DE_TARGET_DIR:-}" ]]; then
  TARGET_DIR_ARG=(-e "CARGO_TARGET_DIR=$RABBIZ_DE_TARGET_DIR")
fi

docker run --rm \
  -v "$ROOT_DIR":/w \
  -v rabbiz-de-cargo-cache:/usr/local/cargo/registry \
  -w /w/tools/rabbiz-de \
  "${TARGET_DIR_ARG[@]}" \
  "$IMAGE" \
  /usr/local/cargo/bin/cargo build --release -p de-bin "$@"
