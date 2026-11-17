#!/usr/bin/env bash
# M1 acceptance run for rabbiz-de on the A21s.
#
# Holds the device lock via scripts/device_session.sh, stops the phone
# session, runs the protocol smoke clients plus one full `rabbiz-de run`
# scenario, prints a PASS/FAIL table, restores the unit, and exits non-zero
# on any FAIL.
#
# NOTE: `--scenario` is only a stats label today - it records a name in the
# JSON summary, it does NOT select or assert any behaviour.
#
# Prereqs on the host: scripts/de_build.sh already ran for arm64, so the
# binaries exist under tools/rabbiz-de/target/{release,debug}/.
# Override paths with RABBIZ_DE_BIN / DE_SMOKE_BIN if needed.
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
HOST="${DE_HOST:-root@172.16.42.1}"
DE_BIN="${RABBIZ_DE_BIN:-$ROOT_DIR/tools/rabbiz-de/target/release/rabbiz-de}"
SMOKE_BIN="${DE_SMOKE_BIN:-$ROOT_DIR/tools/rabbiz-de/target/debug/de-smoke}"
DEVICE="/run/lock/a21s-device-test.lock"

for f in "$DE_BIN" "$SMOKE_BIN"; do
  [[ -f "$f" ]] || { echo "missing binary: $f (run scripts/de_build.sh and cargo build -p de-smoke first)" >&2; exit 2; }
done

echo "== staging test binaries to /tmp under the device lock" >&2
# SC2029: $DEVICE must expand client-side; it is the same lock path everywhere.
# shellcheck disable=SC2029
ssh "$HOST" "flock --close --timeout 300 $DEVICE mkdir -p /tmp/de-m1-check" </dev/null
# shellcheck disable=SC2029
ssh "$HOST" "flock --close --timeout 300 $DEVICE sh -c 'cat > /tmp/de-m1-check/rabbiz-de'" <"$DE_BIN"
# shellcheck disable=SC2029
ssh "$HOST" "flock --close --timeout 300 $DEVICE sh -c 'cat > /tmp/de-m1-check/de-smoke'" <"$SMOKE_BIN"
# shellcheck disable=SC2029
ssh "$HOST" "flock --close --timeout 300 $DEVICE chmod 777 /tmp/de-m1-check && chmod +x /tmp/de-m1-check/rabbiz-de /tmp/de-m1-check/de-smoke" </dev/null

REMOTE_SCRIPT="$(cat <<'REMOTE'
set -u
# device_session.sh runs us under `bash -se`; -e would abort on expected
# nonzero exits (pkill with no match, record), so disable it explicitly.
set +e
# Device-side M1 acceptance. Runs inside the flock held by device_session.sh.
UID_G=$(id -u gamer)
RT=/run/user/$UID_G
CHECK=/tmp/de-m1-check
DE=$CHECK/rabbiz-de
SMOKE=$CHECK/de-smoke
UNIT=a21s-phone-session.service
LOG=$CHECK/de.log
OUT=$RT/de-m1-check
SUMMARY=$OUT/summary.json
SHOT=$OUT/m1.png
PASS=0
FAIL=0
declare -A RESULT

record() {
  local name="$1" ok="$2"
  if [[ "$ok" == "0" ]]; then
    RESULT[$name]="PASS"; PASS=$((PASS+1))
  else
    RESULT[$name]="FAIL"; FAIL=$((FAIL+1))
  fi
}

restore() {
  pkill -x rabbiz-de 2>/dev/null || true
  systemctl start "$UNIT" 2>/dev/null || true
}
trap restore EXIT

systemctl stop "$UNIT" 2>/dev/null || true
sleep 1
pkill -x rabbiz-de 2>/dev/null || true
# a21s-osk is a user-session client of the labwc session; left running it
# reconnects to the test socket every ~2.5s and floods the wire log.
pkill -x a21s-osk 2>/dev/null || true
rm -f "$RT/wayland-0" "$RT/rabbiz-de.ready" "$SUMMARY" "$SHOT"
mkdir -p "$RT"; chown "$UID_G":"$UID_G" "$RT" 2>/dev/null

echo "-- starting rabbiz-de (scenario label m1-calc)"
# $OUT must be writable by gamer (screenshot + stats summary are written
# by the DE running as gamer, not by this root script).
runuser -u gamer -- mkdir -p "$OUT"
runuser -u gamer -- env \
  XDG_RUNTIME_DIR="$RT" WAYLAND_DISPLAY=wayland-0 \
  LIBSEAT_BACKEND=seatd XDG_SESSION_TYPE=wayland \
  "$DE" run --scenario m1-calc --spawn a21s-calculator \
      --screenshot "$SHOT" --screenshot-at 5 \
      --stats-summary "$SUMMARY" \
      --duration 30 \
      >"$LOG" 2>&1 &
DEPID=$!

READY=0
for _ in $(seq 1 100); do
  [[ -f "$RT/rabbiz-de.ready" ]] && { READY=1; break; }
  kill -0 $DEPID 2>/dev/null || break
  sleep 0.2
done
if [[ "$READY" != "1" ]]; then
  echo "de did not become ready; log:"; cat "$LOG"
  record ready 1
else
  record ready 0
  # Give --spawn a beat to map the calculator before the smoke clients run.
  sleep 2

  for t in popup clipboard datacontrol framecb; do
    if timeout 25 runuser -u gamer -- env XDG_RUNTIME_DIR="$RT" WAYLAND_DISPLAY=wayland-0 \
        "$SMOKE" "$t" >>"$LOG" 2>&1; then
      record "$t" 0
    else
      record "$t" 1
    fi
  done
fi

wait $DEPID 2>/dev/null

if [[ -s "$SUMMARY" ]] && grep -q '"primary_composite":[1-9]' "$SUMMARY"; then
  record primary_composite 0
else
  record primary_composite 1
fi
[[ -s "$SHOT" ]] && record screenshot 0 || record screenshot 1

echo
echo "==== M1 ACCEPTANCE ===="
printf '%-20s %s\n' check result
for k in ready popup clipboard datacontrol framecb primary_composite screenshot; do
  printf '%-20s %s\n' "$k" "${RESULT[$k]:-SKIP}"
done
echo "summary: $(cat "$SUMMARY" 2>/dev/null || echo missing)"
[[ $FAIL -eq 0 ]]
REMOTE
)"

echo "== running device session" >&2
set +e
echo "$REMOTE_SCRIPT" | "$ROOT_DIR/scripts/device_session.sh" "$HOST"
rc=$?
set -e
if [[ $rc -eq 75 ]]; then
  echo "device slot busy; stopped without running" >&2
elif [[ $rc -ne 0 ]]; then
  echo "device session failed (rc=$rc)" >&2
fi
exit "$rc"
