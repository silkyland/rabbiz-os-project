# Repo-Wide Project Structure Audit Handoff (2026-11-19)

Scope: Full RabbizOS repository outside `tools/rabbiz-de` and `tools/rabbiz-osk`/`tools/rabbiz-apps` (see companion audits [HANDOFF-de-audit-2026-11-19.md](HANDOFF-de-audit-2026-11-19.md) and [HANDOFF-osk-apps-audit-2026-11-19.md](HANDOFF-osk-apps-audit-2026-11-19.md)).  
Evidence level: **L0 (static audit)**. **Not tested on device.**

---

## 1. What Works Well

- **Multi-device layout (`devices/`)**: Clean separation across `devices/_template/`, `devices/samsung/a21s/`, and `devices/xiaomi/muyu/`, each with its own `AGENTS.md`, `device.toml`, `docs/`, `findings/`, and kernel/scripts dirs.
- **SoC driver isolation (`socs/`)**: Exynos 850 out-of-tree drivers live cleanly in `socs/exynos850/drivers/` with explicit `LICENSING.md`, separate from device configs.
- **Upstream userland patches (`patches/`)**: `patches/mesa/` and `patches/squeekboard/` are cleanly separated from kernel patches (`devices/samsung/a21s/kernel/patches/`).
- **Automated host checks**: `scripts/check_findings.mjs`, `scripts/check_links.mjs`, and `scripts/check_shell_seams.mjs` enforce doc/link/seam hygiene.

---

## 2. Structural Findings

### P1 — `rootfs-overlay/` mixes OS-wide configs with A21s hardware services
`rootfs-overlay/` currently combines OS-wide files (`rabbiz-session.service`, `rabbiz-splash.service`, `50-rabbiz-session.rules`, `mimeapps.list`) with A21s-specific hardware services, udev rules, and helper scripts:
- `rootfs-overlay/etc/systemd/system/`: `a21s-display-pin.service`, `a21s-drm-keeper.service`, `a21s-modem-init.service`, `abox-audio.service`, `s2mu106-charger.service`
- `rootfs-overlay/etc/udev/rules.d/`: `90-a21s-backlight.rules`, `99-a21s-modem-fp.rules`, `99-a21s-sensors-vib.rules`
- `rootfs-overlay/usr/local/bin/`: `a21s-*` wrappers alongside `rabbiz-*` wrappers

**Recommendation**: Split into OS-wide `rootfs-overlay/` + device-specific `devices/samsung/a21s/rootfs-overlay/` before bringing up a second device (`muyu` or others).

### P1 — Three generations of apps & two generations of shells coexist
- **Shells**: Legacy Quickshell/QML occupies top-level `shell/`, while the active pure-Rust compositor/shell lives under `tools/rabbiz-de/`. [AGENTS.md](../AGENTS.md) still documents `shell/Rabbiz/` as the primary shell layout.
- **Apps**: Three locations exist simultaneously:
  - `shell/apps/` (legacy QML Dialer/SMS/Settings)
  - `apps/` (legacy Qt/RabbizKit QML apps + `tests/*.test.mjs`; note `apps/camera/` is not yet ported to `tools/rabbiz-apps/`)
  - `tools/rabbiz-apps/` (active 12 pure-Rust Wayland apps)

**Recommendation**: Once `rabbiz-de` + `rabbiz-apps` are the default on-device session (and `camera` is ported or intentionally kept), move legacy `shell/` and `apps/` under `legacy/` (or remove superseded ones) and update `AGENTS.md` to reflect the Rust workspace layout.

### P2 — `scripts/` flat sprawl (48 files)
`scripts/` holds 48 flat files mixing:
- Host CI/gates (`check.sh`, `check_*.mjs`, `de_test_host.sh`, `de_golden.sh`)
- DE build/deploy/bench (`de_build.sh`, `de_deploy.sh`, `de_bench.sh`, `de_probe.sh`, `de_m1_check.sh`)
- A21s-only hardware diagnostics (`a21s-power-diag.sh`, `abox_*.sh`, `charger-up.sh`, `cma_stat.sh`, `check_a21s_*.py`, `diagnose_audio_charging.sh`, `setup_panvk_test.sh`, `termux_panvk_run.sh`)
- Legacy Quickshell scripts (`deploy_shell.sh`, `shell_reload.sh`, `shell_perf_compare.sh`, `verify_shell_blur.sh`)
- Systemd unit files (`volume-keys.service`, `xbox-ble-uinput.service`) living inside `scripts/` instead of `rootfs-overlay/etc/systemd/system/`

**Recommendation**: Move A21s-only diagnostic scripts into `devices/samsung/a21s/scripts/` (matching `devices/xiaomi/muyu/scripts/`), move `.service` files to `rootfs-overlay/`, and group remaining scripts into `scripts/ci/` and `scripts/de/`.

### P3 — Minor hygiene
- Removed empty stray `tools/tools/rabbiz-de/` directory (2026-11-19).
- Ensure `__pycache__/` under `scripts/` and `rootfs-overlay/usr/local/bin/` stays ignored in `.gitignore`.

---

## 3. Master Audit Map (All 3 Handoffs)

| Scope | Handoff | Status |
|---|---|---|
| `tools/rabbiz-de` (compositor & shell) | [HANDOFF-de-audit-2026-11-19.md](HANDOFF-de-audit-2026-11-19.md) | Tier 0.1/0.2, Tier 1.3–1.6, Tier 2.8 (`idle_inhibit` + `ext-session-lock-v1`), Tier 3.13 (`pointer_constraints`) landed & L1-tested |
| `tools/rabbiz-osk` & `tools/rabbiz-apps` | [HANDOFF-osk-apps-audit-2026-11-19.md](HANDOFF-osk-apps-audit-2026-11-19.md) | `rabbiz-osk` lints/hygiene done; `rabbiz-apps` macOS host tests unlocked (308 passed) |
| Repo-wide (`rootfs-overlay/`, `scripts/`, `shell/`, `apps/`, `devices/`) | This document | Audited; cleanup backlog ready |
