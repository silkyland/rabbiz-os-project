---
status: verified
lane: de
device: samsung/a21s
kernel: 7.3.0-g14a425b3a7dc-dirty
author: Devin (SWE-2)
ai_assisted: true
date: 2026-11-19
evidence:
  - file: tools/rabbiz-de/crates/de-core/src/misc.rs
  - file: tools/rabbiz-de/crates/de-core/src/state.rs
related:
  - devices/samsung/a21s/findings/2026-11-18-de-first-flip-ebusy.md
supersedes: []
---

# Clients that render at a forced scale need wp_fractional_scale_v1 or their buffers overflow the output

## Observed

Google Chrome launched through `rabbiz-browser` (`--force-device-scale-factor=1.5`)
submitted a 1080x2400 buffer for its 720x1600 toplevel. With no
`wp_fractional_scale_v1` global, ozone-wayland does not attach a
`wp_viewport` destination, so the compositor drew the raw buffer 1:1 —
the page was ~1.5x oversized and cropped on the right. labwc never showed
this because wlroots advertises fractional scale out of the box.

## Fix

`de-core` now creates `FractionalScaleManagerState` (preferred scale 1.0 —
the single output is scale 1) and delegates `wp_fractional_scale_v1`.
Chrome then binds the global, sets a 720x1600 viewport destination, and
smithay's `SurfaceView` path downscales the 1080x2400 buffer. Verified
on-device: new-tab page fits the panel at the intended 1.5x UI density.

## Note

Every bare `systemctl restart rabbiz-session.service` during this session
hit the documented first-flip `EBUSY` race
([de-first-flip-ebusy](2026-11-18-de-first-flip-ebusy.md)) and fell back
to labwc; reboot is still the reliable way to swap the DE binary.
