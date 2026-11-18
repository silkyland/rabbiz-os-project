#!/bin/bash
# Device-side test harness for rabbiz-osk (root). Not part of the image; /tmp only.
set -u
GAMER_UID=$(id -u gamer)
AS_GAMER=(runuser -u gamer -- env "XDG_RUNTIME_DIR=/run/user/$GAMER_UID"
  WAYLAND_DISPLAY=wayland-0 "DBUS_SESSION_BUS_ADDRESS=unix:path=/run/user/$GAMER_UID/bus")

start() {
  local client_script scratch
  client_script="$(dirname "$0")/osk_test_client.py"
  [ -r "$client_script" ] || { echo "missing test client: $client_script" >&2; return 1; }
  scratch=$(mktemp -d /tmp/rabbiz-osk-test.XXXXXXXX) || return
  echo "test state: $scratch"
  pkill -x squeekboard 2>/dev/null
  # Only one input method may own the seat: wait for squeekboard to really be gone.
  for _ in $(seq 1 50); do pgrep -x squeekboard >/dev/null || break; sleep 0.1; done
  if pgrep -x squeekboard >/dev/null; then echo "squeekboard still running"; fi
  "${AS_GAMER[@]}" python3 "$client_script" >"$scratch/client.log" 2>&1 &
  echo $! >"$scratch/client.pid"
  sleep 3
  "${AS_GAMER[@]}" /usr/local/bin/rabbiz-osk >"$scratch/keyboard.log" 2>&1 &
  echo $! >"$scratch/keyboard.pid"
  sleep 3
  echo "--- keyboard log ---"
  cat "$scratch/keyboard.log"
  echo "--- client log ---"
  cat "$scratch/client.log"
}

shoot() {
  "${AS_GAMER[@]}" grim -o DSI-1 "$1" && echo "shot $1"
}

tap() {
  "${AS_GAMER[@]}" python3 /root/touch_swipe.py "$1" "$2" >/dev/null 2>&1
  sleep 0.4
}

stop() {
  local scratch=${1:-}
  if [[ ! "$scratch" =~ ^/tmp/rabbiz-osk-test\.[[:alnum:]]{8}$ || ! -d "$scratch" || ! -O "$scratch" || -L "$scratch" ]]; then
    echo "pass the test state directory printed by start" >&2
    return 2
  fi
  [ -f "$scratch/keyboard.pid" ] && kill "$(cat "$scratch/keyboard.pid")" 2>/dev/null
  [ -f "$scratch/client.pid" ] && kill "$(cat "$scratch/client.pid")" 2>/dev/null
  rm -f "$scratch"/{keyboard,client}.{pid,log}
  rmdir "$scratch"
  sleep 1
  # restore the shipped keyboard
  ("${AS_GAMER[@]}" squeekboard >/dev/null 2>&1 &)
  sleep 2
  pgrep -a squeekboard | head -1
}

case "${1:-}" in
  start) start ;;
  shoot) shift; shoot "$@" ;;
  tap) shift; tap "$@" ;;
  stop) shift; stop "$@" ;;
  *) echo "usage: $0 start|shoot <path>|tap <x> <y>|stop <test-state-dir>"; exit 2 ;;
esac
