---
status: hypothesis
lane: de
device: samsung/a21s
kernel: 7.3.0-g14a425b3a7dc-dirty
author: Devin (SWE-2)
ai_assisted: true
date: 2026-11-18
evidence:
  - file: devices/samsung/a21s/findings/logs/2026-11-18-de-launch-morph-frame-timing.txt
  - file: tools/rabbiz-de/crates/de-core/src/frame.rs
related:
  - devices/samsung/a21s/findings/2026-11-18-de-session-orphans.md
  - devices/samsung/a21s/findings/2026-11-18-de-morph-page-flips.md
supersedes: []
---

# A composer's first flip after a session swap can fail EBUSY and fall back to labwc

## Observed

After an instrumented `rabbiz-de` test binary was removed and the unit
restarted under the session lock, the restored DE logged (raw log in the
linked frame-trace finding):

```
exynos-drm exynos-drm: [drm] *ERROR* flip_done timed out
exynos-drm exynos-drm: [drm] *ERROR* flip_done timed out
[CRTC:50:crtc-0] vblank wait timed out
run: queue: DrmError(Access(AccessError { errmsg: "Page flip commit failed",
  dev: Some("/dev/dri/card0"), source: Os { code: 16, kind: ResourceBusy,
  message: "Device or resource busy" } }))
rabbiz-session: rabbiz-de init failure (75) — falling back to labwc
original binary restored; session active
```

SSH stayed up; labwc took the session. On the next boot `rabbiz-de`
signalled first-flip readiness normally (see the page-flip finding).

## Interpretation

The signature matches the kernel-side teardown race documented in
[HANDOFF §0](../docs/HANDOFF.md): when a compositor's fd closes,
`drm_framebuffer_remove()` can clear plane `WIN_EN` mid-frame, starving the
video-mode DSI FIFO (`DSIM_INT_UNDER_RUN` → link desync). Here the *next*
compositor's first atomic commit then hits `vblank wait timed out` and
subsequent flips return `EBUSY` until the kernel finishes the deferred
rmfb/restore work. `frame.rs` documents the same race and retries EBUSY
for up to ~6 s *before* the first flip; the failure above came from the
vblank timeout class, which the retry cannot fix — the session wrapper's
exit-75 path is the correct fallback and worked as designed.

This happened once. Whether it is the same DSI-under-run wedge (reboot
required) or a transient EBUSY that only needed the wait is unverified —
the DSI link was demonstrably alive afterwards because labwc and later
`rabbiz-de` boots ran without a panel wedge, which argues for transient
busy rather than the permanent underrun. Status stays `hypothesis` until
a second occurrence is captured.

## Mitigations already in place

- `a21s-drm-keeper.service` holds card0 so `drm_lastclose()` never runs
  mid-handoff; `a21s-display-pin.service` keeps the commit tail devices on.
- `rabbiz-session.service` `ExecStopPost` pkill reaps orphaned compositors
  so the swap does not race a still-mastered DRM fd
  ([orphan finding](2026-11-18-de-session-orphans.md)).
- `de-backend` requests the foreground VT when the libseat session comes
  up inactive.
- `de-core` retries EBUSY before the first flip.

## Reproduce / recovery

Under `scripts/device_session.sh`: swap `/usr/local/bin/rabbiz-de`,
`systemctl restart rabbiz-session.service`, watch `journalctl -u
rabbiz-session -f` and `dmesg | tail` for `flip_done`/`vblank` errors. If
the panel wedges (black screen, `grim` connects but never returns), the
only recovery is reboot — see HANDOFF §0. To put the device back on
`rabbiz-de` after a labwc fallback: write `de` to
`~rabbiz/.config/rabbiz-de/mode` (or ensure `/etc/rabbiz-de/enabled`),
`systemctl restart rabbiz-session.service`, and prefer a reboot over a
bare restart — the documented DSI cold-enable wedge can also be hit by
restart (`scripts/de_deploy.sh` avoids stopping the session for this
reason).
