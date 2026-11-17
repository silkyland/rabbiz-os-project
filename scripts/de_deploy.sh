#!/usr/bin/env bash
# de_deploy.sh — deploy rabbiz-de binary, a21s-session, and a21s-power-key to the A21s.
#
# Usage:
#   ./scripts/de_deploy.sh [user@host] [--enable | --disable]
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
HOST="${A21S_HOST:-root@172.16.42.1}"
ENABLE_MODE=""

while [ $# -gt 0 ]; do
  case "$1" in
    --enable)
      ENABLE_MODE="enable"
      shift
      ;;
    --disable)
      ENABLE_MODE="disable"
      shift
      ;;
    -h|--help)
      echo "Usage: $0 [user@host] [--enable | --disable]" >&2
      exit 0
      ;;
    *@*)
      HOST="$1"
      shift
      ;;
    *)
      echo "Unknown argument: $1" >&2
      exit 2
      ;;
  esac
done

find_binary() {
  local candidates=()
  if [ -n "${RABBIZ_DE_BIN:-}" ]; then
    candidates+=("$RABBIZ_DE_BIN")
  fi
  local target_base="${RABBIZ_DE_TARGET_DIR:-$REPO_ROOT/tools/rabbiz-de/target}"
  candidates+=(
    "$target_base/aarch64-unknown-linux-gnu/release/rabbiz-de"
    "$target_base/release/rabbiz-de"
  )
  for c in "${candidates[@]}"; do
    if [ -x "$c" ]; then
      printf '%s\n' "$c"
      return 0
    fi
  done
  return 1
}

BIN_PATH="$(find_binary || true)"
if [ -z "$BIN_PATH" ]; then
  echo "de_deploy: rabbiz-de aarch64 binary not found; run scripts/de_build.sh or set RABBIZ_DE_BIN" >&2
  exit 1
fi

SESSION_SCRIPT="$REPO_ROOT/rootfs-overlay/usr/local/bin/a21s-session"
POWER_KEY_SCRIPT="$REPO_ROOT/rootfs-overlay/usr/local/bin/a21s-power-key"

for f in "$SESSION_SCRIPT" "$POWER_KEY_SCRIPT"; do
  if [ ! -f "$f" ]; then
    echo "de_deploy: missing $f" >&2
    exit 1
  fi
done

STAGE_DIR="/tmp/rabbiz-de-stage.$$"
ssh "$HOST" "mkdir -p '$STAGE_DIR'"
scp -q "$BIN_PATH" "$SESSION_SCRIPT" "$POWER_KEY_SCRIPT" "${HOST}:${STAGE_DIR}/"

"$REPO_ROOT/scripts/device_session.sh" "$HOST" <<EOF
set -euo pipefail

cleanup() {
  rm -rf "$STAGE_DIR"
}
trap cleanup EXIT

install -o root -g root -m 0755 "${STAGE_DIR}/rabbiz-de" /usr/local/bin/rabbiz-de
install -o root -g root -m 0755 "${STAGE_DIR}/a21s-session" /usr/local/bin/a21s-session
install -o root -g root -m 0755 "${STAGE_DIR}/a21s-power-key" /usr/local/bin/a21s-power-key

case "$ENABLE_MODE" in
  enable)
    mkdir -p /etc/rabbiz-de
    chown root:root /etc/rabbiz-de
    chmod 0755 /etc/rabbiz-de
    touch /etc/rabbiz-de/enabled
    chown root:root /etc/rabbiz-de/enabled
    chmod 0644 /etc/rabbiz-de/enabled
    ;;
  disable)
    rm -f /etc/rabbiz-de/enabled
    ;;
esac

systemctl daemon-reload
test -x /usr/local/bin/a21s-session
/usr/local/bin/rabbiz-de --help >/dev/null
echo "de_deploy: deployed and verified on \$(hostname)" >&2
EOF
