# AGENTS.md

Project identity: **RabbizOS**, an Arch Linux ARM–based mobile Linux system by Devded. Mascot: **Rabbiz**; macOS companion: **Rabbiz Mirror**. See [docs/BRAND.md](docs/BRAND.md). Keep existing device paths, service names and module names stable when editing branding. Rule for names: code that is shared across devices uses `rabbiz-*`; code tied to this phone's hardware (DRM keeper, modem, power/volume keys, Panfrost/CMA GPU wrappers, telemetry) keeps `a21s-*`. Renamed 2026-11-18: `a21s-osk`, `a21s-appearance`, `a21s-screenshot`, `a21s-wifi-connect` → `rabbiz-*` (autostart falls back to a legacy `a21s-osk` binary; the runtime file `a21s-screenshot.state` keeps its name). Renamed with the rabbiz-de cut-over (2026-11-18): `a21s-session`/`a21s-phone-session`/`a21s-shell`/`a21s-splash`/`a21s-spotify-ctl`/base-app launchers/`a21s-*.desktop`/icons/polkit rules → `rabbiz-*`; `a21s-phone-session.service` → `rabbiz-session.service` (`Alias=` keeps the old name); QML modules `A21s`/`A21sKit` → `Rabbiz`/`RabbizKit`; `tools/a21s-mirror`/`A21sMirror` → `rabbiz-mirror`/`RabbizMirror`; `~/.config/a21s-shell` migrates to `~/.config/rabbiz-shell` on session start; `A21s.colors` → `Rabbiz.colors`; persisted `desktop.json` ids and layer-shell namespaces (`a21s-*`) keep working via alias lists — runtime file names (`a21s-power.cmd`, `a21s-key-combo`, `a21s-saved-brightness`, `a21s-screenshot.state`) stay `a21s-*` as cross-version contracts. Renamed 2026-11-18: the default session user `gamer` (ChimeraOS-era name) → `rabbiz`, home `/home/rabbiz`; the on-device account still needs `usermod -l rabbiz gamer && usermod -d /home/rabbiz -m rabbiz` (or a fresh rootfs) on next deploy.

Native Mainline Linux (`7.3.0`) + Arch Linux ARM64 + SteamOS (`steam -gamepadui`) port for Samsung Galaxy A21s (`SM-A217F`, Exynos 850) and planned Phase 2 Xiaomi Pad 7 Pro (`muyu`). This is **bare-metal Native Linux**, NOT Android/Termux/proot (`docs/PLAN.md`).

## Contributing, lanes and findings

Read [CONTRIBUTING.md](CONTRIBUTING.md) before changing anything. Work is split by **lane**
(`de`, `kernel`, `userland`, `docs` — [docs/LANES.md](docs/LANES.md)) and **device**
(`devices/<vendor>/<codename>/`). Every PR states its lane, device and evidence level
(L0 host-tested … L3 reproduced); never claim on-device success without a log. Record learnings as
one-file **findings** ([docs/FINDINGS.md](docs/FINDINGS.md)), never in a shared log. The ground
rules below are written from the Samsung A21s (`devices/samsung/a21s/`); on another device treat
them as hypotheses until that device has its own verified findings.

## Ground rules

