---
status: verified
lane: de
device: any
kernel: n/a
author: Devin
ai_assisted: true
date: 2026-11-21
evidence:
  - cmd: "WAYLAND_DEBUG=1 google-chrome -> wl_seat.get_touch(new id wl_touch) bound, yet taps did nothing"
  - cmd: "inject_touch down/up on Chrome surface -> no reaction; after adding touch.frame(): tap navigates, drag scrolls (verified via ctl screenshot diff)"
supersedes: []
---

# Chrome appeared input-dead because wl_touch.frame was never sent

## Observed

On the Tier-2 emulator, Chrome bound `wl_touch` (confirmed via
`WAYLAND_DEBUG=1` — `wl_seat.get_touch` is called) yet taps and drags did
nothing at all. GTK/Qt and rabbiz-* apps processed touch normally.
`rabbiz-de` called `touch.down/motion/up` on the seat but never called
`touch.frame()`; `InputEvent::TouchFrame` fell into the `_` catch-all.

## Interpretation

Chromium's ozone-wayland touch path buffers changed touch points and only
dispatches them on `wl_touch.frame` (like evdev SYN_REPORT). Without a frame
event the points sit in the buffer forever — every touch is silently dropped.
Toolkits that dispatch on each event (GTK, Qt, winit) never noticed the missing
frame. The `touch_as_pointer` bridge WIP had been masking this bug by
converting touches to `wl_pointer` events for all third-party windows — which
made clicks work but turned drags into text selection.

## Reproduce

1. In the emu: `inject_touch` down/up on a Chrome surface → no reaction even
   though `wl_touch` is bound.
2. After `touch.frame(self)` is sent after each delivered down/motion/up/cancel
   and `InputEvent::TouchFrame` is forwarded to the seat: tap on a link
   navigates, drag on the page scrolls (screenshot-diff verified).
