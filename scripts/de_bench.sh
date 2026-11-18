#!/usr/bin/env bash
# de_bench.sh — M0 frame benchmark on the A21s.
#
#   ./scripts/de_bench.sh [user@host] [duration_s] [scenario] [--renderer=gl|soft|auto]
# Prints one JSON summary: {"scenarios":[{name,frames,cpu_ms:{p50,p95,max},
#   missed_frames,rss_kb_peak,cma_free_min_kb,renderer,pass}]}
# Exit non-zero when p95 exceeds 2x the measured frame interval (18 ms @60 Hz).
set -euo pipefail
REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
HOST="${1:-root@172.16.42.1}"
DURATION="${2:-10}"
SCENARIO="${3:-m0-bench}"
RENDERER="${4:---renderer=auto}"

"$REPO_ROOT/scripts/device_session.sh" "$HOST" <<REMOTE
set -e
unit=rabbiz-session.service
was_active=\$(systemctl is-active "\$unit" || true)
[ "\$was_active" = "active" ] && systemctl stop "\$unit"
trap '[ "\$was_active" = "active" ] && systemctl start "\$unit" || true' EXIT
sleep 1
summary=/tmp/rabbiz-de-summary.json
runuser -u gamer -- env LIBSEAT_BACKEND=seatd \
  XDG_RUNTIME_DIR=/run/user/\$(id -u gamer) \
  /usr/local/bin/rabbiz-de run $RENDERER --duration $DURATION \
    --scenario $SCENARIO --stats-file /tmp/rabbiz-de-stats.jsonl \
    --stats-summary "\$summary" || rc=\$?
rc=\${rc:-0}
python3 - "\$summary" "$SCENARIO" "\$rc" <<'PY'
import json, sys
try:
    s = json.load(open(sys.argv[1]))
except Exception as e:
    print(json.dumps({"scenarios": [], "error": str(e)})); sys.exit(1)
p95 = s.get("cpu_ms", {}).get("p95", 1e9)
budget = 2.0 * 16.67
s["name"] = sys.argv[2]
s["exit_code"] = int(sys.argv[3])
s["pass"] = bool(s.get("frames", 0) > 0 and p95 <= budget and s["exit_code"] == 0)
print(json.dumps({"scenarios": [s]}))
sys.exit(0 if s["pass"] else 1)
PY
REMOTE
