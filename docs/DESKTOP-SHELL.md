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
inputs in priority order **touch → mouse/keyboard → gamepad**. **D3** (2026-11-16) Xiaomi Pad 7 Pro is *not planned now*. **D4** Steam is an ordinary app inside the desktop, not a separate session. **D2** replace all home-grown QML apps
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
| M7.5 | Form-factor seam, `handset` only (tablet/desktop deferred, D3) | one `formFactor` property drives window rules |
| M7.6 | Close morph via snapshot (S1) | window shrinks to its icon, fallback works |
| M7.7 | Mouse/keyboard + gamepad navigation; Steam as a normal drawer app (D4) | whole shell usable without touch; Steam launches via the morph path |
| M7.8 | Settings v2 (thin panels) | Wi-Fi/BT/Appearance/Session switch all work from it |

## 5. Spikes (do first on device, each ≤ half a day)

- **S1** Does labwc expose `ext-image-capture-source` / `wlr-screencopy` to Quickshell's `ScreencopyView`? (decides M7.6)
- **S2** Does `plasma-settings` start without plasmashell and which KCMs work? (decides O1 split)
- **S3** `xdg-desktop-portal` + backend: does FileChooser open Dolphin/Index dialogs? Disk cost?
- **S4** Disk budget: Qt app set + removing GTK pieces — measured, not guessed (disk is ~99 % full).
- **S5** Software Qt Quick frame time on the desktop surface with wallpaper + widgets (decides widget scope).

## 6. Questions only the owner / customer can answer

Answered: Pad 7 Pro → not now (D3); Steam → normal app (D4). Consequences:
- `tablet` mode drops out of M7.5 scope; `handset` is the only mode built now. `desktop` mode stays
  only as far as mouse/keyboard + external display are wanted (still open, see below). The form-factor
  seam stays (one `formFactor` property) so adding modes later is cheap.
- Steam = `a21s-steam` (gamescope window in labwc) listed in the drawer like any app, same launch/morph
  path. `steamos-session.service` + `a21s-session-switch` are no longer part of the UX (ADR-8); keep the unit
  file installed as a recovery boot only, and stop investing in session-switch hardening.

Still open:
1. Which widgets matter on the home screen (clock, weather, media, calendar)? Each costs frame time (S5).
2. Is an external display + mouse/keyboard (`desktop` mode) a real use case on the A21s, or touch only?
3. Is losing the Material-You look of our current Settings/Spotify acceptable? Real apps look like KDE.
4. Is Steam launched from a gamescope window good enough (needs perf check), or does it need a dedicated game mode later?

## 7. Risks

| Risk | Mitigation |
|---|---|
| Qt apps under `QT_QUICK_BACKEND=software` are slow | S5; per-app env stays in one place (`a21s-launch`), not per wrapper |
| Disk full blocks installs | S4 before any package wave |
| Overlay stays on screen if app never maps | LaunchFlow timeout (tested on host) |
| Retrofit breaks the booting shell | one module per commit, moves only, device smoke test between |

## 8. Spike results (2026-11-16, device `7.3.0-g14a425b3a7dc-dirty`, labwc 0.20.2, Quickshell 0.3.2)

| Spike | Result | Evidence / what is still unverified |
|---|---|---|
| S4 disk + apps | **All target Qt apps already installed**: `kalk kclock marknote buho index-fm dolphin maui-pix vvave merkuro plasma-settings xdg-desktop-portal(-kde)`. **Not packaged**: `plasma-dialer`, `spacebar` (keep `gnome-calls` + `chatty`). **Disk is the blocker: 377 MB free of 22 G (99 %)**. | `scripts/verify_apps.sh`. Biggest installed packages: `linux-firmware-intel` 414 MiB, `linux-aarch64` 386 MiB, `qt6-webengine` 273 MiB, `gcc` 189 MiB, `llvm-libs` 164 MiB, `webkit2gtk-4.1` 159 MiB, `linux-firmware-nvidia` 153 MiB, `linux-firmware-atheros` 141 MiB. Steam data in `~gamer/.local` = 8.8 G. Nothing removed yet — needs owner OK (see §6). |
| S1 close-morph snapshot | **Likely feasible.** `labwc` binary contains `ext_image_copy_capture` + `ext_foreign_toplevel_image_capture` + `ext_foreign_toplevel_list`; Quickshell contains `ScreencopyView` backed by `ext_image_copy_capture`. | Found by `strings`, **not run**. Next: a 20-line QML test that captures a Kalk toplevel into `ScreencopyView`. |
| S3 portals | `xdg-desktop-portal` + `-gtk` + `-kde` backends are running on the user bus; `labwc-portals.conf` prefers `wlr` (no `xdg-desktop-portal-wlr` installed) with `*` fallback. | FileChooser **not tested**. `XDG_CURRENT_DESKTOP=labwc` (not `a21s-shell` as `labwc/environment` sets — the live session value differs, to investigate in M7.2). |
| S2 plasma-settings | **Inconclusive.** Starts under labwc with no plasmashell, no stderr, 144 MB RSS, ~90 MB RAM drop; the device was on the lock screen so the window could not be seen. | Rerun with the phone unlocked; then list which KCMs open. |
| S5 frame time | Not run. Free RAM ≈ 1.0 GB with the shell up. | Needs the desktop surface (M7.3) or a synthetic QML load. |

Also found: device Wi-Fi (`wlan0`) is `disconnected`; USB is the only link; SSH needs the passphrase-protected key loaded in `ssh-agent`.
