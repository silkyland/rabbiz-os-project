# Engineering Handoff — Galaxy A21s (`SM-A217F` / Exynos 850)

**This file is now an index.** The session log it used to be was retired
2026-11-18 (Package 10.2): durable content moved to findings and device
docs, stale state to `device.toml` + `ROADMAP.md`. Section anchors below
are kept so older `HANDOFF §0.x` citations still resolve. The canonical
entry point is [findings/README.md](../findings/README.md).

## -1. Active handoff — kernel lane (evening 2026-11-18)

Continuing kernel work tonight? Read
[HANDOFF-kernel-lane.md](HANDOFF-kernel-lane.md) — self-contained:
access, verified state after the day's boot-avb flash (kernel `#5`,
thermal zones + passive cooling live, FG renamed), the ordered cpuidle
MPIDR RE plan, the pending kernel queue (DSI acceptance, suspend abox
veto, ET520/CHUB/mic probes) and safety notes.

## 0. Latest session — SteamOS ⇄ phone session-switch + DSI underrun wedge

Status: **kernel fix written, not boot-tested**
([finding](../findings/2026-11-18-dsi-underrun-link-recovery.md)).

A single `DSIM_INT_UNDER_RUN` at compositor teardown desyncs the
video-mode DSI link permanently: the next atomic commit hits `vblank
wait timed out` → `DECON timeout` → panel commands time out → black
screen while every service reports `active`. Chain: `drm_close_helper`
→ `drm_framebuffer_remove()` force-disables attached planes →
`decon850_disable_plane` clears `WIN_EN` mid-frame → DSIM FIFO starves.
No clean CRTC restart exists in the path; in-band unbind blocks in
`dsim850_stop` waiting on `VIDEO_MODE_ACTIVE`. Reboot only (and reboot
can itself hang on a D-state task inside the wedged driver).

Mitigations deployed: `a21s-display-pin.service` (power/control=on),
`a21s-drm-keeper.service` (holds an fd on `/dev/dri/card0`, drops DRM
master — verified `master=n` so seatd/labwc can take it), manual
`echo 0 > /sys/class/vtconsole/vtcon1/bind` for the fbcon poke vector
(not persisted). `rabbiz-session-switch steamos|phone` round-trip
verified. Kernel fix: patch 0010 schedules link restart
(`stop → init_link → start`) on the underrun IRQ — see the finding.
Alternative (DE decision): weston nested inside labwc removes the
handoff entirely.

### 0.1–0.5 Audio bring-up rounds (2026-11-15) — RESOLVED

DRAM ceiling characterised, console-on-screen fixed, all audio/PipeWire/
boot-flake root causes fixed and verified, `exynos-drm`/`DSI-1` restored
with `CONFIG_EXYNOS_IOMMU=y`, `rabbiz-shell` deployed. The full root-cause
saga is preserved in
[findings/2026-11-18-abox-calliope-root-causes.md](../findings/2026-11-18-abox-calliope-root-causes.md);
the audio plan lives in [a21s-audio-abox-plan.md](a21s-audio-abox-plan.md).

### 0.6 Round 7 — Speaker silent after kernel #12: I2C renumbering (2026-11-17)

Enabling the fuel-gauge `i2c@13830000` renumbered adapters — SMA1303 moved
`0-001e` → `1-001e` and hardcoded paths silently failed. **Rule (now in
[../AGENTS.md](../AGENTS.md)): never hardcode `i2c-N` paths; glob
`/sys/bus/i2c/drivers/<drv>/*/`.** Card presence does not prove
`playback_init` ran.

### 0.7 Round 8 — Charging restored: S2MU106 was in BUCK mode (2026-11-17)

The chip boots in BUCK mode; `CTRL0` must select CHG every boot via
`i2c-0/0x3d`, probed with **plain write+read** (userspace `I2C_SMBUS`
byte reads return garbage). Charge path lives in `s2mu106-charger.ko`,
not DT (rule in [../AGENTS.md](../AGENTS.md)). Current state:
`power_supply` + upower contract in
[a21s-power-supply-interface.md](a21s-power-supply-interface.md).

## 1. Verified device state — MOVED

Now lives in [../device.toml](../device.toml) `[status]` +
[findings/README.md](../findings/README.md). The old table here rotted.

## 2. The Calliope March — root cause story — MOVED

All eleven root causes and the debugging methods:
[findings/2026-11-18-abox-calliope-root-causes.md](../findings/2026-11-18-abox-calliope-root-causes.md).

## 3. Files changed — REMOVED

Was a point-in-time diff list; `git log` is the record.

## 4. Debugging toolkit — MOVED

Reusable methods are folded into the Calliope finding (IOVA vs PA,
config diffing, instrumented firmware hazards, no `BUG_ON` in IPC paths).

## 5. Next steps — MOVED

Tracked in [ROADMAP.md](../../../../ROADMAP.md) M3.6/M5/M6 with truthful
checkboxes.

## 6. Build / Deploy / Flash reference — MOVED

Canonical recipes in [docs/BUILD.md](../../../../docs/BUILD.md), including
the macOS container module build and the corrected kexec recipe
(`--initrd` is mandatory). Boot recovery:
[a21s-boot-recovery.md](a21s-boot-recovery.md).