- **Never overwrite `/lib` on the A21s rootfs** — `/lib` is a symlink to `usr/lib` on Arch Linux ARM; `scp -r` or `tar` extracting a `/lib/` directory breaks boot. Always deploy modules to `/usr/lib/modules/$(uname -r)/`.
- **Keep `usb0` (`172.16.42.1/24`) unmanaged in NetworkManager** — `systemd-networkd` owns `usb0` (`devices/samsung/a21s/docs/a21s-wifi-bluetooth.md`); NetworkManager manages `wlan0` with `wifi.scan-rand-mac-address=no`.
- **Never leak secrets** — `~/.credentail` on the host holds `WIFI_PASSWORD`; never print or commit it.
- **Keep `clk_ignore_unused` in `exynos850-a217f.dts` `chosen/bootargs`** — `clk_disable_unused()` disables a WLBT/APM clock gate, causing an immediate SoC reset on `SM_MSG_START_REQ` (`devices/samsung/a21s/docs/a21s-wifi-bluetooth.md`).
- **Keep `wifibt_if` at `0xb0000000` (`0xc00000`)** in `exynos850-a217f.dts` — avoids colliding with kernel/initramfs memory (`devices/samsung/a21s/docs/a21s-wifi-bluetooth.md`).
- **Uncompressed `uniLoader` + `--dtb` packing** — pack `build/images/boot-avb.img` using `mkbootimg` with `--dtb src/tools/AIK-Linux/split_img/boot.img-dtb --dtb_offset 0x00000000` and uncompressed `src/uniLoader/uniLoader`.
- **`src/` and `build/` are gitignored** (`.gitignore:2-3`) — tracked driver code lives in `socs/exynos850/drivers/scsc/`.
- **Keep boot-on S2MPU12 rails `always_on`** in `socs/exynos850/drivers/abox/s2mpu12-regulator.c` until panel supplies are wired in DTS — otherwise `regulator_late_cleanup()` disables `vdd_ldo28` (lcd_vddi) at ~31 s and the screen goes black (`devices/samsung/a21s/docs/a21s-mainline-boot.md`).
- **Never `rmmod exynos3830_aud3004x`** — `aud3004x_jack_remove` oopses; reboot instead (`devices/samsung/a21s/docs/a21s-audio-abox-plan.md` §0).
- **Probe `i2c-0`/`0x3d` (S2MU106) with plain `write+read`, not userspace SMBus ioctl** — `I2C_SMBUS` byte reads return unstable garbage and cost real debugging time; kernel `i2c_smbus_*` is fine (`devices/samsung/a21s/docs/HANDOFF.md` §0.7).
- **Charge path lives in `s2mu106-charger.ko`, not DT** — instantiated via `i2c-0/new_device` by `s2mu106-charger.service`/`scripts/charger-up.sh`; the chip boots in BUCK mode so `CTRL0` must be set to `CHG` every boot. A real MFD/MUIC/PDIC port is still open (`devices/samsung/a21s/docs/HANDOFF.md` §0.7).
- **Never hardcode `i2c-N` adapter paths** — enabling/disabling any I2C bus in DTS renumbers adapters (fuel-gauge `i2c@13830000` moved SMA1303 from `0-001e` to `1-001e` on kernel #12 → silent amp). Glob `/sys/bus/i2c/drivers/<drv>/*/` instead; card presence does NOT prove `playback_init` ran (`devices/samsung/a21s/docs/HANDOFF.md` §0.6).
- **Never let a client request ≥768 KiB on a `calliope_*` PCM** — the Calliope buffer heap is 768 KiB (`786432` B) while ALSA advertises `BUFFER_BYTES_MAX = SZ_1M` (`socs/exynos850/drivers/abox/abox.h:41`). The oversized request is refused *inside* the DSP (`Not enough memory: 1048576 of 786432`) and wedges it. WirePlumber does exactly this when it probes calliope PCMs with ALSA defaults (`devices/samsung/a21s/docs/HANDOFF.md` §0.1).
- **Shell-launched Qt Quick apps stay software via `Appearance.appEnvironment` (`RABBIZ_FORCE_SOFTWARE=1`); keep `QT_QUICK_BACKEND=software` as the session default** — direct GPU diagnostics may use `a21s-qt-render` (GPU while CMA ≥ 96 MiB, else software) — labwc must run with `dumb-destroy.so` preloaded (fixes the CMA leak, `devices/samsung/a21s/docs/FINDINGS-hw-render.md` F2) — without it every GPU app leaks ~14 MiB and exhausting CMA segfaults the shell. Original reason: stock Mesa buffers fail exynos-drm import. The shell entry point `rabbiz-shell` uses `a21s-qt-gpu` with a software fallback; app launches must use `Appearance.appEnvironment` to strip GPU overrides. Explicit `a21s-qt-gpu <command>` is a verified experimental exception: private, checksum-pinned Mesa SCANOUT allocation + primary-node shim enables Panfrost/MultiEffect (`devices/samsung/a21s/docs/a21s-qt-gpu-scanout.md`). Do not enable GPU globally or remove its version guard. Chrome still needs `--disable-gpu`; QtWidgets (Dolphin) is unaffected.
- **Dolphin's phone UI needs `PLASMA_PLATFORM=phone:handset` AND a first-run state wipe** — the phone layout (bottom toolbar, hidden Places, `RenameInline=false`) only applies when `dolphinrc [General] Version < 200` and no `~/.local/state/dolphinstaterc` exists (KDE bug 505864). `rabbiz-files` does the one-time wipe; `QT_QPA_PLATFORMTHEME=kde` + `plasma-integration` + seeded `kdeglobals` give Breeze dark/single-click (`docs/a21s-phone-shell.md`).
- **Only one on-screen keyboard per seat** — `rabbiz-osk` (Rust, `tools/rabbiz-osk/`) is the session keyboard since 2026-11-17; squeekboard is the installed fallback. Never run both: the seat accepts one input method. `rabbiz-osk-switch start|stop|status` (root, on the device) flips between them; the autostart prefers `rabbiz-osk` and falls back to squeekboard when the binary is missing.
- **Keep `QT_IM_MODULE`/`GTK_IM_MODULE` unset** — Qt6 ≥6.7/GTK apps auto-use `zwp_text_input_v3` under labwc; squeekboard (`zwlr_input_method_v2`) then auto-shows on text focus once `org.gnome.desktop.a11y.applications screen-keyboard-enabled=true`. Thai layout `th` ships built-in; switch via the 🌐 key. maliit/plasma-keyboard/ibus are `input-method-v1` → dead on labwc.
- **Never extract archives `-C /` without `--no-same-owner`** — `tar`/`deb` extraction re-applies archive uids (macOS `501`, deb `0`) to `/` and `/home`, transiently breaking `runuser`/`su` for `rabbiz`. `deploy_shell.sh` re-chowns `/`, `/home`, `/home/rabbiz` after every push.

## Common wrong assumptions

- **"Use Termux / proot / Android underneath"** is wrong: `R1` pivoted to bootloader replacement via `uniLoader` (`docs/PLAN.md:51`).
- **"Use `wpa_supplicant` + `systemd-networkd` for Steam Wi-Fi"** is wrong: Steam `-gamepadui` requires `org.freedesktop.NetworkManager` over D-Bus (`devices/samsung/a21s/docs/a21s-wifi-bluetooth.md`).
- **"Vendor `scsc_bt` compiles directly on Mainline 7.3"** is wrong: Mainline removed `hdev->dev_type = HCI_BREDR` and uses `<net/bluetooth/hci_h4.h>` `h4_recv_skb(hdev, NULL, NULL, ...)` (`socs/exynos850/drivers/scsc/bt/scsc_bluez.c`).
- **"Vendor `mxman.c` unregisters cleanly without `recovery_disabled_unreg` check"** is wrong: `mxman_stop()` must guard `if (mif->recovery_disabled_unreg)` (`socs/exynos850/drivers/scsc/misc/mxman.c:2667`).
- **"ABOX noise means wrong I2S format/clock"** can be wrong: cicada/mosquito noise on *both* speakers was a broken `sg_table` (no `struct page`) in the `socs/exynos850/drivers/abox/abox_ion.c` ION shim — build it with `dma_get_sgtable()` (`devices/samsung/a21s/docs/a21s-audio-abox-plan.md` §0).
- **"The 18 MiB `ABOX_DRAM_SIZE=0x1200000` is the DSP's audio buffer space"** is wrong: that is the **host-side** DRAM window (`DRAM_FIRMWARE_SIZE`). The firmware's PCM buffer heap is only **768 KiB** — and hitting it does not return a clean error, it data-aborts the DSP.

- **Never restyle a third-party app** — RabbizOS names/artwork are for our own `a21s-*` apps only. Third-party entries keep the name and icon their package ships (Chrome = "Google Chrome" + Chrome logo, KDE apps keep theirs); wrappers (`rabbiz-browser`, `rabbiz-files`, `rabbiz-calendar`, `rabbiz-contacts`) are fallbacks in `AppCatalog.js`, not replacements.

## Commands

- Concurrent tasks may develop and run host tests in parallel. Serialize device
  deploy/reload/touch/power/reboot and their measurements with
  `scripts/device_session.sh` (remote `/run/lock/a21s-device-test.lock`);
  see `docs/DEVICE-QUEUE.md`. Deploy only your task's files, not another task's
  unfinished tree. This coordination was explicitly authorized by the owner.

- Bootstrap `src/` + `build/` on a fresh machine:
  `./scripts/bootstrap_build_env.sh` (clones `samsungexynos850/linux` @ `14a425b3a` + `uniLoader` @ `06a1bba`, applies `devices/samsung/a21s/kernel/patches/*.patch`, prepares `build/linux` from `devices/samsung/a21s/kernel/a21s_linux_defconfig`)
- Build SCSC modules (`socs/exynos850/drivers/scsc/`):
  `make -C src/linux O=../../build/linux ARCH=arm64 LLVM=1 M=$PWD/socs/exynos850/drivers/scsc modules`
- Build kernel (`Image` + DTBs):
  `make -C src/linux O=../../build/linux ARCH=arm64 LLVM=1 -j$(nproc) Image dtbs`
- Verify device over USB or Wi-Fi:
  `ssh root@172.16.42.1 "uname -a && nmcli device status && hciconfig -a"` (or the device Wi-Fi IP)

## Where knowledge lives

- Build & patch guide: [docs/BUILD.md](docs/BUILD.md)
- Architecture and data flow: [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md)
- Current milestone and what's next: [ROADMAP.md](ROADMAP.md)
- Full research and decisions record: [docs/PLAN.md](docs/PLAN.md)
- Wi-Fi & Bluetooth bring-up details: [devices/samsung/a21s/docs/a21s-wifi-bluetooth.md](devices/samsung/a21s/docs/a21s-wifi-bluetooth.md)
- Battery, power & modem plan: [devices/samsung/a21s/docs/a21s-power-battery-modem-plan.md](devices/samsung/a21s/docs/a21s-power-battery-modem-plan.md)

## Where shell code goes (`shell/Rabbiz/`, checked by `scripts/check.sh`)

| New thing | Put it in | Copy |
|---|---|---|
| Wrapper over a system backend (D-Bus, CLI, file) | `Services/<Concept>.qml` singleton, no UI | `Services/Network.qml` |
| Design token | `Theme/Theme.qml` only; no literal colors elsewhere | `Theme/Theme.qml` |
| App launch / morph / window policy logic | `Windowing/`; state logic in plain `.js` with a node test | `Windowing/LaunchFlow.js` + `tests/launchflow.test.mjs` |
| Home surface (grid, dock, drawer, widgets) | `Desktop/` | `Desktop/DesktopView.qml` |
| Icon layout / per-icon label+glyph (data, not UI) | `Desktop/DesktopLayout.qml` + `Desktop/IconLayout.js` | `Desktop/IconLayout.js` + `shell/tests/iconlayout.test.mjs` |
| Status/nav bar, shade, quick settings, power, HUD | `Panels/` | `Panels/Shade.qml` |

Shared controls: `Widgets/` (`MaterialIcon.qml`). Dependency direction: `Theme ← Services ← Widgets ← Windowing ← Desktop/Panels`; never sideways between Desktop and Panels. Do **not** add apps to `shell/apps/` (legacy, being replaced by real Linux apps — `docs/DESKTOP-SHELL.md`). Anti-homes: `utils`, `helpers`, `common`, `misc`, `_v2` — name the concept. Nothing fits? Propose the module, justify it by dependency direction, add a row and a rule in `scripts/check_shell_seams.mjs`.
Vocabulary: *form factor* = handset/tablet/desktop; *desktop contract* = the appearance/env/portal settings every app obeys; *starting window* = shell-drawn morph overlay until the app's toplevel maps; *~/Desktop* = the home page (icon grid + dock), named like the folder it represents.

Icons on `~/Desktop` and in the dock are **user data**: membership, order and per-icon label/glyph live in `~/.config/rabbiz-shell/desktop.json` through `DesktopLayout` — never re-hardcode a home icon list in a view, and keep the ordering rules in `IconLayout.js` with its node test.
