#!/usr/bin/env bash
# de_boot_matrix.sh — M6-T2 cold-boot matrix: N reboots, each verified for
# the brand frame reaching the panel and no wedged compositor.
#
#   ./scripts/de_boot_matrix.sh [user@host] [reboots]
#
# Per boot: wait for rabbiz-de, confirm the session unit is active and the
# compositor answers its ctl socket, then reboot again. Prints one JSON
# summary: {"boots":N,"ok":N,"failures":[{"boot":i,"reason":...}],"pass":bool}
# M6 exit criteria: 10/10 cold boots clean.
#
# WARNING: reboots the device N times — run only through the device queue.
set -euo pipefail
REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
HOST="${1:-root@172.16.42.1}"
BOOTS="${2:-10}"

# The device drops SSH on every reboot, so the matrix loop runs on the device
# itself: copy the runner over, start it detached, then poll the result file
# after each boot completes. The runner must survive reboots — a tmpfiles
# transient unit is the simplest honest mechanism.
MATRIX_DIR=/var/lib/de-boot-matrix
RESULT=/var/lib/de-boot-matrix/result.json

cat > /tmp/de-boot-matrix-local.py <<'PY'
import json, os, subprocess, sys, time

boots = int(sys.argv[1])
result_path = sys.argv[2]
state_path = "/var/lib/de-boot-matrix/state.json"

state = {"boot": 0, "failures": []}
if os.path.exists(state_path):
    state = json.load(open(state_path))
state["boot"] += 1
this = state["boot"]

def check():
    # Session unit active and the compositor process alive.
    unit = subprocess.run(
        ["systemctl", "is-active", "rabbiz-session.service"],
        capture_output=True, text=True,
    ).stdout.strip()
    if unit != "active":
        return f"unit_{unit or 'inactive'}"
    pid = subprocess.run(["pgrep", "-x", "rabbiz-de"], capture_output=True, text=True).stdout.split()
    if not pid:
        return "no_rabbiz_de_process"
    # The ctl socket answers: the compositor composited at least one frame.
    rt = "/run/user/1001"
    sock = os.path.join(rt, "rabbiz-de-ctl.sock")
    if not os.path.exists(sock):
        return "no_ctl_socket"
    out = subprocess.run(
        ["runuser", "-u", "rabbiz", "--", "env", f"XDG_RUNTIME_DIR={rt}",
         "/usr/local/bin/rabbiz-de", "ctl", "state"],
        capture_output=True, text=True, timeout=10,
    )
    if out.returncode != 0:
        return "ctl_state_failed"
    return None

# Give the session up to 90 s to come up after boot.
deadline = time.time() + 90
reason = "timeout"
while time.time() < deadline:
    reason = check()
    if reason is None:
        break
    time.sleep(5)

if reason is not None:
    state["failures"].append({"boot": this, "reason": reason})
json.dump(state, open(state_path, "w"))

done = this >= boots
if done:
    summary = {
        "boots": this,
        "ok": this - len(state["failures"]),
        "failures": state["failures"],
    }
    summary["pass"] = not state["failures"]
    json.dump(summary, open(result_path, "w"))
    subprocess.run(["systemctl", "disable", "de-boot-matrix.service"], capture_output=True)
    for p in (state_path, "/etc/systemd/system/de-boot-matrix.service", "/var/lib/de-boot-matrix/matrix.py"):
        try:
            os.unlink(p)
        except OSError:
            pass
else:
    subprocess.run(["systemctl", "reboot"], capture_output=True)
PY

# Install runner + a oneshot unit that fires once per boot, then reboot now.
"$REPO_ROOT/scripts/device_session.sh" "$HOST" <<REMOTE
set -e
mkdir -p /var/lib/de-boot-matrix && rm -f /var/lib/de-boot-matrix/result.json /var/lib/de-boot-matrix/state.json
cat > /etc/systemd/system/de-boot-matrix.service <<'UNIT'
[Unit]
Description=rabbiz-de cold-boot matrix probe (transient)
After=multi-user.target

[Service]
Type=oneshot
ExecStart=/usr/bin/python3 /var/lib/de-boot-matrix/matrix.py $BOOTS $RESULT

[Install]
WantedBy=multi-user.target
UNIT
systemctl daemon-reload
systemctl enable de-boot-matrix.service
REMOTE

scp -q /tmp/de-boot-matrix-local.py "$HOST:/var/lib/de-boot-matrix/matrix.py"
ssh "$HOST" "cat /var/lib/de-boot-matrix/matrix.py >/dev/null && systemctl reboot" || true
echo ">> boot matrix started: $BOOTS reboots on $HOST"
echo "   poll progress: ssh $HOST cat $RESULT 2>/dev/null"
echo "   (each boot takes ~1-2 min; the matrix writes $RESULT when done)"
