#!/usr/bin/env bash
# de_deploy.sh — push the rabbiz-de binary to the A21s.
#
#   ./scripts/de_deploy.sh [user@host] [path/to/rabbiz-de]
# Default host: root@172.16.42.1. Installs to /usr/local/bin/rabbiz-de
# (never touches /lib). labwc session stays untouched — the DE binary is
# inert until a21s-session selects it.
set -euo pipefail
REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
HOST="${1:-root@172.16.42.1}"
BIN="${2:-$REPO_ROOT/tools/rabbiz-de/target/release/rabbiz-de}"

[[ -f "$BIN" ]] || { echo "no binary at $BIN — build first (de_build.sh --release)" >&2; exit 2; }

scp -q "$BIN" "$HOST:/tmp/rabbiz-de.new"
"$REPO_ROOT/scripts/device_session.sh" "$HOST" <<'REMOTE'
install -m 0755 /tmp/rabbiz-de.new /usr/local/bin/rabbiz-de
rm -f /tmp/rabbiz-de.new
/usr/local/bin/rabbiz-de --help | head -1 || true
REMOTE
