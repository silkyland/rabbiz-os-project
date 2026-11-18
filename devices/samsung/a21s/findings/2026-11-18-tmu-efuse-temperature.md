---
status: verified
lane: kernel
device: samsung/a21s
kernel: 7.3.0-g14a425b3a7dc-dirty
author: Codex
ai_assisted: true
date: 2026-11-18
evidence:
  - cmd: "cat /sys/bus/platform/devices/10070000.tmu/hwmon/hwmon*/temp*_input"
  - cmd: "timeout 8 taskset -c 0 sha256sum /dev/zero"
supersedes:
  - 2026-11-18-tmu-clocked-register-probe.md
---

# The A21s TMU reports three live temperatures using per-sensor eFuse trim

## Observed

The temporary ARM64 hwmon module bound on the running A21s. The trim
selector specified two-point calibration. The 25°C and 85°C codes were
285/347 for LITTLE0, 286/347 for LITTLE1, and 285/347 for G3D.

| Sample | LITTLE0 (m°C) | LITTLE1 (m°C) | G3D (m°C) |
|---|---:|---:|---:|
| Idle before load | 47258 | 45655 | 43387 |
| During one-core CPU load | 52096 | 47622 | 44354 |
| Six seconds after load | 49193 | 45655 | 43387 |

A separate read-only diagnostic saw CONTROL `0x00009503` and the same
first rising-threshold word, `0x0177016d`, for all three sensors.
Bit 12 of CONTROL was already set before any Linux thermal zone was added.
No trip was triggered in these tests.

On a subsequent boot, the diagnostic read ACPM channel 9, zones 0–2, and
then sampled the corresponding MMIO temperature inputs.

| Sensor | ACPM (°C) | MMIO (°C) |
|---|---:|---:|
| LITTLE0 | 44 | 44.354 |
| LITTLE1 | 43 | 44.672 |
| G3D | 41 | 41.451 |

## Interpretation

The physical sensor IDs (1, 2, 3), packed current-temperature fields,
and 9-bit trim fields follow the Exynos3830 vendor source. The conversion
maps each sensor's eFuse codes to its specified 25°C and 85°C points.
The temperature response agrees with the CPU workload placement.
ACPM and the MMIO conversion agreed within 1.7°C in the sampled range.
Both read the same hardware through different paths. Agreement near idle
does not measure absolute sensor accuracy near the trip temperatures.
[Vendor TMU source](https://api.github.com/repos/samsungexynos850/android_kernel_samsung_exynos850/git/blobs/0552e332bea312d05b9f9fe016c1a08a7c779515) · [Vendor DT](https://api.github.com/repos/samsungexynos850/android_kernel_samsung_exynos850/git/blobs/e076d3152a799d34b2c39338e73998adf9f02902)

## Reproduce

Build the out-of-tree TMU module against the device kernel, acquire the
shared device lock, and load the module temporarily. Read the trim log
and the three hwmon inputs before, during and after a brief one-core
load. Unload the module after collecting the samples.
