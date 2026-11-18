---
status: verified
lane: kernel
device: samsung/a21s
kernel: 7.3.0-g14a425b3a7dc-dirty
author: devin-kernel-agent
ai_assisted: true
date: 2026-11-18
evidence:
  - cmd: "cat /sys/class/power_supply/s2mu005-fuel-gauge/{voltage_avg,voltage_now,current_avg}"
  - file: devices/samsung/a21s/findings/logs/2026-11-18-s2mu106-charger-online-props.txt
supersedes: []
related:
  - devices/samsung/a21s/docs/a21s-modem-power-findings.md
---

# S2MU106 fuel gauge: instantaneous reads correct, MONOUT averages bogus

## Observed

On kernel `7.3.0-g14a425b3a7dc-dirty` (#12) with `samsung,s2mu106-fuel-gauge`
bound to `s2mu005_fg_desc` (patch 0006, first revision):

- `voltage_now` ≈ 4.34–4.35 V, plausible and live.
- `current_now` signed and noisy around 0 at DONE/top-off (±4 mA), positive
  (~60 mA) while charging after reboot — sign convention matches
  "positive = charging".
- `capacity` = 100 (battery genuinely full).
- `voltage_avg` = **244 µV** (constant garbage, ~4 orders too small).
- `current_avg` = **−2305176 µA, byte-identical on every read** — a pinned
  bogus value, not a measurement.
- `status` flickers Full↔Discharging on sub-5 mA noise because it derives
  from instantaneous current.

Root cause (confirmed independently upstream): the S2MU106 fuel gauge
shares the direct STATUS/RVBAT/RCURCC/RSOC registers and scaling with
S2MU005 but **not the MONOUT average-monitoring path**. linux-pm series
`20261116074000.355176` (v1, Adil Hüsrev Tepe, verified on XCover5 /
Exynos850 — second device, same chip) adds a per-variant descriptor:
S2MU106 exposes only `voltage_now`, `current_now`, `capacity`, `status`,
and derives status from the instantaneous current alone; the alert IRQ
becomes optional.

Backported into `devices/samsung/a21s/kernel/patches/
0006-s2mu106-fuelgauge-compatible.patch` (squashed upstream 2/3+3/3).
With it the psy renames `s2mu005-fuel-gauge` → **`s2mu106-fuel-gauge`**
(upower object `battery_s2mu106_fuel_gauge`) — the DE must use the new
name once the containing kernel is flashed.

## Interpretation

Direct-register reads (capacity/voltage/instantaneous current) are
trustworthy on the A21s; the two `*_avg` properties were never real.
Temperature is **not** exposed at all — the in-tree driver has no
TEMPERATURE property; the DE should treat battery temperature as
unavailable (Package 1.2 remainder).

## Reproduce

On the device: `cat /sys/class/power_supply/s2mu005-fuel-gauge/{voltage_avg,current_avg}`
twice — fixed bogus values on the pre-backport kernel. After the next
kernel flash, the node renames and the avg files disappear.

## Not yet verified

- The backport itself on-device: the container-built Image (clang 19)
  kexec'd to a black screen (see `2026-11-18-kexec-container-kernel-black.md`),
  so the reduced descriptor is L1 until the next proper flash.
- Whether SOC% tracks a real discharge curve (long-window sampler running;
  see `logs/`).
