# Roadmap

> Source of truth for sequencing. Detail lives in [docs/PLAN.md](docs/PLAN.md) and [docs/a21s-power-battery-modem-plan.md](docs/a21s-power-battery-modem-plan.md); architecture in [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md). Last updated: 2026-11-15.

## Status

| Milestone | Name | Status | Effort | Depends on |
|---|---|---|---|---|
| M0 (R1) | Bootloader Unlock & Host Toolchain Setup (`SM-A217F`) | done | S | — |
| M1 (R1) | Build & Boot `uniLoader` + Mainline Linux (`7.3.0`) | done | M | M0 (R1) |
| M2 (R1) | Native Rootfs, `decon`/`dsi` Display & `Panfrost`/`PanVK` | done | L | M1 (R1) |
| M3 (R1) | Native SteamOS ARM64 (`steam -gamepadui` + Box64/Weston) | done | L | M2 (R1) |
| M3.5 | SCSC Wi-Fi (`wlan0`), Bluetooth (`hci0`) & NetworkManager | done | L | M3 (R1) |
| M3.6 | Internal Speaker Audio (`abox` Calliope DSP, `sma1303`/`aud3004x`, PipeWire/UCM) | wip | L | M3.5 |
| M4 (R1) | Phase 2 — Xiaomi Pad 7 Pro (`muyu` / `SM8635`) `Zaviya` Port | todo | L | M3 (R1) |
| M5 | Power, Battery (`s2mu106`), Thermal (`TMU`) & Suspend | todo | M | M3.5 |
| M6 | Cellular Modem (`cpif` + `pmu-cp`) & Voice Call (`oFono`) | todo | L | M5 |
| M7 | `a21s-shell` Android-style DE (`labwc` + `quickshell` QML: launcher, shade, dialer/SMS, settings) | wip | L | M3.5 |

## Milestone details

### M0 (R1)–M3 (R1): A21s Native Linux + SteamOS Bring-up — done
- **Evidence:** Commits `0a4c0b5`, `a9d3c96`, `26c2bfc`; `docs/a21s-pit.txt`, `docs/a21s-mainline-boot.md`, `docs/a21s-native-gpu.md`, `docs/a21s-game-results.md`.
- **Notes (2026-11-14):** Steam Client runs in Weston landscape mode via `steamos-session.service`.

### M3.5: SCSC Wi-Fi, Bluetooth & NetworkManager — done
- **Objective:** Enable onboard WLBT Wi-Fi (`wlan0`), Bluetooth (`hci0`), and Steam UI network detection.
- **Exit criteria:** `wlan0` connects via `NetworkManager`, Steam detects Wi-Fi over D-Bus, SSH over Wi-Fi works, `hci0` registers cleanly.
- **Tasks:**
  - [x] Fix `SM_MSG_START_REQ` SoC reset (`clk_ignore_unused` + `wifibt_if` at `0xb0000000` in `exynos850-a217f.dts`).
  - [x] Port `drivers/scsc/bt/` (`scsc_bt.ko`) to Mainline 7.3 (`h4_recv_skb`, `scsc_bt_copy_{to,from}_buf`, `mxman_stop` guard) — commit `cc6eb14`.
  - [x] Configure `NetworkManager` (`unmanaged` `usb0`, `wifi.scan-rand-mac-address=no`) and verify Steam `client_networkmanager.txt` `Activated/Connected`.
  - [x] Enable kernel HID stack (`CONFIG_UHID`, `CONFIG_HIDRAW`, `CONFIG_INPUT_JOYDEV`, `CONFIG_HID_{MICROSOFT,PLAYSTATION,NINTENDO,STEAM}`, `CONFIG_JOYSTICK_XPAD`), rebuild/flash `boot-avb.img`, and remove `xbox-ble-uinput` stopgap — [docs/a21s-ble-gamepad.md](docs/a21s-ble-gamepad.md).
- **Notes (2026-11-14):** `p2p0` still appears as `disconnected` in `nmcli`; harmless so far, can be marked `unmanaged` if needed. BlueZ `5.88` + PipeWire/WirePlumber userspace installed; Xbox Wireless Controller bonded and works through the `uinput` bridge stopgap.

### M3.6: Internal Speaker Audio — wip
- **Objective:** Play music/Steam audio through the built-in speaker via the ABOX Calliope DSP, exposed to PipeWire as the default sink.
- **Exit criteria:** `aplay -D plughw:1,100` and PipeWire/Steam both produce clean audio on a good boot, repeatably, without wedging the DSP.
- **Detail:** [docs/a21s-audio-abox-plan.md](docs/a21s-audio-abox-plan.md); current handoff in [docs/HANDOFF.md](docs/HANDOFF.md) §0–§0.1.
- **Tasks:**
  - [x] Port ABOX to Mainline 7.3 (`drivers/abox/`), DT fragment, `calliope_*` firmware, topology (`RC#1–#11`).
  - [x] Speaker + earpiece playback verified once (music heard) — `abox_ion.c` `dma_get_sgtable()` fix.
  - [x] `s2mpu12` PMIC rails → `sma1303` amp powered and responding.
  - [x] UCM2 for `Exynos3830-Madera`, default sink persisted, `abox-audio.service`.
  - [ ] **Fix bug #5: playback `EIO` ~1 s after trigger, DSP `data abort` on a clean boot** (blocks everything else).
  - [ ] Resolve `BUFFER_BYTES_MAX` (1 MiB) vs 768 KiB DSP heap mismatch; keep PipeWire from probing calliope PCMs with defaults.
  - [ ] Resolve `ASRC3` contradiction between `abox_speaker.sh` and the documented recipe; stop `audio-up.sh` hiding `set -e` failures.
  - [ ] Make Calliope boot reliable (flaky `BOOT_DONE`; software reboot sometimes leaves the host offline → manual power cycle).

