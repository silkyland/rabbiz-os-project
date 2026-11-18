---
status: verified
lane: kernel
device: samsung/a21s
kernel: 7.3.0-g14a425b3a7dc-dirty
author: Codex
ai_assisted: true
date: 2026-11-18
evidence:
  - cmd: "a21s-qt-gpu qml6 <90-second-variant-of-Probe.qml>"
  - cmd: "cat /sys/class/thermal/thermal_zone*/{type,mode,temp,trip_point_0_temp}"
supersedes: []
---

# Panfrost ran the Qt blur probe for 90 seconds with live TMU monitoring

## Observed

A temporary module registered three polling Linux thermal zones using the
same eFuse decoder as the hwmon driver. The zones all reported enabled
with a 95°C critical trip. The test separately stopped at 70°C or below
96 MiB free CMA. The running compositor was `rabbiz-de`.

| Measurement | Result |
|---|---|
| Qt OpenGL renderer | Mali-G52 MC1 (Panfrost), Mesa 26.2.6 |
| Animation callbacks | 5,401 in 90 seconds |
| Peak LITTLE0 / LITTLE1 / G3D | 51.129 / 51.557 / 47.258°C |
| CMA free before / minimum / six seconds after | 242,744 / 224,580 / 238,200 KiB |
| Matching new kernel fault, import or CMA-allocation errors | 0 |
| Probe exit | 0, boot unchanged |

## Interpretation

The Qt blur workload completed below the test cutoff. Callback counts
measure Qt animation activity. This run did not exercise a trip crossing,
CPU cooling or sustained GPU throttling. The capture in this run failed
because the test supplied a literal shell variable in the output path.
A 10-second repeat with the corrected path saved a 384,974-byte PNG
and reported 601 callbacks with the same Panfrost renderer.

The tracked DT patch describes the same 95°C critical trips and 80°C
CPU passive trips. The 90-second test used temporary software zones on
the existing boot. The final tracked module was also loaded on that boot
and returned all three hwmon readings. Both modules were removed after
the tests. The new DTB compiled with no new warnings and awaits a boot test.

## Reproduce

Boot a DTB with the A21s thermal zones and load the matching TMU module,
or register temporary polling zones with the same temperature callback.
Take the device lock, verify all three critical trips and sample the
temperatures and CMA every two seconds. Run the Qt scanout probe with
its quit timer set to 90 seconds. Compare the matching kernel fault count
before and after and check that the boot did not change.
