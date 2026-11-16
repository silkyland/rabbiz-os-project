# A21s audio and charging inspection — 2026-11-16

Repository: f34acf7. Device reached over USB at root@172.16.42.1.
Running kernel: 7.3.0-g14a425b3a7dc-dirty #2, built Nov 16 10:12:08 UTC.
User reports no speaker sound and no charging after latest flash. Exact flashed
artifact and last working build are unknown. No device state was changed.

## Read-only reproduction

From repository root:

```sh
ssh root@172.16.42.1 'sh -s' < scripts/diagnose_audio_charging.sh
```

Expected healthy state: Exynos3830 audio card, working SysMMU, and a charger
interface in addition to the battery gauge. This script tests registration,
not acoustic output or physical charging. It exits 1 on the inspected boot:

```text
FAIL: Exynos3830 audio card absent
# CONFIG_EXYNOS_IOMMU is not set
FAIL: no non-battery power supply exposed (not proof of zero physical charge current)
```

## Evidence ledger

0. USB SSH succeeds; Wi-Fi address 192.168.1.142 returns No route to host.
   Hardware path can be inspected remotely; local host cannot exercise the DSP.
1. `/proc/asound/cards` contains only Loopback. All ABOX/codec/amp modules are
   loaded. This rules out simple missing module loading as the full explanation.
2. Boot log: `samsung-abox 14a50000.abox: Unable to alloc iommu domain`;
   machine driver repeatedly reports `Failed to parse cpu DAI for rdma0: -517`.
   ABOX has no driver binding or calliope_version attribute. The sound device
   remains in devices_deferred. No playback test was attempted without a card.
3. Running config explicitly disables CONFIG_EXYNOS_IOMMU; /sys/class/iommu
   is empty and ABOX has no iommu_group. Live DT contains ABOX iommus.
4. SMA1303 is bound at 1-001e, chip version 0x11, playback_init exists there.
   Boot service instead writes 0-001e and logs No such file or directory,
   then returns SUCCESS despite failed initialization.
5. Only s2mu005-fuel-gauge is registered under power_supply. Reads show 38%,
   3790405 uV, current_avg -2305176 uA; current_now fluctuates through
   1953, 5126, -245 uA and status flips Charging/Discharging. These readings
   do not establish a trustworthy physical charge rate.

## Traced mechanisms and competing explanations

VERIFIED audio blocker: drivers/abox/abox.c:3235 allocates an IOMMU paging
domain and aborts probe on failure. In the checked-out kernel,
src/linux/drivers/iommu/iommu.c:2086 returns -ENODEV when dev_has_iommu is
false. CONFIG_EXYNOS_IOMMU is disabled on the running device. The local
configs/a21s_linux_defconfig also lacks an explicit enable. A missing sound
card precedes routing, mute and PipeWire: those cannot explain this probe
failure. Restoring SysMMU support requires a rebuilt kernel, with display
compatibility checked too. Recovery has NOT been tested.

VERIFIED second integration defect: scripts/audio-up.sh:25 hardcodes the old
I2C adapter number. Correct amp binding disproves an absent/dead amp as the
explanation for that path error. This defect alone cannot explain ABOX's
earlier IOMMU failure. Service success is not an audio health check.

VERIFIED charger support gap: patches/linux/0005-a21s-power-modem-dts.patch:38
explicitly omits s2mu106-mfd@3d (charger/MUIC) pending a driver. Patch 0006
only adds fuel-gauge compatibility. No charger supply is registered live.
The fuel gauge's STATUS is inferred from current signs in
src/linux/drivers/power/supply/s2mu005-battery.c:168, not a charger enable
readback. Gauge calibration/register interpretation on S2MU106 remains
unverified; a STATUS of Charging does not prove the charger is controlled.

UNVERIFIED physical no-charge root cause: charger register state, cable/adapter
identification and actual battery current were not measured. No writes to
charger registers, module unloading, service restarts or reflashes performed.

## Minimization and handoff

The audio predicate needs only /proc/asound/cards; removing kernel config
inspection preserves that failure but loses its causal evidence. The power
predicate needs only supply types; removing uevent preserves registration
failure but loses observed gauge behavior. No stress harness is needed.

Next audio acceptance: Exynos3830 card registers after boot, Calliope reports
its version, amp initialization resolves the bound device dynamically, then
bounded-buffer ALSA and PipeWire playback produces audible sound without
DSP aborts. Do not unload exynos3830_aud3004x.

Next charging acceptance: implement and bind S2MU106 charger/MUIC support,
verify cable detection, conservative current limits and temperature handling,
then measure sustained positive battery charge with a known charger and
cross-check fuel-gauge units/calibration. This report proves missing software
interfaces, not a complete reproduction of physical no-charge behavior.
