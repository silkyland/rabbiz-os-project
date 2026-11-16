# Architecture

> Maintained by deep-plan-ingest from [PLAN.md](PLAN.md) and verified implementation up to commit `cc6eb14`. Last ingested: 2026-11-14.
> Facts carry citations; if a citation no longer matches the code, re-verify before relying on it.

## System overview

`android-arm-linux` boots **Native Mainline Linux (`7.3.0`) + Arch Linux ARM64 + SteamOS (`steam -gamepadui`)** directly on unlocked Samsung Galaxy A21s (`SM-A217F`, Exynos 850 / `S5E3830`) hardware with zero Android runtime.

```text
Samsung S-Boot (BL1/BL2, locked by e-fuse)
  └─▶ /dev/mmcblk0p18 (boot-avb.img: uniLoader + Image + exynos850-a217f.dtb)
        └─▶ Mainline Linux 7.3.0 (exynos850-a217f.dts)
              ├─▶ Display/GPU: decon + dsi (td4150) + Panfrost/PanVK (/dev/dri/renderD128)
              ├─▶ Storage: eMMC (&mmc_0) -> Arch Linux ARM64 rootfs + ZRAM/swap
              ├─▶ USB-C: &usbdrd RNDIS (usb0: 172.16.42.1/24 via systemd-networkd)
              ├─▶ WLBT (SCSC): scsc_platform_mif -> scsc_mx -> scsc_wlan (wlan0) + scsc_bt (hci0)
              └─▶ Userspace: steamos-session -> Weston + steam -gamepadui + NetworkManager (D-Bus)
```

## Components

