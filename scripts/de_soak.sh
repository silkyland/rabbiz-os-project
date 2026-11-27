#!/usr/bin/env bash
# de_soak.sh — M6-T2 30-minute burn-in on the A21s.
#
#   ./scripts/de_soak.sh [user@host] [duration_min]
#
# The running session is left untouched: this script samples the live
# rabbiz-de process every 30 s (RSS, CMA free, thermal zones, frame stats
# when --stats-file is active) and prints one JSON summary:
#   {"samples":N,"rss_growth_kb":X,"cma_growth_kb":Y,"temp_max_mc":Z,"pass":bool}
# M6 exit criteria: RSS growth < 2 MB and CMA growth < 2 MB over 30 min.
set -euo pipefail
REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
HOST="${1:-root@172.16.42.1}"
MINUTES="${2:-30}"

"$REPO_ROOT/scripts/device_session.sh" "$HOST" <<REMOTE
set -e
python3 - "$MINUTES" <<'PY'
import json, subprocess, sys, time

minutes = int(sys.argv[1])
interval = 30
samples = minutes * 60 // interval

def read_kb(path):
    try:
        return int(open(path).read().strip())
    except Exception:
        return None

def rabbiz_de_pid():
    out = subprocess.run(["pgrep", "-x", "rabbiz-de"], capture_output=True, text=True)
    pids = out.stdout.split()
    return int(pids[0]) if pids else None

def rss_kb(pid):
    try:
        for line in open(f"/proc/{pid}/status"):
            if line.startswith("VmRSS:"):
                return int(line.split()[1])
    except Exception:
        pass
    return None

def cma_free_kb():
    try:
        for line in open("/proc/meminfo"):
            if line.startswith("CmaFree:"):
                return int(line.split()[1])
    except Exception:
        pass
    return None

def temp_max_mc():
    import glob
    vals = []
    for p in glob.glob("/sys/class/thermal/thermal_zone*/temp"):
        v = read_kb(p)
        if v is not None:
            vals.append(v)
    return max(vals) if vals else None

first_rss = first_cma = None
last_rss = last_cma = None
tmax = 0
taken = 0
missing_pid = 0
for i in range(samples):
    pid = rabbiz_de_pid()
    if pid is None:
        missing_pid += 1
    else:
        r = rss_kb(pid)
        c = cma_free_kb()
        if first_rss is None and r is not None:
            first_rss = r
        if first_cma is None and c is not None:
            first_cma = c
        if r is not None:
            last_rss = r
        if c is not None:
            last_cma = c
    t = temp_max_mc()
    if t is not None:
        tmax = max(tmax, t)
    taken += 1
    if i + 1 < samples:
        time.sleep(interval)

rss_growth = (last_rss - first_rss) if first_rss is not None and last_rss is not None else None
cma_growth = (first_cma - last_cma) if first_cma is not None and last_cma is not None else None
ok = (
    missing_pid == 0
    and rss_growth is not None and rss_growth < 2048
    and cma_growth is not None and cma_growth < 2048
)
print(json.dumps({
    "samples": taken,
    "minutes": minutes,
    "rss_growth_kb": rss_growth,
    "cma_growth_kb": cma_growth,
    "temp_max_mc": tmax,
    "missing_pid_samples": missing_pid,
    "pass": bool(ok),
}))
sys.exit(0 if ok else 1)
PY
REMOTE
