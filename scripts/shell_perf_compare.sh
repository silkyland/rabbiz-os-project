#!/usr/bin/env bash
# shell_perf_compare.sh — compare shell render cost, software vs GPU, on the A21s.
#
# Restarts the shell in each mode with QSG_RENDER_TIMING=1, drives the same scripted
# gestures (drawer open/close, drawer scroll, shade) N times and reports per-phase
# frame-render time plus quickshell/labwc CPU. Runs under the shared device lock
# (docs/DEVICE-QUEUE.md). Ends by restoring the normal GPU shell.
#
# Requires an unlocked home screen. "Frame rendered in X ms" is the shell's own render
# time, NOT display FPS and NOT compositor cost; labwc CPU is reported alongside.
#
# Usage: ./scripts/shell_perf_compare.sh [user@host] [repeats]
set -euo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")/.."
HOST="${1:-root@172.16.42.1}"
REPEATS="${2:-3}"
RAW="$(mktemp -t shell-perf.XXXXXX)"

./scripts/device_session.sh "$HOST" > "$RAW" <<REMOTE
set -u
REPEATS=$REPEATS
CLK=\$(getconf CLK_TCK)
GU=\$(id -u gamer)
LW=\$(pgrep -x labwc | head -1)
WD=\$(ls -t /run/user/\$GU/wayland-[0-9]* 2>/dev/null | grep -v '\.lock\$' | head -1 | xargs -r basename)
ENVF="XDG_RUNTIME_DIR=/run/user/\$GU WAYLAND_DISPLAY=\${WD:-wayland-0} \$(tr '\0' '\n' < /proc/\$LW/environ | grep -E '^(XDG_|WAYLAND_DISPLAY|DBUS_|QML|LIBSEAT|MOZ_|LANG)' | tr '\n' ' ')"
PAT='quickshell -p /usr/share/rabbiz-shell/shell.qml'
ticks() { awk '{print \$14+\$15}' /proc/\$1/stat; }

start_shell() {  # mode timing
  local mode=\$1 timing=\$2 extra="" t=""
  [ "\$mode" = software ] && extra="RABBIZ_SHELL_CMA_FLOOR_KB=999999999"
  [ "\$timing" = 1 ] && t="QSG_RENDER_TIMING=1 WAYLAND_DEBUG=client"
  pkill -f "\$PAT" || true
  sleep 2
  runuser -u gamer -- env \$ENVF QT_QPA_PLATFORMTHEME= QT_QUICK_CONTROLS_STYLE=Basic \$extra \$t \
    setsid rabbiz-shell >/tmp/perf-shell.log 2>&1 </dev/null &
  sleep 9
  QS=\$(pgrep -f "\$PAT" | head -1)
  echo "MODE \$mode backend=\$(tr '\0' '\n' </proc/\$QS/environ | grep -E '^QT_QUICK_BACKEND=' | head -1) cma_free_kb=\$(awk '/^CmaFree:/{print \$2}' /proc/meminfo) shell_pid=\$QS"
}

g_idle()   { sleep 5; }
g_drawer() { for i in \$(seq \$REPEATS); do a21s-touch 360 1370 360 900 260; sleep 1.3; a21s-touch 360 900 360 1370 260; sleep 1.3; done; }
g_scroll() { a21s-touch 360 1370 360 900 260; sleep 1.3
             for i in \$(seq \$REPEATS); do a21s-touch 360 1200 360 400 200; sleep 0.8; a21s-touch 360 400 360 1200 200; sleep 0.8; done
             a21s-touch 360 900 360 1370 260; sleep 1.3; }
g_shade()  { for i in \$(seq \$REPEATS); do a21s-touch 360 40 360 700 260; sleep 1.3; a21s-touch 360 700 360 40 260; sleep 1.3; done; }