### M4 (R1): Xiaomi Pad 7 Pro (`muyu` / `SM8635`) — todo
- **Objective:** Unlock HyperOS bootloader on `muyu` (`ac9c8ddc`) and port Native SteamOS ARM64 (`Zaviya`).
- **Detail:** See [docs/muyu-stock-backup-and-plan.md](docs/muyu-stock-backup-and-plan.md).
- **Tasks:**
  - [x] Stock Fastboot ROM (`OS3.0.303.0.WOYMIXM`) + ADB `device-info` backed up locally (`backups/xiaomi-pad-7-pro/`, gitignored).
  - [ ] `M4-R1-T1`: Submit HyperOS bootloader unlock via Xiaomi Community and unlock `muyu` via `fastboot`.
  - [ ] `M4-R1-T2`: Port/adapt `hashtagbasit/Zaviya` kernel/DTB/Turnip stack for `SM8635`.

### M5: Power, Battery (`s2mu106`), Thermal & Suspend — todo
- **Objective:** Reduce idle power, expose battery/charging to Steam (`upower`), enable thermal throttling and sleep/wake.
- **Detail:** See [docs/a21s-power-battery-modem-plan.md](docs/a21s-power-battery-modem-plan.md).
- **Tasks:**
  - [ ] Switch `CONFIG_CPU_FREQ_DEFAULT_GOV_PERFORMANCE=y` to `schedutil` and verify `cpuidle`.
  - [ ] Port `s2mu106` MFD, Fuel Gauge, and Charger (`power_supply` class) + install `upower`.
  - [ ] Enable Exynos TMU thermal zones (`/sys/class/thermal/`), backlight control, and Suspend-to-RAM.

### M6: Cellular Modem (`cpif`) & Voice Call — todo
- **Objective:** Enable Exynos 850 Baseband (`CP`) for SIM, 4G Data, SMS, and Voice Calls.
- **Detail:** See [docs/a21s-power-battery-modem-plan.md](docs/a21s-power-battery-modem-plan.md).
- **Tasks:**
  - [ ] Port `drivers/soc/samsung/cpif/` + `pmu-cp.c` + reserve `cp_shmem` without colliding with `0xb0000000`.
  - [ ] Integrate `libsamsung-ipc` / `oFono` + ABOX/Bluetooth HFP voice audio routing.

### M7: `a21s-shell` Android-style DE — wip
- **Objective:** Replace the Steam-only session with an Android-like launcher: homescreen/app drawer, pull-down notification+quick-settings shade, dialer, SMS, settings — all QML on `labwc`+`quickshell`, Steam stays launchable via `gamescope`.
- **Layout:** `shell/` (QML, modules `A21s.Theme`/`A21s.Services`/`A21s.Components`), `rootfs-overlay/` (systemd unit, labwc config, wrappers, .desktop), `scripts/deploy_shell.sh`. Details: `docs/a21s-phone-shell.md`, `shell/README.md`.
- **Verified on host:** `quickshell 0.3.2` + sway-headless load of `shell.qml`, `apps/{dialer,sms,settings}` all reach `Configuration Loaded`. **Not yet on-device** — needs `deploy_shell.sh` + enable `a21s-phone-session.service`.
- **Tasks:**
  - [x] Skeleton + design tokens + service contracts (lead)
  - [x] Session/compositor/deploy infra + labwc config (W1)
  - [x] HomeScreen + AppDrawer + TaskSwitcher + NavBar gestures (W2)
  - [x] Shade + QuickSettings + Network/Bluetooth/Brightness services + Settings app (W3)
  - [x] Dialer app + Telephony facade (demo mode) (W4)
  - [x] SMS app (W5)
  - [x] StatusBar + notification daemon/shade list (W6)
  - [ ] On-device smoke test: install packages, enable session, verify touch/shade/launch
  - [ ] Real telephony backend when M6 lands (`Telephony.backend` hook / `mm-backend.sh` reference)

## Open questions

- [ ] Which exact clock gate requires `clk_ignore_unused` for WLBT (`SM_MSG_START_REQ`)? — resolve by auditing `exynos850` clock tree / `scsc_platform_mif` clock requests so `clk_ignore_unused` can be removed.
- [ ] Does `decon`/`dsi` + `scsc_mx` survive Suspend-to-RAM (`echo mem > /sys/power/state`) cleanly? — resolve by testing suspend/resume in `M5`.

## Risks

| Risk | Likelihood | Impact | Mitigation / rollback |
|---|---|---|---|
| `clk_ignore_unused` keeps unused SoC clocks powered | high | medium | Temporary until WLBT clock gate is explicitly claimed (`M5`). |
| No thermal throttling (`/sys/class/thermal/` empty) under load | medium | high | Avoid sustained heavy 3D load until `TMU` is enabled (`M5`). |
| `cp_shmem` memory reservation overlaps `wifibt_if` (`0xb0000000`) | medium | high | Audit vendor memory map before enabling `cpif` (`M6`). |
| Stock `boot.img` / `vbmeta` restore needed | low | medium | Keep stock `SM-A217F` `AP` tar ready for Heimdall/Odin. |
