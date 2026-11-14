#!/usr/bin/env python3
"""Hardware volume keys -> PipeWire default sink.

gpio-keys (event2) emits KEY_VOLUMEUP/KEY_VOLUMEDOWN. There is no desktop
environment in `steam -gamepadui` to act on them, so this daemon adjusts the
PipeWire default sink via `wpctl` in the `gamer` session. Also covers media
keys on paired keyboards (any evdev device advertising KEY_VOLUMEUP).
"""
import glob
import subprocess
from select import select

from evdev import InputDevice, ecodes

GAMER_ENV = [
    "env", "XDG_RUNTIME_DIR=/run/user/1001",
    "DBUS_SESSION_BUS_ADDRESS=unix:path=/run/user/1001/bus",
]


def wpctl(*args):
    subprocess.run(["runuser", "-u", "gamer", "--"] + GAMER_ENV +
                   ["wpctl"] + list(args), capture_output=True)


def volume_devices():
    devs = {}
    for path in sorted(glob.glob("/dev/input/event*")):
        try:
            d = InputDevice(path)
            keys = d.capabilities().get(ecodes.EV_KEY, ())
            if ecodes.KEY_VOLUMEUP in keys or ecodes.KEY_VOLUMEDOWN in keys:
                devs[d.fd] = d
        except (OSError, PermissionError):
            pass
    return devs


devs = volume_devices()
print("watching:", [d.name for d in devs.values()], flush=True)

while True:
    readable, _, _ = select(list(devs), [], [])
    for fd in readable:
        try:
            events = devs[fd].read()
        except OSError:  # device disappeared (BT keyboard disconnected)
            devs = volume_devices()
            break
        for e in events:
            if e.type != ecodes.EV_KEY or e.value == 0:
                continue
            if e.code == ecodes.KEY_VOLUMEUP:
                wpctl("set-volume", "-l", "1.5", "@DEFAULT_AUDIO_SINK@", "5%+")
            elif e.code == ecodes.KEY_VOLUMEDOWN:
                wpctl("set-volume", "-l", "1.5", "@DEFAULT_AUDIO_SINK@", "5%-")
            elif e.code == ecodes.KEY_MUTE:
                wpctl("set-mute", "@DEFAULT_AUDIO_SINK@", "toggle")
