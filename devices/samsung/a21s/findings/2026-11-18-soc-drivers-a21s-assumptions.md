---
status: reproduced
lane: kernel
device: samsung/a21s
kernel: 7.3.0-g14a425b3a7dc-dirty
author: devin-kernel-agent
ai_assisted: true
date: 2026-11-18
evidence:
  - cmd: "grep -rn 'a21s|A217F' socs/exynos850/drivers/ --include='*.c' --include='*.h' --include=Kbuild"
supersedes: []
---

# Ledger of a21s-specific assumptions inside socs/exynos850/drivers

Package 10.3 inventory: which `a21s_*` assumptions live in the shared
Exynos 850 driver tree, and which are actually board data that should
move to `devices/samsung/a21s/` (or DT) versus names that are now
stable ABI.

## Hardcoded board data in shared code (candidates to move)

| Where | Assumption | Suggested fix |
|---|---|---|
| `scsc/include/linux/soc/samsung/exynos-soc.h` | `.unique_id = 0x00A217F00001ULL` — the phone's model number baked into a *shared SoC* header | unique_id belongs in board config, not exynos-soc.h |
| `cpif/cpif.h:20` + `cpif_sbd.c`, `cpif_power.c` | "Physical addresses specific to Exynos 850 / A21s", vendor `a21s_eur_open_w00_r06` SBD tables, "CP_CTRL_S is secure-only **on the A21s**" SMC workaround | split Exynos-850-generic phys map from board SBD tables; gate the SMC path on a board property |
| `sensorhub/a21s_chub.c` | entire driver is board data: hardcoded CHUB mailbox/SRAM/PMU/BAAW phys addresses, firmware name `shub_nacho_a21s.bin`, GIC SPI lookup, no DT binding at all | move phys map + firmware name into DT (`samsung,a21s-chub` compatible) or a board table; rename to exynos850-chub when parameterized |
| `fingerprint/a21s_et5xx.c` | `/dev/esfp0` + `/sys/kernel/a21s_fp/` created unconditionally | keep — see ABI-locked below |
| `vibrator/a21s_vibrator.c` | `/sys/kernel/a21s_vib/` + s2mu106-haptic vendor params | keep ABI; params could move to DT |

## ABI-locked names (do NOT rename — DE contracts)

- `/sys/kernel/a21s_fp/{chipid,status,irq_count}` — documented DE
  fingerprint ABI.
- `/sys/kernel/a21s_vib/duration_ms` — haptic ABI.
- `a21s-*` module names (`a21s-chub`, `a21s-cpif`, `a21s-vibrator`,
  `a21s-et5xx`) — referenced by deploy scripts and load-order docs.
  Renaming modules has no functional benefit and breaks deployment.
- Runtime file names `a21s-saved-brightness`, `a21s-power.cmd` —
  cross-version contracts per the root renaming policy.

## Comment-only references (harmless, keep)

`s2mpu12-regulator.c`, `sma1303.c`, `s2mu106-charger.c`, `cpif_*.c`,
Kbuilds — `a21s` appears only in provenance comments ("adapted for the
A21s port", vendor source citations). No functional assumption; leave.

## Priority

1. `exynos-soc.h` unique_id — smallest real leak of board data into
   shared code.
2. `a21s_chub.c` — the most board-bound driver (no DT binding at all);
   generalizing it doubles as the CHUB debug work in Package 6.
3. `cpif` board tables — defer until M6 modem work begins; separating
   them now would churn untested code.
