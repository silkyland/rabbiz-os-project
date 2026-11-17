#!/usr/bin/env bash
# de_bench.sh — run `rabbiz-de` benchmark scenario on the A21s under the device lock.
#
# Usage:
#   ./scripts/de_bench.sh [user@host] [--scenario <name>] [--duration <sec>] [--de]
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
HOST="${A21S_HOST:-root@172.16.42.1}"
SCENARIO="launcher"
DURATION=10
USE_DE_MODE=0

while [ $# -gt 0 ]; do
  case "$1" in
    --scenario)
      SCENARIO="${2:?--scenario requires a value}"
      shift 2
      ;;
    --duration)
      DURATION="${2:?--duration requires a value}"
      shift 2
      ;;
    --de)
      USE_DE_MODE=1
      shift
      ;;
    -h|--help)
      echo "Usage: $0 [user@host] [--scenario <name>] [--duration <sec>] [--de]" >&2
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

SUMMARY_JSON="$("$REPO_ROOT/scripts/device_session.sh" "$HOST" <<EOF
set -euo pipefail

GAMER_UID="\$(id -u gamer)"
RUNTIME_DIR="/run/user/\${GAMER_UID}"
CONFIG_DIR="/home/gamer/.config/rabbiz-de"
MODE_FILE="\${CONFIG_DIR}/mode"

if [ "$USE_DE_MODE" -eq 1 ]; then
  had_mode=0
  prev_mode=""
  if [ -f "\$MODE_FILE" ]; then
    had_mode=1
    prev_mode="\$(cat "\$MODE_FILE")"
  fi
  restore_mode() {
    if [ "\$had_mode" -eq 1 ]; then
      printf '%s\n' "\$prev_mode" > "\$MODE_FILE"
      chown gamer:gamer "\$MODE_FILE" 2>/dev/null || true
    else
      rm -f "\$MODE_FILE"
    fi
    systemctl restart a21s-phone-session.service || true
  }
  trap restore_mode EXIT

  mkdir -p "\$CONFIG_DIR"
  printf 'de\n' > "\$MODE_FILE"
  chown -R gamer:gamer "\$CONFIG_DIR" 2>/dev/null || true
  systemctl restart a21s-phone-session.service
  sleep "$DURATION"
  if [ -s "\${RUNTIME_DIR}/rabbiz-de.summary.json" ]; then
    cat "\${RUNTIME_DIR}/rabbiz-de.summary.json"
  else
    echo "Missing summary at \${RUNTIME_DIR}/rabbiz-de.summary.json" >&2
    exit 1
  fi
else
  was_active=0
  if systemctl is-active --quiet a21s-phone-session.service; then
    was_active=1
    systemctl stop a21s-phone-session.service
  fi
  restore() {
    if [ "\$was_active" -eq 1 ]; then
      systemctl start a21s-phone-session.service || true
    fi
  }
  trap restore EXIT

  mkdir -p "\$RUNTIME_DIR"
  chown gamer:gamer "\$RUNTIME_DIR" 2>/dev/null || true

  runuser -u gamer -- env \
    XDG_RUNTIME_DIR="\$RUNTIME_DIR" \
    DBUS_SESSION_BUS_ADDRESS="unix:path=\${RUNTIME_DIR}/bus" \
    LIBSEAT_BACKEND=seatd \
    /usr/local/bin/rabbiz-de run --stats --duration "$DURATION" --scenario "$SCENARIO"
fi
EOF
)"

printf '%s\n' "$SUMMARY_JSON"

python3 - "$SUMMARY_JSON" <<'PY'
import json
import sys

raw = sys.argv[1].strip()
if not raw:
    print("de_bench: empty JSON summary", file=sys.stderr)
    sys.exit(1)

try:
    data = json.loads(raw)
except json.JSONDecodeError as exc:
    print(f"de_bench: invalid JSON summary: {exc}", file=sys.stderr)
    sys.exit(1)

if not isinstance(data, dict):
    print("de_bench: summary must be a JSON object", file=sys.stderr)
    sys.exit(1)

cpu = data.get("cpu_ms") or {}
p95 = cpu.get("p95")
missed = data.get("missed_frames", 0)

if isinstance(p95, (int, float)) and p95 > 16.6:
    print(f"de_bench: budget miss: cpu_ms.p95={p95} > 16.6", file=sys.stderr)
    sys.exit(1)

if isinstance(missed, int) and missed > 2:
    print(f"de_bench: budget miss: missed_frames={missed} > 2", file=sys.stderr)
    sys.exit(1)
PY
