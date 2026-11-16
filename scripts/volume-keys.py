#!/usr/bin/env python3
"""Hardware volume keys + screenshot combo -> a21s-volume / a21s-screenshot.

Watches all evdev devices advertising KEY_VOLUMEUP, KEY_VOLUMEDOWN, KEY_MUTE,
or KEY_POWER (`gpio-keys` and `s2mpu12-pwrkey`).

- Single KEY_VOLUMEUP / KEY_VOLUMEDOWN / KEY_MUTE -> `/usr/local/bin/a21s-volume`
- Combo KEY_POWER + KEY_VOLUMEUP (primary), KEY_POWER + KEY_VOLUMEDOWN, or KEY_VOLUMEUP +
  KEY_VOLUMEDOWN within 400ms -> `/usr/local/bin/a21s-screenshot`
- A lone volume press is delayed 200 ms; if Power/another volume key joins in that time it is a combo
  and the volume is NOT changed (no OSD).
- Every volume key press touches MARKER so a21s-power-key can tell that a Power press belongs to a
  key combo and must not sleep/wake the screen (the A21s has no dedicated "action" button; the
  combo uses the Power key).
"""
import glob
import subprocess
import time
from select import select

from evdev import InputDevice, ecodes

COMBO_WINDOW_SEC = 0.40
COMBO_COOLDOWN_SEC = 0.70
VOLUME_DELAY_SEC = 0.20   # hold a lone volume press this long; a combo cancels it (no volume change / OSD)
MARKER = "/run/user/1001/a21s-key-combo"

WATCH_KEYS = {
    ecodes.KEY_VOLUMEUP,
    ecodes.KEY_VOLUMEDOWN,
    ecodes.KEY_MUTE,
    ecodes.KEY_POWER,
}


def mark_volume_press():
    try:
        with open(MARKER, "w") as f:
            f.write("1")
    except OSError:
        pass


def run_async(argv):
    subprocess.Popen(argv, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)


def volume_devices():
    devs = {}
    for path in sorted(glob.glob("/dev/input/event*")):
        try:
            d = InputDevice(path)
            keys = set(d.capabilities().get(ecodes.EV_KEY, ()))
            if keys & WATCH_KEYS:
                devs[d.fd] = d
        except (OSError, PermissionError):
            pass
    return devs


devs = volume_devices()
print("watching:", [d.name for d in devs.values()], flush=True)

held_keys = set()
last_press_ts = {}
last_combo_ts = 0.0
pending = None   # (deadline, argv) of a volume press waiting to see whether it is a combo


def flush_pending(force=False):
    global pending
    if pending and (force or time.monotonic() >= pending[0]):
        run_async(pending[1])
        pending = None


while True:
    wait = 5.0 if not pending else max(0.0, pending[0] - time.monotonic())
    readable, _, _ = select(list(devs), [], [], wait)
    flush_pending()
    if not readable:
        # Periodically rescan for hotplugged USB/BT keyboards
        new_devs = volume_devices()
        if set(new_devs.keys()) != set(devs.keys()):
            devs = new_devs
        continue

    for fd in readable:
        try:
            events = devs[fd].read()
        except OSError:
            devs = volume_devices()
            held_keys.clear()
            break

        for e in events:
            if e.type != ecodes.EV_KEY or e.code not in WATCH_KEYS:
                continue

            now = time.monotonic()

            if e.value == 0:
                held_keys.discard(e.code)
                continue

            if e.value == 1:
                held_keys.add(e.code)
                last_press_ts[e.code] = now

                power_active = (
                    ecodes.KEY_POWER in held_keys
                    or (now - last_press_ts.get(ecodes.KEY_POWER, 0.0)) < COMBO_WINDOW_SEC
                )
                volup_active = (
                    ecodes.KEY_VOLUMEUP in held_keys
                    or (now - last_press_ts.get(ecodes.KEY_VOLUMEUP, 0.0)) < COMBO_WINDOW_SEC
                )
                voldown_active = (
                    ecodes.KEY_VOLUMEDOWN in held_keys
                    or (now - last_press_ts.get(ecodes.KEY_VOLUMEDOWN, 0.0)) < COMBO_WINDOW_SEC
                )

                if e.code in (ecodes.KEY_VOLUMEUP, ecodes.KEY_VOLUMEDOWN):
                    mark_volume_press()

                is_combo = (
                    (e.code == ecodes.KEY_VOLUMEDOWN and (power_active or volup_active))
                    or (e.code == ecodes.KEY_VOLUMEUP and (power_active or voldown_active))
                    or (e.code == ecodes.KEY_POWER and (volup_active or voldown_active))
                )

                if is_combo:
                    pending = None   # the volume press was part of the combo: no volume change
                    if (now - last_combo_ts) >= COMBO_COOLDOWN_SEC:
                        last_combo_ts = now
                        run_async(["/usr/local/bin/a21s-screenshot"])
                    continue

            if (now - last_combo_ts) < COMBO_COOLDOWN_SEC:
                continue

            if e.code in (ecodes.KEY_VOLUMEUP, ecodes.KEY_VOLUMEDOWN):
                flush_pending(force=True)   # rapid taps: the earlier one counts
                step = "up" if e.code == ecodes.KEY_VOLUMEUP else "down"
                pending = (now + VOLUME_DELAY_SEC, ["/usr/local/bin/a21s-volume", step, "5"])
            elif e.code == ecodes.KEY_MUTE and e.value == 1:
                run_async(["/usr/local/bin/a21s-volume", "mute"])
