---
status: hypothesis
lane: kernel
device: samsung/a21s
kernel: 7.3.0-g14a425b3a7dc-dirty
author: devin-kernel-agent
ai_assisted: true
date: 2026-11-18
evidence:
  - "cpuidle-psci.c: arm,psci-suspend-param required; states without it are skipped"
  - "linux-arm-kernel Aug 2026, Klimov: cpuidle patches exist, not upstreamable — 'needs some argument shift in one of the PSCI calls due to messed up MPIDRs' (private, not posted)"
  - "volume tree has no exynos850-pmu.c; generic exynos-pmu.c knows pmu_intr_gen only"
related:
  - devices/samsung/a21s/findings/2026-11-18-thermal-cpuidle-cpufreq-state.md
  - devices/samsung/a21s/kernel/patches/0011-a21s-cpu-idle-states.patch
supersedes: []
---

# cpuidle: MPIDR quirk must be reverse-engineered in-house

Owner decision 2026-11-18: do NOT wait on Klimov's private patch —
reverse-engineer the vendor firmware's CPU_SUSPEND MPIDR convention
ourselves.

## Facts

- `enable-method = "psci"`, PSCIv1.1 present; CPUs boot via PSCI CPU_ON —
  so SMC conduit works; only CPU_SUSPEND MPIDR semantics are unknown.
- cpucl0 = MPIDR 0x0–0x3, cpucl1 = 0x100–0x103 (aff1 = cluster).
- `cpuidle-psci` skips idle-states without `arm,psci-suspend-param`;
  architectural WFI works today regardless.
- DTS half already prepared: patch 0011 (marked DO-NOT-FLASH until the
  quirk lands).
- `exynos850-pmu.c` (v7 series, CPU hotplug via `pmu_intr_gen`) is NOT
  in our tree — needed for hotplug, not strictly for idle entry.

## RE plan (cheapest first)

1. **Vendor kernel source** — pull SM-A217F kernel tarball from
   opensource.samsung.com; grep for `CPU_SUSPEND`/`cpu_suspend`/MPIDR
   handling in `exynos-pm`, `cpuidle-exynos`, AIT/ACPM suspend paths.
   The vendor kernel very likely encodes the expected argument already —
   this may answer it without touching the device.
2. **psci_checker** — `drivers/firmware/psci/psci_checker.c` exists in
   the tree; it exercises CPU_SUSPEND/hotplug cycles with the *standard*
   MPIDR. Cheap first probe: build it, run it under the device lock, see
   whether it reports NOT_SUPPORTED (benign) or wedges a CPU (the "messed
   up MPIDR" case).
3. **Argument candidates** — small test module issuing `CPU_SUSPEND`
   from a secondary CPU with candidate MPIDR encodings: standard aff
   form (0x100+), flat index (cpu4 → 4), cluster-shifted forms. One CPU
   at a time, under lock, expecting possible core hang → watchdog reset;
   no serial console, so success signal = kernel log line after wake.
4. **Backport** `exynos850-pmu` v7 series separately for hotplug once
   idle works.

## Risks

- Wrong MPIDR may suspend/resume the *wrong* core or never return —
  treat every test as a possible forced-reboot; run under the device
  lock with the session aware.
- Deep-idle entry races with S2MPU12 panel rails / ABOX power domains —
  verify screen + audio survive before enabling the state by default.
