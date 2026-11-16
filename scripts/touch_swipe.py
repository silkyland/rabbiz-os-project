#!/usr/bin/env python3
"""touch_swipe.py — inject a touch swipe/tap through uinput (runs ON the device, as root).
Lets agents test gestures without touching the phone.   Usage:
  touch_swipe.py x1 y1 x2 y2 [ms]     swipe (panel is 720x1600 portrait)
  touch_swipe.py x y                  tap
labwc maps every touch device to DSI-1 (rc.xml <touch deviceName="*">)."""
import sys, time
from evdev import UInput, AbsInfo, ecodes as e
W, H = 720, 1600
a = [float(v) for v in sys.argv[1:5]] if len(sys.argv) >= 5 else None
if a is None and len(sys.argv) == 3: a = [float(sys.argv[1]), float(sys.argv[2])] * 2
ms = int(sys.argv[5]) if len(sys.argv) > 5 else 300
caps = {e.EV_KEY: [e.BTN_TOUCH], e.EV_ABS: [
    (e.ABS_MT_SLOT, AbsInfo(0, 0, 4, 0, 0, 0)), (e.ABS_MT_TRACKING_ID, AbsInfo(0, 0, 65535, 0, 0, 0)),
    (e.ABS_MT_POSITION_X, AbsInfo(0, 0, W - 1, 0, 0, 0)), (e.ABS_MT_POSITION_Y, AbsInfo(0, 0, H - 1, 0, 0, 0)),
    (e.ABS_X, AbsInfo(0, 0, W - 1, 0, 0, 0)), (e.ABS_Y, AbsInfo(0, 0, H - 1, 0, 0, 0))]}
ui = UInput(caps, name="a21s-test-touch", version=1)
time.sleep(1.0)  # let labwc pick the new device up
def pos(x, y):
    ui.write(e.EV_ABS, e.ABS_MT_POSITION_X, int(x)); ui.write(e.EV_ABS, e.ABS_MT_POSITION_Y, int(y))
    ui.write(e.EV_ABS, e.ABS_X, int(x)); ui.write(e.EV_ABS, e.ABS_Y, int(y)); ui.syn()
x1, y1, x2, y2 = a
ui.write(e.EV_ABS, e.ABS_MT_SLOT, 0); ui.write(e.EV_ABS, e.ABS_MT_TRACKING_ID, 1)
ui.write(e.EV_KEY, e.BTN_TOUCH, 1); pos(x1, y1)
steps = max(2, ms // 16)
for i in range(1, steps + 1):
    t = i / steps; pos(x1 + (x2 - x1) * t, y1 + (y2 - y1) * t); time.sleep(ms / 1000 / steps)
ui.write(e.EV_ABS, e.ABS_MT_TRACKING_ID, -1); ui.write(e.EV_KEY, e.BTN_TOUCH, 0); ui.syn()
time.sleep(0.3); ui.close()
