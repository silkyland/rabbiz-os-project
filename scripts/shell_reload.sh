#!/usr/bin/env bash
# shell_reload.sh — fast dev loop: push shell/ only, restart the shell's quickshell as gamer,
# print its log and save a screenshot to <out> (default /tmp/a21s-shot.png).
# Usage: ./scripts/shell_reload.sh [user@host] [out.png]
set -euo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")/.."
HOST="${1:-root@172.16.42.1}"; OUT="${2:-/tmp/a21s-shot.png}"
COPYFILE_DISABLE=1 tar -C shell --exclude tests -cf - . | ssh "$HOST" "tar -C /usr/share/a21s-shell --no-same-owner -xf - 2>/dev/null; chown -R root:root /usr/share/a21s-shell"
ssh "$HOST" 'bash -s' <<'REMOTE'
set -e
GU=$(id -u gamer)
LW=$(pgrep -x labwc | head -1)
WD=$(ls -t /run/user/$GU/wayland-[0-9]* 2>/dev/null | grep -v "\.lock$" | head -1 | xargs -r basename)  # newest socket = the live labwc
ENVF="XDG_RUNTIME_DIR=/run/user/$GU WAYLAND_DISPLAY=${WD:-wayland-0} $(tr '\0' '\n' < /proc/$LW/environ | grep -E '^(XDG_|WAYLAND_DISPLAY|DBUS_|QT_|QML|LIBSEAT|MOZ_|SDL_|LANG)' | tr '\n' ' ')"
pkill -f 'quickshell -p /usr/share/a21s-shell/shell.qml' || true
sleep 1
runuser -u gamer -- env $ENVF setsid a21s-shell >/tmp/shell.log 2>&1 </dev/null &
sleep 6
pgrep -af 'quickshell -p /usr/share/a21s-shell/shell.qml' | head -1 || echo "SHELL NOT RUNNING"
echo "--- errors/warnings:"; grep -iE "error|warn|fail|not (a )?type|unavailable|undefined" /tmp/shell.log | sort | uniq -c | sort -rn | head -15 || true
timeout 10 runuser -u gamer -- env $ENVF grim /tmp/a21s-shot.png
REMOTE
scp -q "$HOST:/tmp/a21s-shot.png" "$OUT" && echo "screenshot: $OUT"
