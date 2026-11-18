# Contributing to RabbizOS

RabbizOS is a mobile Linux system (Arch Linux ARM + mainline kernel + its own desktop
environment). It runs on real phones, so a change that "looks right" in a diff can still
brick a device. These rules exist to keep contributions — from people and from AI agents —
checkable. Read [AGENTS.md](AGENTS.md) first if you are an agent.

## 1. Pick a lane

Work is split by **lane** (what you change) and **device** (where it runs).
See [docs/LANES.md](docs/LANES.md) for each lane's rules and gates.

| Lane | What it covers | Needs hardware? |
|---|---|---|
| `de` | `rabbiz-de` desktop environment, shell, apps, OSK, design tokens | No — simulator + golden tests on host |
| `kernel` | drivers, DTS, defconfig, bootloader/boot image, patches | Yes, or attach device logs |
| `userland` | rootfs overlay, systemd units, services, scripts | Usually yes |
| `docs` | docs and findings only | No |

**No device? Work in the `de` lane.** It is built to be developed and verified on a host.

## 2. Evidence levels (mandatory in every PR)

State the highest level you actually reached. Never claim a higher one.

| Level | Meaning | Proof required |
|---|---|---|
| `L0 host-tested` | `./scripts/check.sh` passes | paste the tail of the output |
| `L1 built` | kernel/module/binary builds for the target | build command + result |
| `L2 on-device` | ran on a real device | device id (`devices/<vendor>/<codename>`), kernel `uname -r`, the log |
| `L3 reproduced` | a second person/agent reproduced L2 | link to their finding |

If you had no device, write **"Not tested on device"**. That is an acceptable, honest PR.
A claim of on-device success without a log will be treated as unverified.

## 3. Findings

Anything you learn about hardware or behaviour that is not obvious from the code goes in a
**finding** — one file per topic, never a shared log. Format and rules:
[docs/FINDINGS.md](docs/FINDINGS.md). A guess is a `hypothesis`; only reproducible
evidence earns `verified`. Refuted findings are kept (marked `refuted`) so nobody retries them.

## 4. Hardware safety (all lanes, all devices)

- Never write to bootloader, modem, or vendor partitions unless the device's recovery path is
  documented in `devices/<vendor>/<codename>/` and you cite it.
- Never print, log, or commit secrets, Wi-Fi passwords, tokens, serial numbers, IMEIs, or MAC
  addresses of real hardware. Use placeholders (`<wifi-ip>`, `<controller-mac>`).
- Never run destructive commands on a rootfs (`tar -C /`, `scp -r` over `/lib`) — see the
  device's `AGENTS.md`.
- Device-specific rules live with the device. Do not copy them into `core` code as if they
  were universal.

## 5. Pull requests

- One lane per PR. One concern per PR.
- Fill in the PR template (lane, device, evidence level, findings touched).
- Add the trailer `AI-Assisted-By: <tool/model>` to commits an AI produced or substantially
  shaped. This is disclosure, not a penalty.
- Run `./scripts/check.sh` before opening the PR.
- Changing a frozen interface (`de-api`, see [docs/DE-SEAMS.md](docs/DE-SEAMS.md)) needs
  maintainer sign-off *before* the code lands.
- Keep existing device paths, service names, and module names stable unless the PR is
  specifically about renaming them.

## 6. Adding a new device

1. Copy `devices/_template/` to `devices/<vendor>/<codename>/` (lowercase; the model number goes
   in `device.toml`, because one codename can cover several model numbers).
2. Fill `device.toml` honestly — unknown fields stay `unknown`, not guessed.
3. Start your own `findings/`. Do not edit another device's findings.
4. Open a PR; you become the listed maintainer of that device directory.

## 7. Licensing

- Original RabbizOS code: Apache-2.0 (see [LICENSE](LICENSE)).
- `drivers/` and kernel patches keep their upstream licence (GPL-2.0 / their SPDX headers);
  see [socs/exynos850/drivers/LICENSING.md](socs/exynos850/drivers/LICENSING.md).
- Bundled fonts and assets keep their own licences (see the `SOURCE.txt` next to them).
- By contributing you confirm you have the right to submit the work under these terms.
  Do not submit code copied from sources with an incompatible licence or vendor firmware blobs.
