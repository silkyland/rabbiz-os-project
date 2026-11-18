---
status: hypothesis
lane: de
device: samsung/a21s
kernel: 7.3.0-g14a425b3a7dc-dirty
author: Codex
ai_assisted: true
date: 2026-11-18
evidence:
  - cmd: 'journalctl -b --since "2026-11-18 05:09:22 UTC" --until "2026-11-18 05:10:16 UTC" -o cat --no-pager'
  - file: devices/samsung/a21s/findings/logs/2026-11-18-de-launch-morph-frame-timing.txt
supersedes: []
---

# The launch morph queued frames on the A21s during a run with DRM timeouts

## Observed

One locked run used the arm64 release code in `6d41a07` and the checked-in handset profile. The first
Zenity launch queued 28 morph frames and reached the opening handoff. CPU work across those
frames was 2.58 ms p95 and 8.96 ms max. Most consecutive queue samples were about 25 ms
apart. The raw opening trace also includes 242 ms and 1,752 ms gaps. The compositor logged
handoffs for a cancelled Home swipe and an app close. The first close frame took 91.23 ms
of CPU time. [Frame trace](logs/2026-11-18-de-launch-morph-frame-timing.txt)

The kernel logged a vblank timeout before the captured morph frames. The second launch
did not reach a handoff in the captured run. The test restored the original binary and
removed its temporary service override. The original DE then failed its first flip with
`EBUSY` and the session wrapper selected labwc. SSH remained available.

## Interpretation

The trace shows the compositor exercised launch, cancellation and close submission paths.
`cpu_ms` is time spent preparing and queuing a frame. `queue_gap_ms` is the gap between
successful queue calls in a phase. Physical frame cadence, visual verification, the timeout
fixture and the 60 Hz launcher benchmark are deferred. The cause of the display failure is
unverified. The observed errors match the documented display-handoff failure signature.
[A21s display handoff](../docs/HANDOFF.md#0-latest-session--steamos--phone-session-switch--dsi-underrun-wedge-in-progress)

## Reproduce

Build `de-bin --release` in the arm64 container. Under the device lock, run the binary in
the phone session with `RABBIZ_DE_MOTION_TRACE=1` and `RABBIZ_DE_DEVICE_PROFILE` set to the
A21s profile. Use temporary XDG directories and a fixture `.desktop` with
`Exec=/usr/bin/zenity --info --text=Motion-frame-test --title=Motion-frame-test`. Pin it as
the first grid entry. Use `scripts/touch_swipe.py` to tap `(108,365)`, swipe briefly from
`(360,1585)` to `(360,1550)`, and close with an edge swipe from `(5,700)` to `(240,700)`.
Collect `morph-frame` and `morph-handoff` lines from the journal. Check DRM page flips and
review the actual display before accepting the motion on hardware.