phase() {  # mode name gesture-fn
  local mode=\$1 name=\$2 fn=\$3
  local l0 q0 w0 s e
  l0=\$(wc -l </tmp/perf-shell.log); q0=\$(ticks \$QS); w0=\$(ticks \$LW); s=\$(date +%s.%N)
  \$fn
  sleep 1
  e=\$(date +%s.%N)
  awk -v m=\$mode -v n=\$name -v clk=\$CLK -v s=\$s -v e=\$e -v q0=\$q0 -v q1=\$(ticks \$QS) -v w0=\$w0 -v w1=\$(ticks \$LW) \
    'BEGIN{w=e-s; printf "CPU %s %s qs=%.1f labwc=%.1f wall=%.1f\n", m, n, (q1-q0)/clk/w*100, (w1-w0)/clk/w*100, w}'
  # backend-independent: wl_surface.commit timestamps (ms) per surface, as the compositor sees them
  tail -n +\$((l0+1)) /tmp/perf-shell.log | awk -v m=\$mode -v n=\$name '/-> wl_surface#[0-9]+\.commit\(\)/ {
      split(substr(\$1, 2, 15), a, ":"); ts = (a[1] * 3600 + a[2] * 60 + a[3]) * 1000
      match(\$0, /wl_surface#[0-9]+/); print "COMMIT", m, n, substr(\$0, RSTART + 11, RLENGTH - 11), ts }'
  tail -n +\$((l0+1)) /tmp/perf-shell.log | grep -aiE 'frame rendered' | grep -oE 'in [0-9.]+ ?ms' | grep -oE '[0-9.]+' | sed "s/^/FRAME \$mode \$name /"
}

for mode in software gpu; do
  start_shell \$mode 1
  for p in idle drawer scroll shade; do phase \$mode \$p g_\$p; done
  echo "CMA_AFTER \$mode \$(awk '/^CmaFree:/{print \$2}' /proc/meminfo)"
done
echo "BG \$(pgrep -a chrome | wc -l) chrome procs; \$(pgrep -fa Notes.qml | wc -l) notes"
start_shell gpu 0   # restore the normal shell
REMOTE

python3 - "$RAW" <<'PY'
import sys, collections
commits = collections.defaultdict(lambda: collections.defaultdict(list))
frames = collections.defaultdict(list)
for line in open(sys.argv[1]):
    p = line.split()
    if not p: continue
    if p[0] == "COMMIT": commits[(p[1], p[2])][p[3]].append(float(p[4]))
    elif p[0] == "FRAME": frames[(p[1], p[2])].append(float(p[3]))
    elif p[0] in ("MODE", "CMA_AFTER", "BG", "CPU"): print(line.rstrip())
def pct(v, q): return v[min(len(v) - 1, int(len(v) * q))]
print()
print("Commit interval of the busiest shell surface per phase. Intervals >100 ms are idle gaps between")
print("gestures and are excluded. 60 Hz budget = 16.7 ms; 'slow' = interval > 25 ms (a missed vsync).")
print(f"{'mode':9}{'phase':8}{'surf':>6}{'active':>8}{'p50':>7}{'p90':>7}{'p99':>7}{'slow%':>7}{'~fps':>7}")
for (m, ph) in sorted(commits, key=lambda k: (k[0] != "software", k[1])):
    surf, ts = max(commits[(m, ph)].items(), key=lambda kv: len(kv[1]))
    d = sorted(b - a for a, b in zip(ts, ts[1:]) if 0 < b - a <= 100)
    if len(d) < 5:
        print(f"{m:9}{ph:8}{surf:>6}{len(d):8}  (too few active frames)")
        continue
    slow = 100 * sum(x > 25 for x in d) / len(d)
    print(f"{m:9}{ph:8}{surf:>6}{len(d):8}{pct(d,.5):7.1f}{pct(d,.9):7.1f}{pct(d,.99):7.1f}{slow:7.1f}{1000/(sum(d)/len(d)):7.1f}")
if frames:
    print("\nQt scene-graph 'frame rendered' (GPU only; includes vsync wait):")
    for (m, ph), v in sorted(frames.items()):
        v.sort()
        print(f"{m:9}{ph:8} n={len(v)} p50={pct(v,.5):.1f} p90={pct(v,.9):.1f} p99={pct(v,.99):.1f} max={v[-1]:.1f} ms")
PY
echo "raw: $RAW"
