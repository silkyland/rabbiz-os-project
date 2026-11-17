# AGENTS.md

Native Mainline Linux (`7.3.0`) + Arch Linux ARM64 + SteamOS (`steam -gamepadui`) port for Samsung Galaxy A21s (`SM-A217F`, Exynos 850) and planned Phase 2 Xiaomi Pad 7 Pro (`muyu`). This is **bare-metal Native Linux**, NOT Android/Termux/proot (`docs/PLAN.md`).

## Ground rules

- **Never overwrite `/lib` on the A21s rootfs** — `/lib` is a symlink to `usr/lib` on Arch Linux ARM; `scp -r` or `tar` extracting a `/lib/` directory breaks boot. Always deploy modules to `/usr/lib/modules/$(uname -r)/`.
- **Keep `usb0` (`172.16.42.1/24`) unmanaged in NetworkManager** — `systemd-networkd` owns `usb0` (`docs/a21s-wifi-bluetooth.md`); NetworkManager manages `wlan0` with `wifi.scan-rand-mac-address=no`.
- **Never leak secrets** — `~/.credentail` on the host holds `WIFI_PASSWORD`; never print or commit it.
- **Keep `clk_ignore_unused` in `exynos850-a217f.dts` `chosen/bootargs`** — `clk_disable_unused()` disables a WLBT/APM clock gate, causing an immediate SoC reset on `SM_MSG_START_REQ` (`docs/a21s-wifi-bluetooth.md`).
- **Keep `wifibt_if` at `0xb0000000` (`0xc00000`)** in `exynos850-a217f.dts` — avoids colliding with kernel/initramfs memory (`docs/a21s-wifi-bluetooth.md`).
- **Uncompressed `uniLoader` + `--dtb` packing** — pack `build/images/boot-avb.img` using `mkbootimg` with `--dtb src/tools/AIK-Linux/split_img/boot.img-dtb --dtb_offset 0x00000000` and uncompressed `src/uniLoader/uniLoader`.
- **`src/` and `build/` are gitignored** (`.gitignore:2-3`) — tracked driver code lives in `drivers/scsc/`.
- **Keep boot-on S2MPU12 rails `always_on`** in `drivers/abox/s2mpu12-regulator.c` until panel supplies are wired in DTS — otherwise `regulator_late_cleanup()` disables `vdd_ldo28` (lcd_vddi) at ~31 s and the screen goes black (`docs/a21s-mainline-boot.md`).
- **Never `rmmod exynos3830_aud3004x`** — `aud3004x_jack_remove` oopses; reboot instead (`docs/a21s-audio-abox-plan.md` §0).
- **Probe `i2c-0`/`0x3d` (S2MU106) with plain `write+read`, not userspace SMBus ioctl** — `I2C_SMBUS` byte reads return unstable garbage and cost real debugging time; kernel `i2c_smbus_*` is fine (`docs/HANDOFF.md` §0.7).
- **Charge path lives in `s2mu106-charger.ko`, not DT** — instantiated via `i2c-0/new_device` by `s2mu106-charger.service`/`scripts/charger-up.sh`; the chip boots in BUCK mode so `CTRL0` must be set to `CHG` every boot. A real MFD/MUIC/PDIC port is still open (`docs/HANDOFF.md` §0.7).
- **Never hardcode `i2c-N` adapter paths** — enabling/disabling any I2C bus in DTS renumbers adapters (fuel-gauge `i2c@13830000` moved SMA1303 from `0-001e` to `1-001e` on kernel #12 → silent amp). Glob `/sys/bus/i2c/drivers/<drv>/*/` instead; card presence does NOT prove `playback_init` ran (`docs/HANDOFF.md` §0.6).
- **Never let a client request ≥768 KiB on a `calliope_*` PCM** — the Calliope buffer heap is 768 KiB (`786432` B) while ALSA advertises `BUFFER_BYTES_MAX = SZ_1M` (`drivers/abox/abox.h:41`). The oversized request is refused *inside* the DSP (`Not enough memory: 1048576 of 786432`) and wedges it. WirePlumber does exactly this when it probes calliope PCMs with ALSA defaults (`docs/HANDOFF.md` §0.1).
- **Keep Qt Quick apps software-rendered; GPU is scoped to the shell** — `a21s-shell` uses checksum-gated `a21s-qt-gpu` when CMA has room, with software fallback. Shell app launches must use `Appearance.appEnvironment` (including `A21S_FORCE_SOFTWARE=1`) to strip GPU overrides. Do not enable GPU globally or remove the version guard. See `docs/a21s-qt-gpu-scanout.md`. Chrome keeps `--disable-gpu`; QtWidgets (Dolphin) is unaffected.
- **Dolphin's phone UI needs `PLASMA_PLATFORM=phone:handset` AND a first-run state wipe** — the phone layout (bottom toolbar, hidden Places, `RenameInline=false`) only applies when `dolphinrc [General] Version < 200` and no `~/.local/state/dolphinstaterc` exists (KDE bug 505864). `a21s-files` does the one-time wipe; `QT_QPA_PLATFORMTHEME=kde` + `plasma-integration` + seeded `kdeglobals` give Breeze dark/single-click (`docs/a21s-phone-shell.md`).
- **Keep `QT_IM_MODULE`/`GTK_IM_MODULE` unset** — Qt6 ≥6.7/GTK apps auto-use `zwp_text_input_v3` under labwc; squeekboard (`zwlr_input_method_v2`) then auto-shows on text focus once `org.gnome.desktop.a11y.applications screen-keyboard-enabled=true`. Thai layout `th` ships built-in; switch via the 🌐 key. maliit/plasma-keyboard/ibus are `input-method-v1` → dead on labwc.
- **Never extract archives `-C /` without `--no-same-owner`** — `tar`/`deb` extraction re-applies archive uids (macOS `501`, deb `0`) to `/` and `/home`, transiently breaking `runuser`/`su` for `gamer`. `deploy_shell.sh` re-chowns `/`, `/home`, `/home/gamer` after every push.

## Common wrong assumptions

- **"Use Termux / proot / Android underneath"** is wrong: `R1` pivoted to bootloader replacement via `uniLoader` (`docs/PLAN.md:51`).
- **"Use `wpa_supplicant` + `systemd-networkd` for Steam Wi-Fi"** is wrong: Steam `-gamepadui` requires `org.freedesktop.NetworkManager` over D-Bus (`docs/a21s-wifi-bluetooth.md`).
- **"Vendor `scsc_bt` compiles directly on Mainline 7.3"** is wrong: Mainline removed `hdev->dev_type = HCI_BREDR` and uses `<net/bluetooth/hci_h4.h>` `h4_recv_skb(hdev, NULL, NULL, ...)` (`drivers/scsc/bt/scsc_bluez.c`).
- **"Vendor `mxman.c` unregisters cleanly without `recovery_disabled_unreg` check"** is wrong: `mxman_stop()` must guard `if (mif->recovery_disabled_unreg)` (`drivers/scsc/misc/mxman.c:2667`).
- **"ABOX noise means wrong I2S format/clock"** can be wrong: cicada/mosquito noise on *both* speakers was a broken `sg_table` (no `struct page`) in the `drivers/abox/abox_ion.c` ION shim — build it with `dma_get_sgtable()` (`docs/a21s-audio-abox-plan.md` §0).
- **"The 18 MiB `ABOX_DRAM_SIZE=0x1200000` is the DSP's audio buffer space"** is wrong: that is the **host-side** DRAM window (`DRAM_FIRMWARE_SIZE`). The firmware's PCM buffer heap is only **768 KiB** — and hitting it does not return a clean error, it data-aborts the DSP.

## Commands

- Concurrent tasks may develop and run host tests in parallel. Serialize device
  deploy/reload/touch/power/reboot and their measurements with
  `scripts/device_session.sh` (remote `/run/lock/a21s-device-test.lock`);
  see `docs/DEVICE-QUEUE.md`. Deploy only your task's files, not another task's
  unfinished tree. This coordination was explicitly authorized by the owner.

- Bootstrap `src/` + `build/` on a fresh machine:
  `./scripts/bootstrap_build_env.sh` (clones `samsungexynos850/linux` @ `14a425b3a` + `uniLoader` @ `06a1bba`, applies `patches/linux/*.patch`, prepares `build/linux` from `configs/a21s_linux_defconfig`)
- Build SCSC modules (`drivers/scsc/`):
  `make -C src/linux O=../../build/linux ARCH=arm64 LLVM=1 M=$PWD/drivers/scsc modules`
- Build kernel (`Image` + DTBs):
  `make -C src/linux O=../../build/linux ARCH=arm64 LLVM=1 -j$(nproc) Image dtbs`
- Verify device over USB or Wi-Fi:
  `ssh root@172.16.42.1 "uname -a && nmcli device status && hciconfig -a"` (or `ssh root@192.168.1.142`)

## Where knowledge lives

- Build & patch guide: [docs/BUILD.md](docs/BUILD.md)
- Architecture and data flow: [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md)
- Current milestone and what's next: [ROADMAP.md](ROADMAP.md)
- Full research and decisions record: [docs/PLAN.md](docs/PLAN.md)
- Wi-Fi & Bluetooth bring-up details: [docs/a21s-wifi-bluetooth.md](docs/a21s-wifi-bluetooth.md)
- Battery, power & modem plan: [docs/a21s-power-battery-modem-plan.md](docs/a21s-power-battery-modem-plan.md)

## Where shell code goes (`shell/A21s/`, checked by `scripts/check.sh`)

| New thing | Put it in | Copy |
|---|---|---|
| Wrapper over a system backend (D-Bus, CLI, file) | `Services/<Concept>.qml` singleton, no UI | `Services/Network.qml` |
| Design token | `Theme/Theme.qml` only; no literal colors elsewhere | `Theme/Theme.qml` |
| App launch / morph / window policy logic | `Windowing/`; state logic in plain `.js` with a node test | `Windowing/LaunchFlow.js` + `tests/launchflow.test.mjs` |
| Home surface (grid, dock, drawer, widgets) | `Desktop/` | `Desktop/HomeScreen.qml` |
| Status/nav bar, shade, quick settings, power, HUD | `Panels/` | `Panels/Shade.qml` |

Shared controls: `Widgets/` (`MaterialIcon.qml`). Dependency direction: `Theme ← Services ← Widgets ← Windowing ← Desktop/Panels`; never sideways between Desktop and Panels. Do **not** add apps to `shell/apps/` (legacy, being replaced by real Linux apps — `docs/DESKTOP-SHELL.md`). Anti-homes: `utils`, `helpers`, `common`, `misc`, `_v2` — name the concept. Nothing fits? Propose the module, justify it by dependency direction, add a row and a rule in `scripts/check_shell_seams.mjs`.
Vocabulary: *form factor* = handset/tablet/desktop; *desktop contract* = the appearance/env/portal settings every app obeys; *starting window* = shell-drawn morph overlay until the app's toplevel maps.
