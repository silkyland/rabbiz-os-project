---
status: hypothesis
lane: de
device: samsung/a21s
kernel: 7.3.0-g14a425b3a7dc-dirty
author: Devin (SWE-2)
ai_assisted: true
date: 2026-11-18
evidence:
  - cmd: 'git show 598fd8c -- rootfs-overlay/etc/systemd/system/rabbiz-session.service tools/rabbiz-de/crates/de-backend/src/session.rs'
  - file: devices/samsung/a21s/findings/logs/2026-11-18-de-osk-acceptance/evidence.txt
  - file: rootfs-overlay/etc/systemd/system/rabbiz-session.service
related:
  - devices/samsung/a21s/findings/2026-11-18-de-first-flip-ebusy.md
supersedes: []
---

# Stopping rabbiz-session.service can orphan the compositor, which keeps DRM and the seat

## Observed

During the 2026-11-18 OSK acceptance runs the compositor "had lost DRM to an
orphan and the session fell back to labwc" (evidence.txt). Root cause
identified and fixed in `598fd8c` ("de: session/seat fixes from A21s device
bring-up"):

1. **`PAMName=login` moves the compositor out of the unit's cgroup.**
   `pam_systemd` places the session in `session-*.scope`, so
   `systemctl stop rabbiz-session.service` kills the wrapper but leaves
   `rabbiz-de`/`labwc` running while still holding DRM master and the seat.
   The next start then races the orphan — the failure signature in
   [the EBUSY finding](2026-11-18-de-first-flip-ebusy.md).
   Fix shipped: `ExecStopPost=-/usr/bin/pkill -x rabbiz-de` and
   `pkill -x labwc` in
   [rabbiz-session.service](../../../rootfs-overlay/etc/systemd/system/rabbiz-session.service).
2. **seatd grants DRM master only while the seat is active.** wlroots does
   the VT switch itself; smithay leaves it to the caller. `de-backend` now
   requests the foreground VT (`/sys/class/tty/tty0/active`) when the
   libseat session comes up inactive
   (`tools/rabbiz-de/crates/de-backend/src/session.rs`).

## Interpretation

The pkill-on-stop is a workaround for a systemd property (scope escape),
not a fix for the DRM-handoff wedge — the kernel-side signature is the
separate [HANDOFF §0](../docs/HANDOFF.md) thread. Orphans were observed
once; the fix has been in place for the subsequent clean boots (page-flip
finding, OSK acceptance runs), but no dedicated stop/start orphan test has
been run since, hence `hypothesis`.

## Reproduce

On the device under `scripts/device_session.sh`: start
`rabbiz-session.service`, confirm `rabbiz-de` runs, `systemctl stop` the
unit, then `pgrep -a rabbiz-de` — the ExecStopPost pkill should leave
nothing. Without the pkill lines, `pgrep` shows the orphaned compositor
still running and `ls -l /dev/dri/card0` holders can be checked with
`fuser`.
