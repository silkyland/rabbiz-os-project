# rabbiz-shell — Android-like launcher DE for Galaxy A21s (SM-A217F)

Stack: **labwc** (wlroots compositor, layer-shell + XWayland) + **Quickshell 0.3.2**
(QML shell surfaces) + **native apps** (real xdg toplevels — the in-house apps are
pure-Rust Wayland binaries in `tools/rabbiz-apps/`; the QML app tree was removed
2026-11-20). Target:
Exynos 850 / 2.8GB RAM / Mali-G52 Panfrost (GLES only — `QSG_RHI_BACKEND=opengl`).

## Architecture

```
rabbiz-session.service ─ labwc (compositor)
  ├─ quickshell -p /usr/share/rabbiz-shell/shell.qml   (the shell, one process)
  │    ├─ StatusBar   WlrLayer.Top     44px, exclusive zone
  │    ├─ DesktopView  WlrLayer.Background  ~/Desktop: wallpaper+icons+dock+drawer
  │    ├─ Shade       WlrLayer.Top     pull-down notifications+quick settings
  │    └─ NavBar      WlrLayer.Top     bottom 48px gesture strip
  ├─ app windows (native Wayland / XWayland) — labwc window-rule: always maximize,
  │    no decorations → Android-style fullscreen single-tasking
  └─ steam → `gamescope -- steam -gamepadui` nested in a labwc window
```

## Modules (`QML_IMPORT_PATH=/usr/share/rabbiz-shell`)

- `Rabbiz.Theme` — singleton, ALL colors/type/spacing/radii/motion tokens.
  No hardcoded colors or pixel magic numbers outside Theme.qml.
- `Rabbiz.Services` — singletons: `ShellState` (UI state), `Network` (nmcli),
  `Bluetooth` (bluetoothctl), `Brightness` (logind busctl), `Telephony`
  (ModemManager→demo fallback). **Contracts are frozen — extend, don't rename.**

## Apps

The in-house app family is native Rust (`tools/rabbiz-apps/crates/`, binaries
`rabbiz-<app>` installed to `/usr/local/bin/` by `scripts/de_apps_deploy.sh`) —
no QML apps remain under `shell/`. Third-party apps run as normal Wayland/X11
toplevels.

## `~/Desktop` (home) icon surface

- `Rabbiz.Desktop.DesktopLayout` (singleton) owns which apps are on the desktop grid and in
  the dock, their order, and per-icon label/glyph overrides. It persists to
  `~/.config/rabbiz-shell/desktop.json`; ordering rules are plain JS in `IconLayout.js`
  (`shell/tests/iconlayout.test.mjs`).
- `DesktopView.qml` is the `~/Desktop` surface (grid + hero deck + search + dock). Tap an
  icon to launch it, **long-press (or right-click) for its menu** (open / rename & icon /
  move to dock or desktop / add app / remove), **hold and drag to move** it — the tile lifts
  into a ghost, the source slot becomes a hole, and the drop target is previewed (grid cell
  outline or dock caret). The **+** tile at the end of the grid and in the dock adds an
  installed app (`AppPicker`).
- Overrides are scoped to the home surfaces: the drawer still shows the real app name/icon.
- `quickshell ... ipc call desktop state|menu|action|edit|applyEdit|picker|pick|move|add|remove|reset|dragStart|dragTo|dragDrop`
  drives the same paths for automation.

## Backend rules

- Hardware calls go through `Quickshell.Io.Process` (nmcli, wpctl, bluetoothctl,
  busctl) or built-in services: `Quickshell.Services.Pipewire` (volume),
  `Quickshell.Services.UPower` (battery), `Quickshell.Services.Notifications`
  (notification server — implement the server in-shell),
  `Quickshell.DesktopEntries` is in base `Quickshell` module (installed apps),
  `Quickshell.Wayland.ToplevelManager` (task switcher).
- Wi-Fi via NetworkManager only. NEVER touch `usb0`, never enable
  `wifi.scan-rand-mac-address`.
- Modem does not exist yet (M6) — Telephony must work fully in demoMode.
- Brightness/ battery may report unavailable until M5 drivers land — UI must
  degrade gracefully, never crash on missing backends.

## Layout on device (`rootfs-overlay/` mirrors this)

| Repo path | Device path |
|---|---|
| `shell/` | `/usr/share/rabbiz-shell/` |
| `rootfs-overlay/` | `/` (scp -r overlay/*) |
| app binaries (`tools/rabbiz-apps`) | `/usr/local/bin/rabbiz-<app>` (via `scripts/de_apps_deploy.sh`) |

Deploy: `./scripts/deploy_shell.sh` (scp to root@172.16.42.1).

## Ground rules (from AGENTS.md)

- Never overwrite `/lib` on device; deploy shell files under `/usr/share` only.
- Keep `steamos-session.service` intact — phone session is a parallel unit;
  switching sessions must not break Steam.
