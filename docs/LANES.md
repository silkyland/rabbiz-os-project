# Lanes

A **lane** is a kind of work with its own rules and its own gate. A **device** is where the
work runs. Every change and every finding names both (`device: any` when it is device-independent).

## `de` — desktop environment

Scope: `tools/rabbiz-de/`, `shell/` (legacy QML, being replaced), `apps/`, `tools/a21s-osk/`,
design tokens and assets.

- Develops and verifies on a host: `./scripts/de_test_host.sh` (cargo tests + golden images),
  `de-sim` for visual runs. No hardware required.
- `de-api` is frozen for v1 ([DE-SEAMS.md](DE-SEAMS.md)): changes need integration-lane
  sign-off before they land. `de-api` depends on nothing but `std` + `serde`.
- Device facts (panel size, DPI, form factor) must come from the device profile
  (`devices/<vendor>/<codename>/device.toml`), not be hard-coded in DE crates.
- A gate that needs a real device and was not run is recorded as **deferred**, never as passed.
- The DE must respect kernel-lane constraints listed in the device's `AGENTS.md` (for example
  the DRM keeper / display-pin requirements on the A21s). Those are *inputs* to the DE, not DE
  knowledge.

## `kernel` — drivers, DTS, boot

Scope: `drivers/`, `socs/*`, `patches/`, `configs/`, DTS, boot image packing.

- Rules that are device-specific (clock gates, memory carve-outs, I2C wiring) live in the
  device's `AGENTS.md` and `findings/`. SoC-wide rules live in `socs/<soc>/`.
- Minimum evidence: L1 (builds). Anything that changes boot behaviour needs L2 with a boot log.
- Never change boot-critical DTS properties (see the device `AGENTS.md`) without citing the
  finding that proves it safe.

## `userland` — rootfs, services, scripts

Scope: `rootfs-overlay/`, systemd units, `scripts/`.

- Serialize device deploys when several people share one device (`docs/DEVICE-QUEUE.md`).
  If you own your device, you do not need the shared lock.
- Never hardcode bus numbers or adapter indexes; glob `/sys/bus/...` instead.

## `docs` — documentation and findings

Scope: `docs/`, `findings/`, `devices/*/findings/`. No code changes. Same evidence rules as findings.

## Where each lane lives

| Path | Lane | Notes |
|---|---|---|
| `tools/rabbiz-de/`, `tools/a21s-osk/`, `tools/a21s-mirror/`, `shell/`, `apps/`, `assets/` | `de` | `shell/` is legacy QML, being replaced by `rabbiz-de` |
| `socs/<soc>/drivers/` | `kernel` | shared by every device on that SoC |
| `devices/<vendor>/<codename>/kernel/` | `kernel` | defconfig, DTS patches for one device |
| `devices/<vendor>/<codename>/docs/`, `findings/` | `docs` | hardware notes and evidence for one device |
| `rootfs-overlay/`, `scripts/`, `patches/mesa`, `patches/squeekboard` | `userland` | still mixes generic and A21s-specific files |
| `docs/` | `docs` | shared, device-independent documents |

`rootfs-overlay/` and `scripts/` have **not** been split per device yet: many files there are
A21s-only (charger, audio, display-pin units, udev rules) but the deploy scripts and systemd
units reference them by path. Splitting them is a separate, device-verified change.

## Interfaces between lanes

| From → To | Contract |
|---|---|
| kernel → DE | device profile (`device.toml`) + the "DE must…" section of the device `AGENTS.md` |
| DE → kernel | open a finding describing the observed kernel-side symptom; do not patch kernel code in a `de` PR |
| any → device | findings with `device:` set; a finding about another device is a different file |
