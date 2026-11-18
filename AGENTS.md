# AGENTS.md

Project identity: **RabbizOS**, an Arch Linux ARM–based mobile Linux system by Devded. Mascot: **Rabbiz**; macOS companion: **Rabbiz Mirror**. See [docs/BRAND.md](docs/BRAND.md). Keep existing device paths, service names and module names stable when editing branding. Rule for names: code that is shared across devices uses `rabbiz-*`; code tied to this phone's hardware (DRM keeper, modem, power/volume keys, Panfrost/CMA GPU wrappers, telemetry) keeps `a21s-*`. Renamed 2026-11-18: `a21s-osk`, `a21s-appearance`, `a21s-screenshot`, `a21s-wifi-connect` → `rabbiz-*` (autostart falls back to a legacy `a21s-osk` binary; the runtime file `a21s-screenshot.state` keeps its name). Renamed with the rabbiz-de cut-over (2026-11-18): `a21s-session`/`a21s-phone-session`/`a21s-shell`/`a21s-splash`/`a21s-spotify-ctl`/base-app launchers/`a21s-*.desktop`/icons/polkit rules → `rabbiz-*`; `a21s-phone-session.service` → `rabbiz-session.service` (`Alias=` keeps the old name); QML modules `A21s`/`A21sKit` → `Rabbiz`/`RabbizKit`; `tools/a21s-mirror`/`A21sMirror` → `rabbiz-mirror`/`RabbizMirror`; `~/.config/a21s-shell` migrates to `~/.config/rabbiz-shell` on session start; `A21s.colors` → `Rabbiz.colors`; persisted `desktop.json` ids and layer-shell namespaces (`a21s-*`) keep working via alias lists — runtime file names (`a21s-power.cmd`, `a21s-key-combo`, `a21s-saved-brightness`, `a21s-screenshot.state`) stay `a21s-*` as cross-version contracts. Renamed 2026-11-18: the default session user `gamer` (ChimeraOS-era name) → `rabbiz`, home `/home/rabbiz`; the on-device account still needs `usermod -l rabbiz gamer && usermod -d /home/rabbiz -m rabbiz` (or a fresh rootfs) on next deploy.

Native Mainline Linux (`7.3.0`) + Arch Linux ARM64 + SteamOS (`steam -gamepadui`) port for Samsung Galaxy A21s (`SM-A217F`, Exynos 850) and planned Phase 2 Xiaomi Pad 7 Pro (`muyu`). This is **bare-metal Native Linux**, NOT Android/Termux/proot (`docs/PLAN.md`).

## Contributing, lanes and findings

Read [CONTRIBUTING.md](CONTRIBUTING.md) before changing anything. Work is split by **lane**
(`de`, `kernel`, `userland`, `docs` — [docs/LANES.md](docs/LANES.md)) and **device**
(`devices/<vendor>/<codename>/`). Every PR states its lane, device and evidence level
(L0 host-tested … L3 reproduced); never claim on-device success without a log. Record learnings as
one-file **findings** ([docs/FINDINGS.md](docs/FINDINGS.md)), never in a shared log. Ground rules
are split by scope: OS-wide rules below; per-device rules live in
`devices/<vendor>/<codename>/AGENTS.md` and apply to that device only.

## Ground rules

Device-specific ground rules live in the device's own AGENTS.md — for the
A21s see [devices/samsung/a21s/AGENTS.md](devices/samsung/a21s/AGENTS.md)
(the previous A21s bullet list moved there on 2026-11-18; every rule cites
its finding or doc). Rules that apply OS-wide:

- **Never leak secrets** — `~/.credentail` on the host holds `WIFI_PASSWORD`; never print or commit it. Same for serials, IMEIs, real MAC addresses and tokens.
- **`src/` and `build/` are gitignored** (`.gitignore:2-3`) — tracked driver code lives in `socs/exynos850/drivers/`.

## Common wrong assumptions

- **"Use Termux / proot / Android underneath"** is wrong: `R1` pivoted to bootloader replacement via `uniLoader` (`docs/PLAN.md:51`); this is bare-metal Native Linux on every supported device.
- **Never restyle a third-party app** — RabbizOS names/artwork are for our own apps only. Third-party entries keep the name and icon their package ships (Chrome = "Google Chrome" + Chrome logo, KDE apps keep theirs); wrappers (`rabbiz-browser`, `rabbiz-files`, `rabbiz-calendar`, `rabbiz-contacts`) are fallbacks in `AppCatalog.js`, not replacements.
- Device-level wrong assumptions (vendor driver ports, ABOX buffer sizes, Wi-Fi stack choice) live in [devices/samsung/a21s/AGENTS.md](devices/samsung/a21s/AGENTS.md).

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
