#!/usr/bin/env python3
"""touch_path.py 'x,y,ms; x,y,ms; ...' — finger down at the first point, linear moves to each
next point taking ms, a dwell (same point, ms) jitters +-1px so a long-press can fire, then lift.
Env HOLD_END_MS keeps the finger down that long at the end (for screenshots while held)."""
import os, sys, time
from evdev import UInput, AbsInfo, ecodes as e
W, H = 720, 1600
pts = [tuple(float(v) for v in s.split(',')) for s in sys.argv[1].split(';')]
caps = {e.EV_KEY: [e.BTN_TOUCH], e.EV_ABS: [
    (e.ABS_MT_SLOT, AbsInfo(0, 0, 4, 0, 0, 0)), (e.ABS_MT_TRACKING_ID, AbsInfo(0, 0, 65535, 0, 0, 0)),
    (e.ABS_MT_POSITION_X, AbsInfo(0, 0, W - 1, 0, 0, 0)), (e.ABS_MT_POSITION_Y, AbsInfo(0, 0, H - 1, 0, 0, 0)),
    (e.ABS_X, AbsInfo(0, 0, W - 1, 0, 0, 0)), (e.ABS_Y, AbsInfo(0, 0, H - 1, 0, 0, 0))]}
ui = UInput(caps, name="a21s-test-touch", version=1)
time.sleep(1.2)
def pos(x, y):
    ui.write(e.EV_ABS, e.ABS_MT_POSITION_X, int(x)); ui.write(e.EV_ABS, e.ABS_MT_POSITION_Y, int(y))
    ui.write(e.EV_ABS, e.ABS_X, int(x)); ui.write(e.EV_ABS, e.ABS_Y, int(y)); ui.syn()
x, y, _ = pts[0]
ui.write(e.EV_ABS, e.ABS_MT_SLOT, 0); ui.write(e.EV_ABS, e.ABS_MT_TRACKING_ID, 1)
ui.write(e.EV_KEY, e.BTN_TOUCH, 1); pos(x, y)
def dwell(ms, x, y):
    t0 = time.time(); k = 0
    while (time.time() - t0) * 1000 < ms:
        k ^= 1; pos(x + k, y); time.sleep(0.03)
for (nx, ny, ms) in pts[1:]:
    if (nx, ny) == (x, y):
        dwell(ms, x, y)
    else:
        steps = max(2, int(ms // 16))
        for i in range(1, steps + 1):
            t = i / steps; pos(x + (nx - x) * t, y + (ny - y) * t); time.sleep(ms / 1000 / steps)
        x, y = nx, ny
hold = int(os.environ.get("HOLD_END_MS", "0"))
open("/tmp/tp.hold","w").write("1")
if hold: dwell(hold, x, y)
ui.write(e.EV_ABS, e.ABS_MT_TRACKING_ID, -1); ui.write(e.EV_KEY, e.BTN_TOUCH, 0); ui.syn()
time.sleep(0.3); ui.close()
