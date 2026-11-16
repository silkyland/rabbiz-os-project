# Desktop shell rewrite — plan (M7 v2)

> Status: **proposed**, branch `feat/desktop-shell-rewrite`. Decisions below marked **D#** were
> answered by the owner on 2026-11-16; everything marked **H#** is a hypothesis to prove on the
> device. Nothing in this file has been run on hardware yet (device was offline when it was written).

## 1. What the owner asked for

1. Rework the launcher: new UI/UX, **morph animations**.
2. **Every app respects our desktop environment** (one look, one set of settings).
3. Base apps are **real Linux apps**, not our own QML.
4. The first screen is a **real desktop**.

Answers: **D1** home = a *touch-first desktop* (wallpaper, icons/widgets, dock, drawer, real windows),
inputs in priority order **touch → mouse/keyboard → gamepad**. **D2** replace all home-grown QML apps
with real Linux apps.

## 2. Where the current code disagrees with the request (evidence)

| # | Claim in the request | Evidence | Counter-proposal |
|---|---|---|---|
| O1 | "Replace *all* base apps" | `Settings.qml` is the only app that edits state the shell itself owns (Wi-Fi/BT panel, appearance, session switch). Every DE ships its own settings app for this reason. `plasma-settings` KCMs expect Plasma daemons (kscreen, powerdevil) we do not run. | Settings stays **shell-owned** but becomes thin panels over standard backends (NM, BlueZ, portal, gsettings). Everything else is replaced. *Spike S2 decides if `plasma-settings` can do part of it.* Recorded as ADR-6. |
| O2 | "Apps open as real windows" on a 720×1600 panel | `rc.xml` maximizes every window today; a floating 600 px window on a 6.5″ screen is unusable. | One window policy per **form factor**: handset = maximize + immersive, tablet/desktop = floating/snap. Same shell, different policy (ADR-7). |
| O3 | "Morph animation for every app" | labwc has no window animation hooks, and an external toplevel's pixels are not ours. `AppLaunchMorph.qml` already morphs an *overlay*, not the app. | Android's approach: a shell-drawn **starting window** (icon → fullscreen surface) that is dismissed when the toplevel maps and draws its first frame; the **close** morph needs a snapshot of the window (spike S1: `ext-image-capture`/`wlr-screencopy` on labwc). Fallback: icon-only close morph. |
| O4 | "Base apps should be Linux apps" (any toolkit) | Disk is ~99 % full, RAM 2.8 GB (`docs/a21s-phone-shell.md`). Qt *and* GTK stacks side by side cost both. | **Qt/KDE-first** app set (Kirigami + Maui): the shell is Qt, `kdeglobals` is already seeded. GTK only where there is no Qt equivalent (Chrome, Steam, Chatty if needed). The Desktop Contract still themes both. |
| O5 | "Rewrite from scratch" | 18.6 k QML lines: ~8.8 k are the apps being deleted; the shell's Services (`Network`, `Bluetooth`, `Telephony`, `SystemControl`) and `Theme` are working, device-verified backends. | Rewrite the **launcher/desktop surface and windowing**; keep and re-home Services/Theme. Do not write a new compositor: keep **labwc** (verified on-device; KWin/Wayfire are heavier/unverified on Mali + 2.8 GB). |

## 3. Target architecture

```
a21s-phone-session.service ─ labwc  (policy: window rules by form factor)
   ├─ quickshell  shell.qml  (wiring only)
   │    A21s.Desktop     wallpaper · icon grid · widgets · dock · app drawer
   │    A21s.Windowing   launch flow · morph overlay · task switcher · window policy
   │    A21s.Panels      status bar · nav/gesture bar · shade · quick settings · power · HUD
   │    A21s.Services    NM · BlueZ · UPower · audio · telephony · Appearance (contract writer)
   │    A21s.Theme       tokens (derived from the appearance source of truth)
   └─ real apps (Qt/KDE-first) as ordinary Wayland toplevels
```

### 3.1 Form factors (**H1**: one shell, three policies)

| Mode | Trigger | Windows | Chrome |
|---|---|---|---|
| `handset` | output < 900 px wide (A21s today) | maximized, no decorations | immersive bars, gesture strip |
| `tablet` | ≥ 900 px (Pad 7 Pro, rotated A21s) | floating/snap, touch title bar | dock + status bar always |
| `desktop` | external output or pointer+keyboard present | floating, resizable, server titlebar | dock + panel, hover states |

### 3.2 Desktop Contract — what "respect our DE" means, concretely

One **appearance source of truth** (accent, dark/light, font, scale, icon theme, wallpaper), written
by `A21s.Services.Appearance` into every channel apps actually read:

| Channel | Read by | Written how |
|---|---|---|
| `org.freedesktop.appearance` portal + `gsettings` (`color-scheme`, `gtk-theme`, `icon-theme`, fonts, cursor) | GTK4/libadwaita, Chrome | `gsettings set` + `xdg-desktop-portal` settings backend |
| `~/.config/kdeglobals` + `QT_QPA_PLATFORMTHEME=kde` | Qt/KDE/Maui apps | rewrite `[General]`/`[Colors:*]` keys |
| `XDG_CURRENT_DESKTOP`, `mimeapps.list` | launchers, `xdg-open` | session env + default-apps writer |
| `org.freedesktop.Notifications` | all apps | already implemented in-shell (`NotificationServer.qml`) |
| `xdg-desktop-portal` FileChooser/OpenURI/Settings | sandboxed + GTK apps | install backend, verify (spike S3) |
| Scale | all | single `QT_SCALE_FACTOR`/`GDK_SCALE` from the shell, not per-wrapper `1.5` hacks |
| Input method | all | unchanged (squeekboard; `QT_IM_MODULE` stays unset — AGENTS.md) |

