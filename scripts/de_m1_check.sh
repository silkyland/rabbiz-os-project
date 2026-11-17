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
ssh "$HOST" "flock --close --timeout 300 $DEVICE sh -c 'cat > /tmp/de-m1-check/a21s-session'" <"$ROOT_DIR/rootfs-overlay/usr/local/bin/a21s-session"
# shellcheck disable=SC2029
ssh "$HOST" "flock --close --timeout 300 $DEVICE sh -c 'cat > /tmp/de-m1-check/a21s-power-key'" <"$ROOT_DIR/rootfs-overlay/usr/local/bin/a21s-power-key"
# shellcheck disable=SC2029
ssh "$HOST" "flock --close --timeout 300 $DEVICE chmod 777 /tmp/de-m1-check && chmod +x /tmp/de-m1-check/rabbiz-de /tmp/de-m1-check/de-smoke /tmp/de-m1-check/a21s-session /tmp/de-m1-check/a21s-power-key" </dev/null

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
SESSION=$CHECK/a21s-session
POWERKEY=$CHECK/a21s-power-key
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
  # Kill the staged wrapper first: its restart-on-failure path would respawn.
  pkill -f 'de-m1-check/a21s-session' 2>/dev/null || true
  pkill -x rabbiz-de 2>/dev/null || true
  # Leftover manual fallback from the forcefail test must not fight the unit.
  pkill -f 'a21s-phone-session' 2>/dev/null || true
  pkill -x labwc 2>/dev/null || true
  sleep 1
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

# --- env contract (M1-T7): exported vars must reach a child env, and the
# unset sweep must remove wlroots/Mesa overrides. This is the exact eval the
# session wrapper performs.
runuser -u gamer -- bash -c 'eval "$('"$DE"' env --shell)" && env' >"$CHECK/env.out" 2>>"$LOG"
if grep -q '^A21S_FORCE_SOFTWARE=1$' "$CHECK/env.out" \
   && grep -q '^XDG_CURRENT_DESKTOP=Rabbiz$' "$CHECK/env.out" \
   && grep -q '^XKB_DEFAULT_LAYOUT=us,th$' "$CHECK/env.out" \
   && grep -q '^XKB_DEFAULT_OPTIONS=grp:alt_shift_toggle$' "$CHECK/env.out" \
   && grep -q '^QT_SCALE_FACTOR=1$' "$CHECK/env.out"; then
  record env 0
else
  record env 1
fi
runuser -u gamer -- env LD_PRELOAD=/tmp/evil.so WLR_FAKE_VAR=1 RABBIZ_EXYNOS_TEST=1 \
  bash -c 'eval "$('"$DE"' env --shell)" && env' >"$CHECK/env2.out" 2>>"$LOG"
if ! grep -qE '^(LD_PRELOAD|WLR_FAKE_VAR|RABBIZ_EXYNOS_TEST)=' "$CHECK/env2.out"; then
  record env_unset 0
else
  record env_unset 1
fi

# --- mode resolution (M1-T6): the staged wrapper, a throwaway config dir.
MODE_CFG="$CHECK/xdgcfg"
mkdir -p "$MODE_CFG"; chown "$UID_G":"$UID_G" "$MODE_CFG"
mode_ok=0
pm() { runuser -u gamer -- env XDG_CONFIG_HOME="$MODE_CFG" XDG_RUNTIME_DIR="$RT" \
        RABBIZ_DE_BIN="$DE" bash "$SESSION" --print-mode "$@" 2>/dev/null; }
