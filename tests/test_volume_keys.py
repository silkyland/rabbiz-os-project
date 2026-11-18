"""Run the actual evdev loop with deterministic key events and no hardware."""
import contextlib
import io
from pathlib import Path
import runpy
import sys
from types import SimpleNamespace
import unittest
from unittest.mock import mock_open, patch

BIN = Path(__file__).resolve().parents[1] / 'rootfs-overlay/usr/local/bin'
sys.path.insert(0, str(BIN))
POWER, UP, DOWN = 116, 115, 114


class TraceComplete(Exception):
    pass


def replay(events):
    now = 10.0
    frames = [(at, [SimpleNamespace(type=1, code=code, value=value)])
              for at, code, value in events]
    frames.append((events[-1][0] + 1, []))
    current = []
    actions = []
    live = set()

    def read_events():
        for e in current:
            if e.value == 1:
                live.add(e.code)
            elif e.value == 0:
                live.discard(e.code)
        return current

    device = SimpleNamespace(fd=42, name='test keys',
        capabilities=lambda: {1: [POWER, UP, DOWN, 113]},
        read=read_events,
        active_keys=lambda: set(live))

    def select_events(read, write, error, timeout):
        nonlocal now, current
        if not frames:
            raise TraceComplete
        at, events = frames[0]
        if now + timeout < at:
            now += timeout
            return [], [], []
        frames.pop(0)
        now = at
        current = events
        return ([42] if events else []), [], []

    evdev = SimpleNamespace(InputDevice=lambda _: device, ecodes=SimpleNamespace(
        EV_KEY=1, KEY_POWER=POWER, KEY_VOLUMEUP=UP, KEY_VOLUMEDOWN=DOWN, KEY_MUTE=113))
    with patch.dict(sys.modules, {'evdev': evdev}), \
         patch('glob.glob', return_value=['test-device']), \
         patch('time.monotonic', side_effect=lambda: now), \
         patch('select.select', side_effect=select_events), \
         patch('subprocess.Popen', side_effect=lambda argv, **kwargs: actions.append(argv)), \
         patch('pwd.getpwnam', return_value=SimpleNamespace(pw_uid=1001)), \
         patch('builtins.open', mock_open()), contextlib.redirect_stdout(io.StringIO()):
        try:
            runpy.run_path(str(BIN / 'volume-keys.py'))
        except TraceComplete:
            pass
    return [argv[0].rsplit('/', 1)[-1:] + argv[1:] for argv in actions]


class VolumeKeysTest(unittest.TestCase):
    def test_released_power_does_not_turn_volume_up_into_screenshot(self):
        self.assertEqual(replay([(10, POWER, 1), (10.1, POWER, 0),
                                 (10.2, UP, 1), (10.3, UP, 0)]),
                         [['a21s-volume', 'up', '5']])

    def test_separate_volume_taps_are_not_a_combo(self):
        self.assertEqual(replay([(10, DOWN, 1), (10.1, DOWN, 0),
                                 (10.25, UP, 1), (10.35, UP, 0)]),
                         [['a21s-volume', 'down', '5'], ['a21s-volume', 'up', '5']])

    def test_held_combo_repeats_do_not_change_volume(self):
        self.assertEqual(replay([(10, DOWN, 1), (10.03, POWER, 1),
                                 (10.5, DOWN, 2), (10.7, DOWN, 2),
                                 (14, POWER, 0), (14.03, DOWN, 0)]),
                         [['a21s-power-key', 'menu']])

    def test_tap_combo_takes_one_screenshot(self):
        self.assertEqual(replay([(10, DOWN, 1), (10.03, POWER, 1),
                                 (10.15, POWER, 0), (10.18, DOWN, 0)]),
                         [['rabbiz-screenshot']])


if __name__ == '__main__':
    unittest.main()
