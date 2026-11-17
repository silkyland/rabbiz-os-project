#!/usr/bin/env bash
# de_deploy.sh — install /usr/local/bin/rabbiz-de on the A21s (M0-T8).
#
#   ./scripts/de_deploy.sh [user@host] [path/to/rabbiz-de]
# M0 installs only the binary (embedded assets inside). Session wrapper,
# unit changes and the enable marker are M1-T6's job — labwc stays default.
# Runs a post-copy `probe --json` sanity check under the device lock and
# restores file ownership.
set -euo pipefail
REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
HOST="${1:-root@172.16.42.1}"
BIN="${2:-$REPO_ROOT/tools/rabbiz-de/target/release/rabbiz-de}"

[[ -f "$BIN" ]] || { echo "no binary at $BIN — build first (de_build.sh --release)" >&2; exit 2; }

scp -q "$BIN" "$HOST:/tmp/rabbiz-de.new"
"$REPO_ROOT/scripts/device_session.sh" "$HOST" <<'REMOTE'
set -e
install -m 0755 -o root -g root /tmp/rabbiz-de.new /usr/local/bin/rabbiz-de
rm -f /tmp/rabbiz-de.new
unit=a21s-phone-session.service
was_active=$(systemctl is-active "$unit" || true)
[ "$was_active" = "active" ] && systemctl stop "$unit"
trap '[ "$was_active" = "active" ] && systemctl start "$unit" || true' EXIT
sleep 1
runuser -u gamer -- env LIBSEAT_BACKEND=seatd \
  XDG_RUNTIME_DIR=/run/user/$(id -u gamer) \
  /usr/local/bin/rabbiz-de probe --json
REMOTE
