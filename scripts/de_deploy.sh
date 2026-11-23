#!/usr/bin/env bash
# de_deploy.sh — install rabbiz-de + session wrapper + unit + power-key on
# the A21s (M1-T6).
#
#   ./scripts/de_deploy.sh [user@host] [path/to/rabbiz-de]
#
# Installs:
#   /usr/local/bin/rabbiz-de            (binary)
#   /usr/local/bin/rabbiz-session         (mode selector wrapper)
#   /etc/systemd/system/rabbiz-session.service  (ExecStart→rabbiz-session)
#   /usr/local/bin/a21s-power-key       (rabbiz-de detection)
# then daemon-reloads and verifies the unit's ExecStart. The DE stays
# opt-in: no /etc/rabbiz-de/enabled marker is written here — enable it
# with `rabbiz-session --de`, a user mode file, or the root marker.
# Runs a post-copy `probe --json` sanity check under the device lock and
# restores the session state.
set -euo pipefail
REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
HOST="${1:-root@172.16.42.1}"
BIN="${2:-$REPO_ROOT/tools/rabbiz-de/target/release/rabbiz-de}"
OVL="$REPO_ROOT/rootfs-overlay"

[[ -f "$BIN" ]] || { echo "no binary at $BIN — build first (de_build.sh --release)" >&2; exit 2; }
for f in "$OVL/usr/local/bin/rabbiz-session" \
         "$OVL/usr/local/bin/a21s-power-key" \
         "$OVL/etc/systemd/system/rabbiz-session.service" \
         "$OVL/etc/systemd/system/rabbiz-auth-helper.service" \
         "$OVL/usr/share/xdg-desktop-portal/rabbiz-portals.conf" \
         "$OVL/etc/xdg/mimeapps.list" \
         "$OVL/etc/xdg/autostart/rabbiz-keyring.desktop"; do
    [[ -f "$f" ]] || { echo "missing $f" >&2; exit 2; }
done

scp -q "$BIN" "$HOST:/tmp/rabbiz-de.new"
HELPER_BIN="$(dirname "$BIN")/rabbiz-auth-helper"
if [[ -f "$HELPER_BIN" ]]; then
  scp -q "$HELPER_BIN" "$HOST:/tmp/rabbiz-auth-helper.new"
fi
scp -q "$OVL/usr/local/bin/rabbiz-session" "$HOST:/tmp/rabbiz-session.new"
scp -q "$OVL/usr/local/bin/a21s-power-key" "$HOST:/tmp/a21s-power-key.new"
scp -q "$OVL/etc/systemd/system/rabbiz-session.service" \
    "$HOST:/tmp/rabbiz-session.service.new"
scp -q "$OVL/etc/systemd/system/rabbiz-auth-helper.service" \
    "$HOST:/tmp/rabbiz-auth-helper.service.new"
scp -q "$OVL/usr/share/xdg-desktop-portal/rabbiz-portals.conf" \
    "$HOST:/tmp/rabbiz-portals.conf.new"
scp -q "$OVL/etc/xdg/mimeapps.list" "$HOST:/tmp/rabbiz-mimeapps.list.new"
scp -q "$OVL/etc/xdg/autostart/rabbiz-keyring.desktop" \
    "$HOST:/tmp/rabbiz-keyring.desktop.new"

REMOTE_SCRIPT=$(mktemp)
trap 'rm -f "$REMOTE_SCRIPT"' EXIT
cat > "$REMOTE_SCRIPT" <<'REMOTE'
set -e
install -m 0755 -o root -g root /tmp/rabbiz-de.new /usr/local/bin/rabbiz-de
if [ -f /tmp/rabbiz-auth-helper.new ]; then
  install -m 0755 -o root -g root /tmp/rabbiz-auth-helper.new /usr/local/bin/rabbiz-auth-helper
fi
install -m 0755 -o root -g root /tmp/rabbiz-session.new /usr/local/bin/rabbiz-session
install -m 0755 -o root -g root /tmp/a21s-power-key.new /usr/local/bin/a21s-power-key
install -m 0644 -o root -g root /tmp/rabbiz-session.service.new \
  /etc/systemd/system/rabbiz-session.service
install -m 0644 -o root -g root /tmp/rabbiz-auth-helper.service.new \
  /etc/systemd/system/rabbiz-auth-helper.service
mkdir -p /usr/share/xdg-desktop-portal /etc/xdg/autostart
install -m 0644 -o root -g root /tmp/rabbiz-portals.conf.new \
  /usr/share/xdg-desktop-portal/rabbiz-portals.conf
install -m 0644 -o root -g root /tmp/rabbiz-mimeapps.list.new /etc/xdg/mimeapps.list
install -m 0644 -o root -g root /tmp/rabbiz-keyring.desktop.new \
  /etc/xdg/autostart/rabbiz-keyring.desktop
rm -f /tmp/rabbiz-de.new /tmp/rabbiz-auth-helper.new /tmp/rabbiz-session.new \
      /tmp/a21s-power-key.new /tmp/rabbiz-session.service.new \
      /tmp/rabbiz-auth-helper.service.new /tmp/rabbiz-portals.conf.new \
      /tmp/rabbiz-mimeapps.list.new /tmp/rabbiz-keyring.desktop.new

systemctl daemon-reload

# Rename migration (a21s-* → rabbiz-*, 2026-11): hand over enablement and
# drop the superseded unit file — the new unit's [Install] Alias keeps the
# old name resolving for polkit and stray scripts.
if systemctl is-enabled a21s-phone-session.service >/dev/null 2>&1; then
  was_enabled=1
  systemctl disable a21s-phone-session.service
fi
rm -f /etc/systemd/system/a21s-phone-session.service
if [ "${was_enabled:-0}" = "1" ]; then
  systemctl enable rabbiz-session.service
fi
# Stale launchers — superseded by rabbiz-session / rabbiz-labwc-session.
rm -f /usr/local/bin/a21s-session /usr/local/bin/a21s-phone-session

# Verify the unit really starts the selector wrapper.
es=$(systemctl cat rabbiz-session.service | awk '/^ExecStart=/{print $1}')
[ "$es" = "ExecStart=/usr/local/bin/rabbiz-session" ] || {
  echo "ExecStart mismatch: $es" >&2; exit 1; }

# Probe sanity check — skipped when DE_DEPLOY_SKIP_PROBE=1 (expanded on the
# host before this script is sent): stopping the running session then
# committing can hit the Exynos cold-enable DSI wedge (devices/samsung/a21s/docs/HANDOFF.md §0);
# probe is proven separately by de_probe.sh.
if [ "__SKIP_PROBE__" != "1" ]; then
  unit=rabbiz-session.service
  was_active=$(systemctl is-active "$unit" || true)
  [ "$was_active" = "active" ] && systemctl stop "$unit"
  trap '[ "$was_active" = "active" ] && systemctl start "$unit" || true' EXIT
  sleep 1
  runuser -u rabbiz -- env LIBSEAT_BACKEND=seatd \
    XDG_RUNTIME_DIR=/run/user/$(id -u rabbiz) \
    /usr/local/bin/rabbiz-de probe --json
fi
REMOTE

sed -i.bak "s/__SKIP_PROBE__/${DE_DEPLOY_SKIP_PROBE:-0}/g" "$REMOTE_SCRIPT"
rm -f "$REMOTE_SCRIPT.bak"
"$REPO_ROOT/scripts/device_session.sh" "$HOST" < "$REMOTE_SCRIPT"

echo "de_deploy: installed; DE remains opt-in (rabbiz-session default = labwc)"
