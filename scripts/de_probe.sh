#!/usr/bin/env bash
# de_probe.sh — run `rabbiz-de probe --json` on the A21s.
#
# Holds the device lock, stops rabbiz-labwc-session (KMS master), probes as
# `gamer` through seatd, restores the unit in a trap. Exactly one JSON
# object on stdout; logs on stderr.
#
#   ./scripts/de_probe.sh [user@host] [--require=gl]
set -euo pipefail
REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
HOST="${1:-root@172.16.42.1}"
EXTRA="${2:-}"

"$REPO_ROOT/scripts/device_session.sh" "$HOST" <<REMOTE
unit=rabbiz-session.service
was_active=\$(systemctl is-active "\$unit" || true)
[ "\$was_active" = "active" ] && systemctl stop "\$unit"
trap '[ "\$was_active" = "active" ] && systemctl start "\$unit" || true' EXIT
sleep 1
runuser -u gamer -- env LIBSEAT_BACKEND=seatd \
  XDG_RUNTIME_DIR=/run/user/\$(id -u gamer) \
  /usr/local/bin/rabbiz-de probe --json $EXTRA
REMOTE
