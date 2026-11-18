---
status: reproduced
lane: kernel
device: samsung/a21s
kernel: 7.3.0-g14a425b3a7dc-dirty
author: devin-kernel-agent
ai_assisted: true
date: 2026-11-18
evidence:
  - cmd: "echo mem > /sys/power/state ; dmesg | tail -15"
  - cmd: "echo freeze > /sys/power/state"
supersedes: []
---

# Suspend-to-RAM cannot be entered: abox vetoes it (kernel #12)

## Observed

Under the device lock with an RTC wakealarm armed:

- `echo mem > /sys/power/state` → dmesg shows
  `PM: suspend entry (deep)`, `Filesystems sync`, then
  `samsung-abox 14a50000.abox: runtime suspend: -11` with
  `usage_count:1`, then `PM: suspend exit`. The write returns
  `Operation not permitted` to userspace — the whole suspend aborts on
  the abox `-EAGAIN`.
- `echo freeze > /sys/power/state` is rejected even earlier — no
  `PM: suspend entry` line at all (s2idle vetoed in the write path;
  `/sys/power/wakeup_count` reads 162).
- No userspace PCM holders (`fuser /dev/snd/pcm*` empty) — the
  `usage_count 1` runtime reference is held inside the abox driver
  itself, not by PipeWire/WirePlumber.
- Side observation: during the suspend device phase the panel driver
  tried `Direct firmware load for td4150_a21s.bin` and failed `-2` —
  that firmware file does not exist on the rootfs. Harmless today, but
  it will be on the suspend/resume path when suspend actually works.

## Interpretation

Suspend is not merely risky — it cannot be entered at all on this
kernel, and the first named blocker is the ABOX audio driver's own
pm_runtime reference. Because the abort happens in the early device
phase, the actual question from M5 — whether `decon`/`dsi`, `scsc_mx`
(WLBT) and CHUB survive resume — is **completely untested**, not
answered.

## Reproduce

```sh
echo +35 > /sys/class/rtc/rtc0/wakealarm
echo mem > /sys/power/state     # returns EPERM; dmesg shows the abox veto
```

## Next

1. Find where the abox driver takes the pm_runtime ref it never drops
   (`pm_runtime_get*` in `socs/exynos850/drivers/abox/`; likely held
   across the card's life rather than per-stream).
2. Once `mem` passes the device phase, expect the next blockers in
   order: decon/dsi (the wedge path), scsc/wlbt, chub.
3. Ship `td4150_a21s.bin` in `/lib/firmware` before retesting — the
   panel driver wants it on the suspend path.
