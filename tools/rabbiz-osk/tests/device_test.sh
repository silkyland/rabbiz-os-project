#!/bin/bash
# Device-side test harness for rabbiz-osk (root). Not part of the image; /tmp only.
set -u
GAMER_UID=$(id -u gamer)
GAMER_ENV="XDG_RUNTIME_DIR=/run/user/$GAMER_UID WAYLAND_DISPLAY=wayland-0 DBUS_SESSION_BUS_ADDRESS=unix:path=/run/user/$GAMER_UID/bus"
as_gamer() { runuser -u gamer -- env $GAMER_ENV "$@"; }

start() {
  [ -f /tmp/osk-kb.pid ] && kill "$(cat /tmp/osk-kb.pid)" 2>/dev/null
  [ -f /tmp/osk-client.pid ] && kill "$(cat /tmp/osk-client.pid)" 2>/dev/null
  pkill -x squeekboard 2>/dev/null
  # Only one input method may own the seat: wait for squeekboard to really be gone.
  for _ in $(seq 1 50); do pgrep -x squeekboard >/dev/null || break; sleep 0.1; done
  if pgrep -x squeekboard >/dev/null; then echo "squeekboard still running"; fi
  as_gamer python3 /tmp/osk_test_client.py >/tmp/osk-client.log 2>&1 &
  echo $! >/tmp/osk-client.pid
  sleep 3
  as_gamer /usr/local/bin/rabbiz-osk >/tmp/osk-kb.log 2>&1 &
  echo $! >/tmp/osk-kb.pid
  sleep 3
  echo "--- keyboard log ---"
  cat /tmp/osk-kb.log
  echo "--- client log ---"
  cat /tmp/osk-client.log
}

shoot() {
  as_gamer grim -o DSI-1 "$1" && echo "shot $1"
}

tap() {
  as_gamer python3 /root/touch_swipe.py "$1" "$2" >/dev/null 2>&1
  sleep 0.4
}

stop() {
  [ -f /tmp/osk-kb.pid ] && kill "$(cat /tmp/osk-kb.pid)" 2>/dev/null
  [ -f /tmp/osk-client.pid ] && kill "$(cat /tmp/osk-client.pid)" 2>/dev/null
  pkill -f osk_test_client.py 2>/dev/null
  sleep 1
  # restore the shipped keyboard
  (as_gamer squeekboard >/dev/null 2>&1 &)
  sleep 2
  pgrep -a squeekboard | head -1
}

case "${1:-}" in
  start) start ;;
  shoot) shift; shoot "$@" ;;
  tap) shift; tap "$@" ;;
  stop) stop ;;
  *) echo "usage: $0 start|shoot <path>|tap <x> <y>|stop"; exit 2 ;;
esac
