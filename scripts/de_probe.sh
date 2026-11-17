#!/usr/bin/env bash
# de_probe.sh — run `rabbiz-de probe` on the A21s through the serialized
# device session. Exactly one JSON object goes to stdout; logs stay on stderr.
#
#   ./scripts/de_probe.sh [user@host] [--require-gl]
set -euo pipefail
REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
HOST="${1:-root@172.16.42.1}"
EXTRA="${2:-}"

"$REPO_ROOT/scripts/device_session.sh" "$HOST" <<REMOTE
/usr/local/bin/rabbiz-de probe $EXTRA
REMOTE