Today three wrappers each set their own env (`a21s-files` → `QT_SCALE_FACTOR=1.5`, Chrome
→ `--force-device-scale-factor=1.5`, labwc env → `QT_SCALE_FACTOR=1`, `QT_QPA_PLATFORMTHEME=`
empty). That inconsistency is exactly what the contract removes.

### 3.3 Base app set (**H2**: availability to verify with `scripts/verify_apps.sh` on device)

| Role | Target app | Toolkit | Status |
|---|---|---|---|
| Calculator | `kalk` | Qt | wrapper exists, install unverified |
| Files | `index-fm` → `dolphin` (phone UI) | Qt | installed, verified |
| Notes | `marknote` / `buho` | Qt | `buho` installed |
| Clock | `kclock` | Qt | unverified |
| Gallery | `maui-pix` | Qt | installed |
| Music | `vvave` / web Spotify in Chrome | Qt/Chromium | `vvave` installed |
| Calendar/Contacts | `merkuro` | Qt | installed |
| Dialer / SMS | `gnome-calls` + `chatty` now; `plasma-dialer` + `spacebar` if packaged | GTK / Qt | installed; **keep our Dialer/SMS QML until M6 modem works** |
| Settings | shell-owned (see O1) | Qt | rewrite thin |
| Browser | Chrome ARM64 (`a21s-browser`) | Chromium | works |

### 3.4 Morph animation (**H3**)

`ShellState.trigger*Morph` + `AppLaunchMorph.qml` become `Windowing`: a pure **LaunchFlow** state
machine (`idle → launching → mapped → closing`, with a timeout fallback so a crashed app never
leaves a full-screen overlay) driven by `ToplevelManager` events. The QML overlay only renders
`LaunchFlow` state. The state machine is plain JS so it is tested on the host with no Qt.

### 3.5 Input (**H4**)

Touch first. Mouse/keyboard: hover + focus rings, labwc keybinds, titlebars in tablet/desktop mode.
Gamepad: a small uinput bridge maps pad → arrows/Enter/Esc while the shell has focus; Steam session
keeps native pad handling. Shell QML uses `KeyNavigation`/`activeFocus`, never touch-only handlers.

## 4. Milestones (all under ROADMAP M7)

| ID | Goal | Exit criterion |
|---|---|---|
| M7.0 | Retrofit: split `A21s.Components` into Desktop / Windowing / Panels / Widgets; add seam checker | `scripts/check.sh` green; shell still boots on device; move commits contain moves only |
| M7.1 | **Launch slice**: tap dock icon → starting-window morph → real Kalk window → home | works on device; `LaunchFlow` test green on host |
| M7.2 | Desktop Contract v1: Appearance service + env unification + portals | change dark/accent once → Kalk, Dolphin, Chrome all follow (screenshots) |
| M7.3 | Desktop surface: wallpaper, icon grid, widgets, dock, drawer | home screen is a desktop, 60 fps-ish on software Qt Quick (**H5**, measure) |
| M7.4 | Replace base apps (Calc, Files, Notes, Clock, Gallery, Music) and delete their QML | `verify_apps.sh` green; `shell/apps/*` removed except Dialer/SMS |
| M7.5 | Form-factor policy (handset/tablet/desktop) + labwc rules per mode | external display or rotation flips mode without restart |
| M7.6 | Close morph via snapshot (S1) | window shrinks to its icon, fallback works |
| M7.7 | Mouse/keyboard + gamepad navigation | whole shell usable without touch |
| M7.8 | Settings v2 (thin panels) | Wi-Fi/BT/Appearance/Session switch all work from it |

## 5. Spikes (do first on device, each ≤ half a day)

- **S1** Does labwc expose `ext-image-capture-source` / `wlr-screencopy` to Quickshell's `ScreencopyView`? (decides M7.6)
- **S2** Does `plasma-settings` start without plasmashell and which KCMs work? (decides O1 split)
- **S3** `xdg-desktop-portal` + backend: does FileChooser open Dolphin/Index dialogs? Disk cost?
- **S4** Disk budget: Qt app set + removing GTK pieces — measured, not guessed (disk is ~99 % full).
- **S5** Software Qt Quick frame time on the desktop surface with wallpaper + widgets (decides widget scope).

## 6. Questions only the owner / customer can answer

1. Which widgets matter on the home screen (clock, weather, media, calendar)? Each costs frame time (S5).
2. Does Steam stay a separate session switch, or a normal app in the desktop? (Today: both exist.)
3. Is the Pad 7 Pro (phase 2) a hard requirement now? It drives how much `tablet` mode is worth now.
4. Is losing the Material-You look of our current Settings/Spotify acceptable? Real apps look like KDE.

## 7. Risks

| Risk | Mitigation |
|---|---|
| Qt apps under `QT_QUICK_BACKEND=software` are slow | S5; per-app env stays in one place (`a21s-launch`), not per wrapper |
| Disk full blocks installs | S4 before any package wave |
| Overlay stays on screen if app never maps | LaunchFlow timeout (tested on host) |
| Retrofit breaks the booting shell | one module per commit, moves only, device smoke test between |
