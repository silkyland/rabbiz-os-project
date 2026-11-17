#!/usr/bin/env bash
# de_deploy.sh — install rabbiz-de + session wrapper + unit + power-key on
# the A21s (M1-T6).
#
#   ./scripts/de_deploy.sh [user@host] [path/to/rabbiz-de]
#
# Installs:
#   /usr/local/bin/rabbiz-de            (binary)
#   /usr/local/bin/a21s-session         (mode selector wrapper)
#   /etc/systemd/system/a21s-phone-session.service  (ExecStart→a21s-session)
#   /usr/local/bin/a21s-power-key       (rabbiz-de detection)
# then daemon-reloads and verifies the unit's ExecStart. The DE stays
# opt-in: no /etc/rabbiz-de/enabled marker is written here — enable it
# with `a21s-session --de`, a user mode file, or the root marker.
# Runs a post-copy `probe --json` sanity check under the device lock and
# restores the session state.
set -euo pipefail
REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
HOST="${1:-root@172.16.42.1}"
BIN="${2:-$REPO_ROOT/tools/rabbiz-de/target/release/rabbiz-de}"
OVL="$REPO_ROOT/rootfs-overlay"

[[ -f "$BIN" ]] || { echo "no binary at $BIN — build first (de_build.sh --release)" >&2; exit 2; }
for f in "$OVL/usr/local/bin/a21s-session" \
         "$OVL/usr/local/bin/a21s-power-key" \
         "$OVL/etc/systemd/system/a21s-phone-session.service"; do
    [[ -f "$f" ]] || { echo "missing $f" >&2; exit 2; }
done

scp -q "$BIN" "$HOST:/tmp/rabbiz-de.new"
scp -q "$OVL/usr/local/bin/a21s-session" "$HOST:/tmp/a21s-session.new"
scp -q "$OVL/usr/local/bin/a21s-power-key" "$HOST:/tmp/a21s-power-key.new"
scp -q "$OVL/etc/systemd/system/a21s-phone-session.service" \
    "$HOST:/tmp/a21s-phone-session.service.new"

REMOTE_SCRIPT=$(mktemp)
trap 'rm -f "$REMOTE_SCRIPT"' EXIT
cat > "$REMOTE_SCRIPT" <<'REMOTE'
set -e
install -m 0755 -o root -g root /tmp/rabbiz-de.new /usr/local/bin/rabbiz-de
install -m 0755 -o root -g root /tmp/a21s-session.new /usr/local/bin/a21s-session
install -m 0755 -o root -g root /tmp/a21s-power-key.new /usr/local/bin/a21s-power-key
install -m 0644 -o root -g root /tmp/a21s-phone-session.service.new \
  /etc/systemd/system/a21s-phone-session.service
rm -f /tmp/rabbiz-de.new /tmp/a21s-session.new /tmp/a21s-power-key.new \
      /tmp/a21s-phone-session.service.new

systemctl daemon-reload

# Verify the unit really starts the selector wrapper.
es=$(systemctl cat a21s-phone-session.service | awk '/^ExecStart=/{print $1}')
[ "$es" = "ExecStart=/usr/local/bin/a21s-session" ] || {
  echo "ExecStart mismatch: $es" >&2; exit 1; }

# Probe sanity check — skipped when DE_DEPLOY_SKIP_PROBE=1 (expanded on the
# host before this script is sent): stopping the running session then
# committing can hit the Exynos cold-enable DSI wedge (docs/HANDOFF.md §0);
# probe is proven separately by de_probe.sh.
if [ "__SKIP_PROBE__" != "1" ]; then
  unit=a21s-phone-session.service
  was_active=$(systemctl is-active "$unit" || true)
  [ "$was_active" = "active" ] && systemctl stop "$unit"
  trap '[ "$was_active" = "active" ] && systemctl start "$unit" || true' EXIT
  sleep 1
  runuser -u gamer -- env LIBSEAT_BACKEND=seatd \
    XDG_RUNTIME_DIR=/run/user/$(id -u gamer) \
    /usr/local/bin/rabbiz-de probe --json
fi
REMOTE

sed -i "s/__SKIP_PROBE__/${DE_DEPLOY_SKIP_PROBE:-0}/g" "$REMOTE_SCRIPT"
"$REPO_ROOT/scripts/device_session.sh" "$HOST" < "$REMOTE_SCRIPT"

echo "de_deploy: installed; DE remains opt-in (a21s-session default = labwc)"