EXPECT_DEF=labwc; [ -e /etc/rabbiz-de/enabled ] && EXPECT_DEF=de
[ "$(pm)" = "$EXPECT_DEF" ] || mode_ok=1
[ "$(pm --de)" = "de" ] || mode_ok=1
[ "$(pm --labwc)" = "labwc" ] || mode_ok=1
[ "$(pm --de --labwc)" = "labwc" ] || mode_ok=1
[ "$(pm --labwc --de)" = "labwc" ] || mode_ok=1
echo de > "$MODE_CFG/rabbiz-de.mode.tmp"; mkdir -p "$MODE_CFG/rabbiz-de"; mv "$MODE_CFG/rabbiz-de.mode.tmp" "$MODE_CFG/rabbiz-de/mode"; chown -R "$UID_G":"$UID_G" "$MODE_CFG"
[ "$(pm)" = "de" ] || mode_ok=1
echo bogus > "$MODE_CFG/rabbiz-de/mode"
[ "$(pm)" = "labwc" ] || mode_ok=1
rm -f "$MODE_CFG/rabbiz-de/mode"
record mode "$mode_ok"

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
      --duration 60 \
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

  # --- launch path (M1-T7): `rabbiz-de launch <desktop-id>` must spawn the
  # app with the contract env. a21s-clock is a quickshell wrapper; match its
  # script arg, not the process name.
  # HOME must point at gamer's home: runuser keeps the caller's HOME, and the
  # launch helper derives XDG_STATE_HOME from it for the app log file.
  runuser -u gamer -- env HOME=/home/gamer XDG_RUNTIME_DIR="$RT" WAYLAND_DISPLAY=wayland-0 \
    "$DE" launch a21s-clock >>"$LOG" 2>&1
  sleep 5
  CLOCK_PID="$(pgrep -f 'a21s-apps/clock' | head -1)"
  if [ -n "$CLOCK_PID" ] \
     && [ -f "/home/gamer/.local/state/rabbiz-de/apps/a21s-clock.log" ] \
     && tr '\0' '\n' <"/proc/$CLOCK_PID/environ" 2>/dev/null | grep -q '^A21S_FORCE_SOFTWARE=1$'; then
    record launch 0
  else
    record launch 1
  fi

  # --- input path (M1-T1/T8): inject a key press and a tap through uinput;
  # the smoke client only passes if both reach it as wl_keyboard.key and
  # wl_touch.down.
  cat > "$CHECK/inject.py" <<'PYEOF'
from evdev import UInput, ecodes, AbsInfo
import time

kb = UInput({ecodes.EV_KEY: [ecodes.KEY_A]}, name="de-m1-kbd")
# INPUT_PROP_DIRECT is what libinput uses to classify this as a touchscreen
# (wl_touch) rather than a touchpad (wl_pointer). Older python-evdev lacks the
# keyword, so fall back and hope the seat still routes it as touch.
ts_events = {
    ecodes.EV_KEY: [ecodes.BTN_TOUCH],
    ecodes.EV_ABS: [
        (ecodes.ABS_X, AbsInfo(0, 0, 719, 0, 0, 0)),
        (ecodes.ABS_Y, AbsInfo(0, 0, 1599, 0, 0, 0)),
    ],
}
try:
    ts = UInput(ts_events, name="de-m1-touch",
                input_props={ecodes.INPUT_PROP_DIRECT})
except TypeError:
    ts = UInput(ts_events, name="de-m1-touch")
