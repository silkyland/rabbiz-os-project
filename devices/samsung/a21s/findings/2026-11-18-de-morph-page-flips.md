---
status: hypothesis
lane: de
device: samsung/a21s
kernel: 7.3.0-g14a425b3a7dc-dirty
author: Codex
ai_assisted: true
date: 2026-11-18
evidence:
  - cmd: 'strace --trace-fds=<DRM fd> -e trace=read -ttt -xx -s 4096 -p <rabbiz-de pid>'
  - file: devices/samsung/a21s/findings/logs/2026-11-18-de-morph-page-flips.txt
supersedes: []
---

# Launch and return gestures completed real page flips after reboot

## Observed

The A21s rebooted under the device lock and `rabbiz-de` signalled first-flip readiness on the
new boot. A Clock launch, a short Home swipe, and an edge-back close produced 37, 47, and 9
DRM `FLIP_COMPLETE` records in their respective gesture windows. From kernel timestamps, p95
of intervals up to 100 ms was 16.692 ms on launch and 16.687 ms in the other two windows.
Launch and the short swipe each included one double interval near 33.37 ms. The launch window
also contained a 917.74 ms gap between two bursts. The kernel reported zero display timeouts
during this capture. [Decoded flip records](logs/2026-11-18-de-morph-page-flips.txt)

## Interpretation

The records were read by the compositor from card0. Their timestamps and sequences show real
DRM flip completion at roughly 60 Hz during contiguous playback. The gesture windows can
also contain surface updates outside the morph, and the long launch gap was kept in the raw
records. This was one traced run. The regular uninstrumented benchmark and visual feel review
remain open. [Earlier queue-only run](2026-11-18-de-launch-morph-frame-timing.md)

## Reproduce

Use the documented display recovery and keep the device lock for gesture injection and sampling.
Attach `strace` to the ready compositor, restricting reads to its card0 descriptors. Tap the
Clock tile at `(444,365)`, make a 250 ms swipe from `(360,1585)` to `(360,1550)`, and close with
a 250 ms edge swipe from `(5,700)` to `(240,700)` using `a21s-touch`. Decode 32-byte little-endian
DRM events with type `0x02`. The `u32` fields at offsets 16, 20, 24 and 28 hold timestamp seconds,
timestamp microseconds, sequence, and CRTC. Use the syscall time to place events inside each
gesture window and kernel timestamps to compute intervals within a CRTC.
[Display recovery](../docs/HANDOFF.md#0-latest-session--steamos--phone-session-switch--dsi-underrun-wedge-in-progress)
