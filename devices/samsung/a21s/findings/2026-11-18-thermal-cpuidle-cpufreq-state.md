---
status: reproduced
lane: kernel
device: samsung/a21s
kernel: 7.3.0-g14a425b3a7dc-dirty
author: devin-kernel-agent
ai_assisted: true
date: 2026-11-18
evidence:
  - cmd: "ls /sys/class/thermal/; cat /sys/devices/system/cpu/cpu*/cpufreq/{scaling_governor,scaling_cur_freq,stats/time_in_state}; ls /sys/devices/system/cpu/cpu0/cpuidle/"
supersedes: []
related:
  - devices/samsung/a21s/findings/2026-11-18-panfrost-thermal-run.md
  - devices/samsung/a21s/findings/2026-11-18-tmu-efuse-temperature.md
---

# Thermal zones, cpuidle and cpufreq governor state on kernel #12

## Observed

On the flashed kernel `7.3.0-g14a425b3a7dc-dirty` (#12):

- **No thermal zones exist** (`/sys/class/thermal/thermal_zone*` absent).
  Two cooling devices (`cpufreq-cpu0`, `cpufreq-cpu4`, max_state 15) are
  registered but bound to nothing. The temporary TMU module from the
  Panfrost run (three zones, 95°C critical) is gone — no stale critical
  trip remains, but only because nothing is there at all. The patch-0009
  DTB thermal zones are not in this kernel.
- **cpuidle has zero states** — `cpuidle: using governor menu` logged at
  boot, but no `state*` directories under
  `/sys/devices/system/cpu/cpu0/cpuidle/` and no `idle-states` nodes in
  the running DT (`/sys/firmware/devicetree/base`). `CONFIG_ARM_PSCI_CPUIDLE=y`
  and PSCIv1.1 firmware is present; the DT simply declares no idle states,
  so CPUs only get WFI.
- **Governor is `performance`**, not the defconfig default `schedutil`
  (which is compiled in and listed in `scaling_available_governors`
  alongside ondemand/userspace). `time_in_state` shows ~100% at
  1,846,000 kHz. Nothing in systemd/sysctl sets it; the udev rule
  `90-a21s-backlight.rules` + `scripts/deploy_shell.sh` grant group
  `video` write access to `scaling_governor` — i.e. the design intends
  userspace (DE/session) to pick the governor, and `performance` is the
  current choice of unknown origin.

## Interpretation

Thermal protection is currently **absent**, not merely untested — any
sustained CPU/GPU load relies on hardware TMU shutdown only. Package 2.1
(boot-testing patch 0009) needs a kernel+dtb boot, which kexec cannot
currently provide for foreign-built images (see
`2026-11-18-kexec-container-kernel-black.md`).

For power: with no idle states and a pinned `performance` governor, the
device runs max clock at all times — the worst-case configuration for
both battery life and heat. Switching to `schedutil` is a userspace
decision (write access is deliberately given to the DE group); whether
to change the default, and where, is a DE/power-policy question, not a
kernel fix.

## Reproduce

`ls /sys/class/thermal/` (empty), `ls /sys/devices/system/cpu/cpu0/cpuidle/`
(no state* dirs), `cat /sys/devices/system/cpu/cpu0/cpufreq/scaling_governor`
(`performance`), `cat .../stats/time_in_state` (all time at 1.846 GHz).

## Open

- Does a `schedutil` switch measurably drop idle frequency? (needs the
  device lock; the sysfs file is group-writable by design)
- `idle-states` DT nodes for WFI-retention/deeper states — worth a DTS
  patch once the boot-test path works.
