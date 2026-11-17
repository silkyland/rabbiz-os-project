#!/usr/bin/env bash
# frame_time.sh — measure shell frame times on the device (spike S5).
#
# Qt's scene-graph profiler prints one line per rendered frame when QSG_RENDER_TIMING=1.
# This starts the shell with that env, drives a scripted interaction (default: open and
# close the app drawer), then reports the frame-time distribution from /tmp/shell.log.
#
# Usage: ./scripts/frame_time.sh [user@host] [--gesture drawer|shade|none] [seconds]
#
# Notes:
# - the shell must be restarted with the env var, so this script restarts quickshell;
# - it injects touch through a21s-touch, which needs root (/dev/uinput);
# - "Frame rendered ... in X ms" numbers depend on the active backend: compare before/after
#   a change within one backend; they are not measured display FPS.
set -euo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")/.."

HOST="${1:-root@172.16.42.1}"
GESTURE="drawer"
SECONDS_TO_RUN=6
shift $(( $# > 0 ? 1 : 0 )) || true
while [ $# -gt 0 ]; do
    case "$1" in
        --gesture) GESTURE="${2:-drawer}"; shift 2 ;;
        *) SECONDS_TO_RUN="$1"; shift ;;
    esac
done

echo "== restarting the shell with QSG_RENDER_TIMING=1 on $HOST"
ssh "$HOST" 'bash -s' <<'REMOTE'
set -e
GU=$(id -u gamer)
LW=$(pgrep -x labwc | head -1)
WD=$(ls -t /run/user/$GU/wayland-[0-9]* 2>/dev/null | grep -v "\.lock$" | head -1 | xargs -r basename)
ENVF="XDG_RUNTIME_DIR=/run/user/$GU WAYLAND_DISPLAY=${WD:-wayland-0} $(tr '\0' '\n' < /proc/$LW/environ | grep -E '^(XDG_|WAYLAND_DISPLAY|DBUS_|QML|LIBSEAT|MOZ_|LANG)' | tr '\n' ' ')"
pkill -f 'quickshell -p /usr/share/a21s-shell/shell.qml' || true
sleep 1
runuser -u gamer -- env $ENVF QT_QPA_PLATFORMTHEME= QT_QUICK_CONTROLS_STYLE=Basic QSG_RENDER_TIMING=1 \
    setsid a21s-shell >/tmp/shell.log 2>&1 </dev/null &
sleep 6
REMOTE

case "$GESTURE" in
    drawer)
        # swipe up over the search pill: opens the drawer (HomeScreen drag handler)
        echo "== driving: drawer open/close"
        ssh "$HOST" 'a21s-touch 360 1370 360 900 260; sleep 1.2; a21s-touch 360 900 360 1370 260' || true
        ;;
    shade)
        echo "== driving: shade open/close"
        ssh "$HOST" 'a21s-touch 360 40 360 700 260; sleep 1.2; a21s-touch 360 700 360 40 260' || true
        ;;
    none) echo "== idle (no gesture)" ;;
esac
sleep "$SECONDS_TO_RUN"

ssh "$HOST" 'grep -aiE "frame rendered in" /tmp/shell.log | tail -600' > /tmp/frames.raw || true
COUNT=$(wc -l < /tmp/frames.raw | tr -d ' ')
if [ "$COUNT" -eq 0 ]; then
    echo "No frame timings captured. Check that QSG_RENDER_TIMING prints on this Qt build:"
    ssh "$HOST" 'grep -aiE "sgsg|render loop|Frame" /tmp/shell.log | head -5'
    exit 1
fi
python3 - "$COUNT" <<'PY'
import re, sys
vals = []
for line in open("/tmp/frames.raw"):
    m = re.search(r"frame rendered in ([0-9.]+) ?ms", line)
    if m:
        vals.append(float(m.group(1)))
vals.sort()
def pct(p):
    return vals[min(len(vals) - 1, int(len(vals) * p))]
if not vals:
    print("no values parsed"); sys.exit(1)
print(f"frames={len(vals)} p50={pct(0.5):.1f}ms p90={pct(0.9):.1f}ms "
      f"p99={pct(0.99):.1f}ms max={vals[-1]:.1f}ms  (~{1000/pct(0.5):.1f} fps at p50)")
print("worst 5:", ", ".join(f"{v:.1f}" for v in vals[-5:]))
PY
echo "raw lines: /tmp/frames.raw"
