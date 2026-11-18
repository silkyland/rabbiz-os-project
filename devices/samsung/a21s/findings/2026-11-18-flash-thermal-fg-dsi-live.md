---
status: verified
lane: kernel
device: samsung/a21s
kernel: 7.3.0-g14a425b3a7dc-dirty
author: devin-kernel-agent
ai_assisted: true
date: 2026-11-18
evidence:
  - cmd: "cat /proc/version; ls /sys/class/thermal/thermal_zone*; cat $z/{type,temp,trip_point_*_temp,trip_point_*_type}"
  - cmd: "ls /sys/class/power_supply/; upower -e"
related:
  - devices/samsung/a21s/findings/2026-11-18-kexec-container-kernel-black.md
  - devices/samsung/a21s/findings/2026-11-18-thermal-cpuidle-cpufreq-state.md
  - devices/samsung/a21s/findings/2026-11-18-fuel-gauge-s2mu106-monout-bogus.md
  - devices/samsung/a21s/findings/2026-11-18-dsi-underrun-link-recovery.md
supersedes: []
---

# boot-avb flash landed: thermal zones live, FG renamed, kexec theory corrected

Owner-approved flash of `boot-avb-new.img` (patches 0006-tail + 0009 +
0010) to `/dev/mmcblk0p18` on 2026-11-18; backup
`/tmp/a21s-p18-backup.img` on the Mac and `/root/p18-pre-flash.img` on
the device.

## Verified on the flashed kernel

- **New kernel booted clean**: `#5 SMP Nov 18 10:26 UTC`, built by
  clang 19.1.7 in `rabbizos-kbuild` — **the same container build that
  black-screened under kexec** (refutes the "bad kernel" hypothesis for
  the kexec incident: the kexec path itself is broken, not the image).
- **Thermal zones live** (patch 0009 + `exynos850-tmu.ko`):
  `little0-thermal` 44.4 °C, `little1-thermal` 45.7 °C,
  `gpu-thermal` 43.4 °C; two-point eFuse trims; passive 80 °C trips
  bound to `cpufreq-cpu0`/`cpufreq-cpu4`, 95 °C critical on all three.
  The device went from **zero thermal protection** to passive CPU
  cooling + software criticals. Module autoload persisted via
  `/etc/modules-load.d/exynos850-tmu.conf`.
- **Fuel gauge renamed** (patch 0006 backport): `s2mu106-fuel-gauge`,
  `type=Battery`, bogus `*_avg` properties gone, `capacity=100`,
  `voltage_now=4.347 V`. UPower object is now
  `battery_s2mu106_fuel_gauge` — the DE contract
  (a21s-power-supply-interface.md) already covers the rename.
- **DSI underrun recovery** (patch 0010) is in the running Image;
  functional proof needs an underrun event — the 10-cycle compositor
  acceptance test is still to run.

## Post-flash load tests (same day)

- Full-load `yes` ×8 cores for 2×90 s: zone temps climb 44 → 76-77 °C
  and plateau — passive trip (80 °C) never reached; `cur_state` stayed
  `0/15`, freq pinned at 1.846 GHz. Sensing verified end-to-end.
- **Passive cooling verified**: lowered `trip_point_0_temp` to 70 °C,
  re-ran the load — at 69.5 °C the cooling device stepped to
  `cur_state=1/15` and `scaling_cur_freq` dropped 1846 → 1742 MHz
  (exactly one OPP). Trip restored to 80 °C afterwards.

## Not yet verified

- The underrun restart path firing (needs the wedge or a forced fault).