time.sleep(1.0)
# Tap first: the DE configures every toplevel fullscreen, so the tap at
# (360,800) lands on the topmost (smoke) window and pins touch focus there.
ts.write(ecodes.EV_ABS, ecodes.ABS_X, 360)
ts.write(ecodes.EV_ABS, ecodes.ABS_Y, 800)
ts.write(ecodes.EV_KEY, ecodes.BTN_TOUCH, 1)
ts.syn()
time.sleep(0.05)
ts.write(ecodes.EV_KEY, ecodes.BTN_TOUCH, 0)
ts.syn()
time.sleep(0.1)
kb.write(ecodes.EV_KEY, ecodes.KEY_A, 1)
kb.syn()
time.sleep(0.05)
kb.write(ecodes.EV_KEY, ecodes.KEY_A, 0)
kb.syn()
time.sleep(0.5)
kb.close()
ts.close()
print("injected tap + key")
PYEOF
  runuser -u gamer -- env XDG_RUNTIME_DIR="$RT" WAYLAND_DISPLAY=wayland-0 \
    timeout 25 "$SMOKE" input >>"$LOG" 2>&1 &
  INP_PID=$!
  sleep 3
  python3 "$CHECK/inject.py" >>"$LOG" 2>&1
  if wait "$INP_PID"; then
    record input 0
  else
    record input 1
  fi

  # --- power-key detection (M1-T6): with the DE running, `a21s-power-key
  # lock` must not touch the raw backlight (the shell owns brightness). If
  # the panel exposes no backlight node there is nothing to compare: skip.
  BL_BEFORE="$(cat /sys/class/backlight/*/brightness 2>/dev/null | head -1)"
  if [ -n "$BL_BEFORE" ]; then
    sh "$POWERKEY" lock >>"$LOG" 2>&1
    sleep 0.5
    BL_AFTER="$(cat /sys/class/backlight/*/brightness 2>/dev/null | head -1)"
    if [ "$BL_BEFORE" = "$BL_AFTER" ]; then
      record powerkey 0
    else
      record powerkey 1
    fi
  fi
fi

wait $DEPID 2>/dev/null
pkill -f 'a21s-apps/clock' 2>/dev/null || true

if [[ -s "$SUMMARY" ]] && grep -q '"primary_composite":[1-9]' "$SUMMARY"; then
  record primary_composite 0
else
  record primary_composite 1
fi
[[ -s "$SHOT" ]] && record screenshot 0 || record screenshot 1

# --- session wrapper end-to-end (M1-T6): `a21s-session --de` must start the
# DE and produce the readiness file.
rm -f "$RT/rabbiz-de.ready"
runuser -u gamer -- env HOME=/home/gamer XDG_RUNTIME_DIR="$RT" LIBSEAT_BACKEND=seatd \
  XDG_SESSION_TYPE=wayland RABBIZ_DE_BIN="$DE" \
  bash "$SESSION" --de >>"$LOG" 2>&1 &
SESS_PID=$!
SESS_OK=1
for _ in $(seq 1 75); do
  [ -f "$RT/rabbiz-de.ready" ] && { SESS_OK=0; break; }
  kill -0 "$SESS_PID" 2>/dev/null || break
  sleep 0.2
done
record session "$SESS_OK"
# Kill the wrapper first so its restart-on-failure path cannot respawn.
pkill -f 'de-m1-check/a21s-session' 2>/dev/null || true
pkill -x rabbiz-de 2>/dev/null || true
# labwc cannot claim the seat until the DE released it - wait for the exit.
for _ in $(seq 1 25); do
  pgrep -x rabbiz-de >/dev/null 2>&1 || break
  sleep 0.2
done

# --- force-fail fallback (M1-T6): `a21s-session --de --force-fail` must exec
# into labwc within ~10s (child exits 75 before the first frame).
runuser -u gamer -- env HOME=/home/gamer XDG_RUNTIME_DIR="$RT" LIBSEAT_BACKEND=seatd \
  XDG_SESSION_TYPE=wayland RABBIZ_DE_BIN="$DE" \
  bash "$SESSION" --de --force-fail >>"$LOG" 2>&1 &
sleep 1
FF_OK=1
for _ in $(seq 1 50); do
  if pgrep -x labwc >/dev/null 2>&1; then FF_OK=0; break; fi
  sleep 0.2
done
record forcefail "$FF_OK"
# The fallback spawned the real labwc session outside systemd - tear it down
# before the restore trap brings the unit back.
pkill -f 'de-m1-check/a21s-session' 2>/dev/null || true
pkill -x labwc 2>/dev/null || true
pkill -x quickshell 2>/dev/null || true
sleep 1

echo
echo "==== M1 ACCEPTANCE ===="
printf '%-20s %s\n' check result
for k in env env_unset mode ready popup clipboard datacontrol framecb launch input powerkey primary_composite screenshot session forcefail; do
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
