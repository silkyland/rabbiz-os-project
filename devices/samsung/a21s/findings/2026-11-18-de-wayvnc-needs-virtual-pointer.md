---
status: verified
lane: de
device: samsung/a21s
kernel: 7.3.0-g14a425b3a7dc-dirty
author: claude-sonnet-5.5
ai_assisted: true
date: 2026-11-18
evidence:
  - file: devices/samsung/a21s/findings/logs/2026-11-18-de-osk-typing.txt
  - file: devices/samsung/a21s/findings/logs/2026-11-18-de-wayvnc-virtual-pointer.txt
---

# a21s-wayvnc fails and restarts under rabbiz-de

## Observed

With `rabbiz-de` as the session compositor, `a21s-wayvnc.service` exits with
"Virtual Pointer protocol not supported by compositor" and systemd restarts it (restart counter 79
shortly after boot). [Raw log](logs/2026-11-18-de-osk-typing.txt)

## Resolution (2026-11-18, commit 77c93ea)

The dependency chain was real, not a guess: wayvnc needs `zwlr_virtual_pointer_v1` (rejected at
bind), `zwlr_virtual_keyboard_v1` and `wlr-screencopy`. `de-core` now registers a v2
`zwlr_virtual_pointer_manager_v1` global (hand-rolled on `wayland-protocols-wlr` 0.3.12 bindings;
Smithay 0.7 has no helper), and the virtual-keyboard client filter was widened from session-IM-only
to any client. After deploying the build and rebooting, `a21s-wayvnc` went from a crash loop
(`status=1/FAILURE`, auto-restart) to `active (running)`, listening on `5900`. [Verification
log](logs/2026-11-18-de-wayvnc-virtual-pointer.txt)

End-to-end pointer proof: an RFB PointerEvent click sent from the Mac at (160,711) hit the
rabbiz-camera shutter and produced a real capture file — virtual pointer events reach focused
xdg-toplevel clients.

## Input mapping policy

Virtual pointer events dispatch through the same `wl_pointer` paths as physical libinput: motion,
absolute motion (protocol extents mapped to output coordinates), button, axis/source/discrete —
batched per `frame` like a real pointer. They do **not** enter the touch gesture tracker, so
shell-internal UI (lock/home/recents/shade zones, which are touch-first) does not react to them;
Wayland client surfaces do. A second `a21s-mirror` caveat: the helper script still shells out as
`gamer`; on-device it only needed `systemctl --user` under `rabbiz`, which works.

## Original hypothesis (superseded by the resolution above)

wayvnc, which the Rabbiz Mirror companion app relies on, needs `zwlr_virtual_pointer_v1` and screen
capture (`wlr-screencopy`); `de-core` implements neither. This was seen once on one boot, and which
globals wayvnc needs in total was not checked, so this is a guess about the dependency chain.
