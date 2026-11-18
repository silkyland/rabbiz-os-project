# Kernel-lane handoff — evening of 2026-11-18

Self-contained state for continuing the kernel lane without the agent.
Owner plan: cpuidle MPIDR RE tonight (per
[findings/2026-11-18-cpuidle-mpidr-re-plan.md](../findings/2026-11-18-cpuidle-mpidr-re-plan.md)).

## Access

- `ssh root@172.16.42.1` over usb0 (reliable). Wi-Fi `192.168.50.6` is
  usually down — don't depend on it.
- Anything that mutates the device or runs a test goes through
  `scripts/device_session.sh` (remote flock
  `/run/lock/a21s-device-test.lock`); the DE agent shares it.
  `DEVICE_LOCK_WAIT=<s>` env bumps the wait.
- Device user for the session/desktop: `rabbiz` (uid 1001).

## Current device state (verified today)

- Running kernel `#5` (7.3.0-g14a425b3a7dc-dirty, clang 19.1.7 build),
  flashed to `mmcblk0p18` with patches 0006+0009+0010.
- Thermal zones live + passive cooling **verified**: 80 °C trips bound
  to cpufreq-cpu0/cpu4, 95 °C criticals; a lowered 70 °C trip made the
  governor drop 1846→1742 MHz. `exynos850-tmu.ko` installed under
  `/usr/lib/modules/$(uname -r)/extra/` + autoloads via
  `/etc/modules-load.d/exynos850-tmu.conf`.
- Fuel gauge is `s2mu106-fuel-gauge` (renamed), bogus `*_avg` gone;
  UPower object `battery_s2mu106_fuel_gauge`.
- cpuidle: zero registered states (WFI only) — tonight's task.
- cpufreq governor is `performance` (userspace-set, udev gives `video`
  group write). `schedutil` is built-in and available.

## Tonight: cpuidle MPIDR RE (ordered)

1. **Vendor source first (no device needed).** Download SM-A217F kernel
   source from opensource.samsung.com; grep for `CPU_SUSPEND`,
   `cpu_suspend`, MPIDR usage in `exynos-pm`/AIT/ACPM paths. The vendor
   tree probably encodes the expected argument — may answer everything.
2. **psci_checker probe (device, ~2 min).**
   `drivers/firmware/psci/psci_checker.c` in the build tree; build as
   module like other `socs/` drivers (`make ... M=<dir> modules` in the
   `a21s-kbuild` container), deploy to
   `/usr/lib/modules/$(uname -r)/extra/`, `depmod -a`, insmod under the
   lock. Watch dmesg: NOT_SUPPORTED = benign; CPU wedge = the "messed up
   MPIDR" case (expect a possible watchdog reset).
3. **Candidate-argument test module** — only if 1–2 don't resolve it.
   Issue `arm_smccc_smc(CPU_SUSPEND, state, entry, ctx)` from a
   secondary CPU with candidate MPIDRs (standard aff, flat index
   cpu4→4, shifted forms). One core at a time; assume forced reboot is
   possible; no serial console, so log before the call.
4. When a working encoding is found: write the quirk patch, then patch
   `0011-a21s-cpu-idle-states.patch` (already written, dry-run-clean)
   becomes flashable. **Do not flash 0011 alone.**
5. `exynos850-pmu` v7 backport is a separate later task (hotplug, not
   required for idle entry).

## Pending kernel queue (after cpuidle)

- **DSI acceptance**: 10× compositor stop/start on kernel #5 to prove
  patch 0010 recovers the underrun wedge. Kills the DE session while
  running — coordinate.
- **Suspend**: `echo mem` vetoed by `samsung-abox` runtime-suspend
  `-EAGAIN` (driver-internal usage_count=1, no PCM users). Find/fix the
  ref in `socs/exynos850/drivers/abox/`. Panel also wants missing
  firmware `td4150_a21s.bin`.
- **ET520 fingerprint ID probe**, **CHUB A/B** (sensors still report
  ENODATA — real presence unknown), **mic capture probe**.

## Safety / environment notes

- `/lib` is a symlink to `usr/lib` — deploy modules under
  `/usr/lib/modules/$(uname -r)/`, never copy a `/lib` tree.
- Never `rmmod exynos3830_aud3004x` — remove path oopses; reboot instead.
- Keep `clk_ignore_unused` + `wifibt_if` in the dts (WLBT reset otherwise).
- Kernel build tree lives in Docker volume `a21s-kbuild` (`/work/linux`,
  out `/work/out`); image `rabbizos-kbuild:usb-investigation`.
  `mkbootimg` installs via `apt-get` in the container (network OK).
- Boot image recipe + recovery runbook:
  [a21s-flash-boot-checklist.md](a21s-flash-boot-checklist.md).
  Backups: `/root/p18-pre-flash.img` on device,
  `/tmp/a21s-p18-backup.img` on this Mac (md5 `24a89598…`). Heimdall is
  NOT installed on this Mac — recovery path is dd-back-over-ssh or
  source-build heimdall.
- kexec of a locally-built kernel black-screens on this device (kexec
  path itself is broken) — flash, don't kexec.
- Left on device (harmless): `/root/boot-avb-new.img`,
  `/root/p18-pre-flash.img`, `/tmp/a21s-chub.ko` staging leftovers may
  have been wiped by reboot.