| Component | Responsibility | Status | Evidence |
|---|---|---|---|
| `src/uniLoader` (`a21s_defconfig`) | Intermediate bootloader bypassing Samsung Android boot quirks | works | [PLAN.md](PLAN.md#L34), [a21s-mainline-boot.md](a21s-mainline-boot.md) |
| `src/linux` (`exynos850-a217f.dts`) | Mainline 7.3.0 kernel + Exynos 850 DTS (`decon`, `dsi`, `gpu`, `usbdrd`, `s2mpu12-pmic`, `wifibt_if`) | works | [PLAN.md](PLAN.md#L35), [a21s-wifi-bluetooth.md](a21s-wifi-bluetooth.md) |
| `drivers/scsc/misc/` & `drivers/scsc/Kbuild` | SCSC Maxwell (`scsc_mx.ko`) + Exynos 850 platform MIF (`scsc_platform_mif.ko`) | works | [drivers/scsc/Kbuild](../drivers/scsc/Kbuild), [mxman.c](../drivers/scsc/misc/mxman.c) |
| `drivers/scsc/wlan/` | SCSC Wi-Fi driver (`scsc_wlan.ko`) exposing `wlan0` / `p2p0` | works | [drivers/scsc/Kbuild](../drivers/scsc/Kbuild), [a21s-wifi-bluetooth.md](a21s-wifi-bluetooth.md) |
| `drivers/scsc/bt/` | SCSC BlueZ H4 Bluetooth driver (`scsc_bt.ko`) exposing `hci0` + `rfkill` | works | [scsc_bluez.c](../drivers/scsc/bt/scsc_bluez.c), [scsc_shm.c](../drivers/scsc/bt/scsc_shm.c), [scsc_bt_module.c](../drivers/scsc/bt/scsc_bt_module.c) |
| `scripts/probe_device.sh` | ADB device inspection script (produced `device-a21s-profile.txt`) | works (setup) | [scripts/probe_device.sh](../scripts/probe_device.sh), [device-a21s-profile.txt](device-a21s-profile.txt) |
| `scripts/prep_android_host.sh`, `setup_panvk_test.sh`, `termux_panvk_run.sh` | Legacy Android/Termux track scripts | obsolete (`R1`) | [PLAN.md](PLAN.md#L28) |

## Data flow

### Boot & Graphics Path
1. Unlocked S-Boot loads `boot-avb.img` from `/dev/mmcblk0p18` (`uniLoader` uncompressed + `--dtb` at offset `0x00000000`).
2. `uniLoader` boots Mainline Linux `7.3.0` with `exynos850-a217f.dtb` (`clk_ignore_unused` in `chosen/bootargs`).
3. Kernel mounts Arch Linux ARM64 rootfs on eMMC (`&mmc_0`) and initializes `decon`/`dsi` (`syna,td4150-panel`), `ovt,td4150` touch, and Mali-G52 (`panfrost`/`panvk`).
4. `systemd` starts `steamos-session.service`, launching Weston (landscape) + `steam -gamepadui`.

### Wireless & Networking Path
1. `/etc/modules-load.d/scsc-wifi.conf` loads `scsc_platform_mif`, `scsc_mx`, `scsc_wlan`, `scsc_bt`.
2. `scsc_platform_mif` maps `wifibt_if` shared memory at `0xb0000000` (`0xc00000`), and `scsc_mx` boots WLBT firmware (`mx140`).
3. `scsc_wlan` registers `wlan0` (`cfg80211`); `NetworkManager` connects `wlan0` (`wifi.scan-rand-mac-address=no`) and exposes `org.freedesktop.NetworkManager` over D-Bus to Steam.
4. `usb0` (`172.16.42.1/24`) stays `unmanaged` in NetworkManager and is served by `systemd-networkd` for USB-C SSH.
5. `scsc_bt` exchanges H4 packets over shared memory ([scsc_shm.c](../drivers/scsc/bt/scsc_shm.c)) and delivers frames to BlueZ via `h4_recv_skb()` ([scsc_bluez.c](../drivers/scsc/bt/scsc_bluez.c)).

## Framework mechanisms in use

| Mechanism | How this project uses it | Source |
|---|---|---|
| `samsungexynos850/uniLoader` | Intermediate bootloader (`a21s_defconfig`) loading `Image` + `dtb` | [PLAN.md](PLAN.md#L34) |
| Linux `cfg80211` + `NetworkManager` D-Bus | Steam `-gamepadui` queries `org.freedesktop.NetworkManager` for Wi-Fi | [a21s-wifi-bluetooth.md](a21s-wifi-bluetooth.md) |
| BlueZ `hci_h4` (`h4_recv_skb`) | Mainline 7.3 H4 RX path replacing vendor `hci_recv_frame` wrapper | [drivers/scsc/bt/scsc_bluez.c](../drivers/scsc/bt/scsc_bluez.c) |
| `systemd-networkd` | Dedicated management of `usb0` (`172.16.42.1/24`) | [a21s-wifi-bluetooth.md](a21s-wifi-bluetooth.md) |

## Shell seams (`shell/`, M7 v2 — plan in [DESKTOP-SHELL.md](DESKTOP-SHELL.md))

Dependency direction is enforced by `scripts/check_shell_seams.mjs` (run `scripts/check.sh`).
`shell.qml` is wiring only and may import anything; nothing imports it.

| Module (`shell/A21s/<X>`) | Owns | May import | Status |
|---|---|---|---|
| `Theme` | design tokens, derived from the appearance source of truth | — | exists |
| `Services` | singletons over system backends (NM, BlueZ, UPower, audio, telephony, Appearance writer); no UI | `Theme` | exists |
| `Windowing` | launch flow, morph overlay, task switcher, app gesture layer, window policy by form factor | `Theme`, `Services`, `Widgets` | exists (M7.0 done) |
| `Widgets` | small shared controls (tile, slider, icon) | `Theme`, `Services` | exists (M7.0 done) |
| `Desktop` | wallpaper, icon grid, widgets, dock, app drawer | `Theme`, `Services`, `Widgets`, `Windowing` | exists (`HomeScreen`, `AppDrawer`) |
| `Panels` | status bar, nav/gesture bar, shade, quick settings, power menu, HUD, notifications UI | `Theme`, `Services`, `Widgets`, `Windowing` | exists |
| `shell/apps/*` | **legacy** home-grown apps, deleted in M7.4 (Dialer/SMS last, after M6) | `Theme`, `Services`, `Widgets`, `Windowing` | exists, shrinking |

Not checked (stated gap): `Loader { source: }` / `Qt.createComponent` paths and runtime string imports.
Currently none exist in `shell/`. Deferred decisions: widget API, gamepad bridge location, Settings split (spike S2).

## Decision records

### 1. `uniLoader` in `boot.img` as Native Linux Bootloader (2026-11-14, R1)
- **Decision:** Replace Android with `uniLoader` (`a21s_defconfig`) + Mainline Linux in `boot` (`vbmeta` disabled).
- **Why:** Samsung BL1/BL2/S-Boot are e-fuse locked; `uniLoader` gives a clean mainline boot without touching BL1/BL2 ([PLAN.md](PLAN.md#L63-L68)).
- **Rejected:** Android/Termux/proot (`R1` pivot); `u-boot` (`e850-96_defconfig` is for the dev board).

### 2. USB-C RNDIS (`usb0`) as Always-On Recovery/Dev Network (2026-11-14)
- **Decision:** Keep `usb0` (`172.16.42.1/24`) managed by `systemd-networkd` and `unmanaged` in NetworkManager.
- **Why:** Guarantees SSH access even when Wi-Fi is down or reconfiguring ([PLAN.md](PLAN.md#L69-L73)).

### 3. Native Arch Linux ARM64 Rootfs + ZRAM (2026-11-14)
- **Decision:** Run Arch Linux ARM64 on `USERDATA` eMMC with ZRAM + swapfile + Weston/Steam.
- **Why:** 3 GB RAM (`2767544 kB`) requires ZRAM + swap to run Steam + Box64/FEX-Emu ([PLAN.md](PLAN.md#L46)).

### 4. WLBT `clk_ignore_unused` + `0xb0000000` Reserved Memory (2026-11-14)
- **Decision:** Add `clk_ignore_unused` to `chosen/bootargs` and move `wifibt_if` to `0xb0000000` (`0xc00000`).
- **Why:** Prevents `clk_disable_unused()` from gating a WLBT/APM clock that triggers SoC reset on `SM_MSG_START_REQ` ([a21s-wifi-bluetooth.md](a21s-wifi-bluetooth.md)).

### 5. NetworkManager for `wlan0` with `wifi.scan-rand-mac-address=no` (2026-11-14)
- **Decision:** Use `NetworkManager` on `wlan0` instead of `wpa_supplicant@wlan0` + `systemd-networkd`.
- **Why:** Steam UI requires `org.freedesktop.NetworkManager` D-Bus; disabling MAC randomization prevents `scsc_wlan` `WPS-CANCEL` during scan/connect.

### 6. Settings stays shell-owned; every other base app is a real Linux app (2026-11-16)
- **Decision:** Replace Calculator/Files/Notes/Clock/Gallery/Music with Qt/KDE-first real apps; keep Settings as thin shell-owned panels over NM/BlueZ/portal/gsettings. Dialer/SMS stay ours until M6.
- **Why:** Settings edits state the shell owns (appearance contract, Wi-Fi/BT panel, session switch); `plasma-settings` KCMs expect Plasma daemons we do not run (unverified — spike S2). Disk ~99 % full forbids carrying two toolkits.
- **Rejected:** replacing Settings outright (breaks the contract); GTK/Qt mix (disk).

### 8. Steam is an ordinary desktop app, not a session — **BLOCKED, interim = session hand-over** (2026-11-16)
- **Decision (D4):** Steam should be launched like any app. **Evidence from the device (2026-11-16) that it cannot be yet:**
  1. Steam on labwc's XWayland dies right after start: `DRM_IOCTL_MODE_CREATE_DUMB failed: Permission denied` / `MESA: Failed to create scanout resource` (Mesa opens `card0` while labwc holds DRM master).
  2. `a21s-steam --gamescope` (nested): `vulkan: failed to find physical device` / `Failed to create backend` (panvk is non-conformant; gamescope needs a real Vulkan device).
  3. The shim `steam` / `steam.desktop` currently run `a21s-session-switch steamos`, which stops the phone session; killing that session by hand left labwc up with no usable display until a reboot.
- **Interim rule:** keep the verified session hand-over (`a21s-session-switch steamos`) behind the normal Steam icon; the Launcher shows the morph and times out after 8 s. Do not run `steamos-session` from a shell/SSH while the phone session owns the seat.
- **Next experiments (not done):** force the render node for Mesa under Xwayland (`MESA_LOADER_DRIVER_OVERRIDE`, `DRI_PRIME`, render-node-only EGL); nested `weston --backend=wayland` as a labwc client with Steam inside; `LIBGL_ALWAYS_SOFTWARE` as a last resort for the launcher UI only.
- **Supersedes the "parallel session" text only once** one of those works; `shell/README.md` still describes the parallel session and is correct today.

### 7. One shell, window policy per form factor (2026-11-16)
- **Decision:** `handset` maximizes, `tablet`/`desktop` float (only `handset` built now; Pad 7 Pro deferred); the shell and apps are identical across modes.
- **Why:** 720x1600 cannot host floating windows; Pad 7 Pro (phase 2) can. Rejected: separate phone/desktop shells.

## Known constraints and sharp edges

- **3 GB RAM ceiling (`2767544 kB`)**: Requires ZRAM + swapfile ([PLAN.md](PLAN.md#L46)).
- **`/lib -> usr/lib` symlink on Arch ARM**: Never copy directories to `/lib/` on target; always use `/usr/lib/modules/...`.
- **`src/linux/arch/arm64/boot/dts/exynos/exynos850-a217f.dts`** lives inside gitignored `src/`; keep DTS changes documented in `docs/a21s-wifi-bluetooth.md`.
- **Empty `/sys/class/power_supply/` and `/sys/class/thermal/`**: Battery (`s2mu106`) and Exynos TMU thermal drivers are not ported yet ([a21s-power-battery-modem-plan.md](a21s-power-battery-modem-plan.md)).
