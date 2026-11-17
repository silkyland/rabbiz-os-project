# a21s-shell — Android-like launcher DE for Galaxy A21s (SM-A217F)

Stack: **labwc** (wlroots compositor, layer-shell + XWayland) + **Quickshell 0.3.2**
(QML shell surfaces) + **quickshell-hosted apps** (real xdg toplevels). Target:
Exynos 850 / 2.8GB RAM / Mali-G52 Panfrost (GLES only — `QSG_RHI_BACKEND=opengl`).

## Architecture

```
a21s-phone-session.service ─ labwc (compositor)
  ├─ quickshell -p /usr/share/a21s-shell/shell.qml   (the shell, one process)
  │    ├─ StatusBar   WlrLayer.Top     44px, exclusive zone
  │    ├─ DesktopView  WlrLayer.Background  ~/Desktop: wallpaper+icons+dock+drawer
  │    ├─ Shade       WlrLayer.Top     pull-down notifications+quick settings
  │    └─ NavBar      WlrLayer.Top     bottom 48px gesture strip
  ├─ app windows (quickshell -p / XWayland) — labwc window-rule: always maximize,
  │    no decorations → Android-style fullscreen single-tasking
  └─ steam → `gamescope -- steam -gamepadui` nested in a labwc window
```

## Modules (`QML_IMPORT_PATH=/usr/share/a21s-shell`)

- `A21s.Theme` — singleton, ALL colors/type/spacing/radii/motion tokens.
  No hardcoded colors or pixel magic numbers outside Theme.qml.
- `A21s.Services` — singletons: `ShellState` (UI state), `Network` (nmcli),
  `Bluetooth` (bluetoothctl), `Brightness` (logind busctl), `Telephony`
  (ModemManager→demo fallback). **Contracts are frozen — extend, don't rename.**

## `~/Desktop` (home) icon surface

- `A21s.Desktop.DesktopLayout` (singleton) owns which apps are on the desktop grid and in
  the dock, their order, and per-icon label/glyph overrides. It persists to
  `~/.config/a21s-shell/desktop.json`; ordering rules are plain JS in `IconLayout.js`
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
| `shell/` | `/usr/share/a21s-shell/` |
| `rootfs-overlay/` | `/` (scp -r overlay/*) |
| app launchers | `/usr/local/bin/a21s-<app>` → `QML_IMPORT_PATH=… exec quickshell -p …` |

Deploy: `./scripts/deploy_shell.sh` (scp to root@172.16.42.1).

## Ground rules (from AGENTS.md)

- Never overwrite `/lib` on device; deploy shell files under `/usr/share` only.
- Keep `steamos-session.service` intact — phone session is a parallel unit;
  switching sessions must not break Steam.
