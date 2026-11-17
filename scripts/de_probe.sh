#!/usr/bin/env bash
# de_probe.sh — run `rabbiz-de probe --json` on the A21s under the device lock.
#
# Usage:
#   ./scripts/de_probe.sh [user@host]    # default: $A21S_HOST or root@172.16.42.1
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
HOST="${1:-${A21S_HOST:-root@172.16.42.1}}"

"$REPO_ROOT/scripts/device_session.sh" "$HOST" <<'EOF'
set -euo pipefail

was_active=0
if systemctl is-active --quiet a21s-phone-session.service; then
  was_active=1
  systemctl stop a21s-phone-session.service
fi

restore() {
  if [ "$was_active" -eq 1 ]; then
    systemctl start a21s-phone-session.service || true
  fi
}
trap restore EXIT

GAMER_UID="$(id -u gamer)"
RUNTIME_DIR="/run/user/${GAMER_UID}"
mkdir -p "$RUNTIME_DIR"
chown gamer:gamer "$RUNTIME_DIR" 2>/dev/null || true

runuser -u gamer -- env \
  XDG_RUNTIME_DIR="$RUNTIME_DIR" \
  DBUS_SESSION_BUS_ADDRESS="unix:path=${RUNTIME_DIR}/bus" \
  LIBSEAT_BACKEND=seatd \
  /usr/local/bin/rabbiz-de probe --json
EOF
