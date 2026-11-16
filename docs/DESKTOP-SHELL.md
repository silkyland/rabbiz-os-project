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
inputs in priority order **touch → mouse/keyboard → gamepad**. **D3** (2026-11-16) Xiaomi Pad 7 Pro is *not planned now*. **D4** Steam is an ordinary app inside the desktop, not a separate session (**solved with nested Weston — ADR-8**). **D5** (2026-11-16) we may wrap/override app looks with our own theme, and `A21s.Theme` may be rewritten from scratch. **D2** replace all home-grown QML apps
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
| S2 plasma-settings | **Runs without plasmashell** (screenshot taken, 144 MB RSS, no stderr) with a mobile sidebar: Keyboard, Display, Wallpaper, Colors & Themes, Text & Fonts, Animations, Default Applications, Notifications, Window Management, Screen Locking, Region & Language, Day-Night Cycle, Users, Autostart. **No Wi-Fi, Bluetooth, Network, Sound or Battery pages** (Networking = Online Accounts only). It rendered **light**, ignoring our dark `kdeglobals` because `QT_QPA_PLATFORMTHEME` is empty in the session env — a live proof of why the Desktop Contract is needed. | Confirms ADR-6: appearance/default-apps/keyboard KCMs can be reused; Wi-Fi/BT/sound/battery panels must stay shell-owned. Which KCMs actually *work* when opened (not just listed) is untested. |
| S5 frame time | Not run. Free RAM ≈ 1.0 GB with the shell up. | Needs the desktop surface (M7.3) or a synthetic QML load. |

Also found: device Wi-Fi (`wlan0`) is `disconnected`; USB is the only link; SSH needs the passphrase-protected key loaded in `ssh-agent`.

### Disk action taken (2026-11-16, owner said "do it fully")
`journalctl --vacuum-size=30M` (freed 0 B, journal is on tmpfs-backed `/run`), `pacman -Rdd linux-firmware-{intel,nvidia,atheros}` (PC GPU/Wi-Fi firmware, no Exynos hardware uses them; `mx140`/`calliope` firmware lives in `/usr/lib/firmware` and is untouched). Free space **377 MB → 1.2 GB (95 %)**. Side effect: the pacman hook ran `mkinitcpio` for the stale `linux-aarch64` preset and errored (`/lib/modules/7.3.4-1-aarch64-ARCH` missing) — harmless, our boot uses `boot-avb.img`, but that package is dead weight (386 MiB, required by nothing) — candidate for removal. **Do not remove `llvm-libs`: `mesa` requires it.** `gcc`/`clang` (≈ 190 MiB + clang-libs) are removable.

## 9. Theme decision (D5) and what it changes

Owner: apps that look bad may be themed over, and `Theme.qml` may be rewritten from the ground up.
Evidence for doing it: Plasma Settings rendered light/default on our dark device (§8 S2). Plan for M7.2:

1. **One palette definition** (`shell/A21s/Theme/palette` data: dark + light, accent, radii, motion) is the only source.
2. **Generators** (run by `A21s.Services.Appearance` at session start and on change): a Qt/KDE colour scheme (`~/.local/share/color-schemes/A21s.colors` + `kdeglobals`), GTK 3/4 overrides (`gtk.css` + `gsettings`), cursor/icon/font settings; `QT_QPA_PLATFORMTHEME=kde` set once in the session env (fixes the light render).
3. **Rewrite `Theme.qml`** as a thin reader of that palette (no literal colours), keeping today's token *names* as aliases until M7.0 moves are done, so nothing breaks mid-retrofit.
4. Limits (honest): a colour scheme + Breeze style recolours Qt/KDE apps well; GTK4/libadwaita and Chromium only follow `color-scheme` + accent, not arbitrary shapes. Deep restyling of those is out of scope; per-app CSS only where it is ugly enough to matter (decide per app after M7.4 screenshots).

## 10. Progress log (2026-11-16, all verified on the device unless noted)

| Milestone | State | Evidence |
|---|---|---|
| M7.0 split `Components` | **done** | Widgets/Windowing/Desktop/Panels; shell loads with no errors; home unchanged; seam checker green |
| M7.1 launch slice | **done** | `quickshell ipc call launcher launch kalk` -> starting window -> real Kalk window -> state idle (2-3 s); same for kclock, index, marknote |
| M7.2 desktop contract v1 | **done (apps)** | `a21s-appearance` + `Appearance` service write `A21s.colors`, `kdeglobals [Colors:*]`, gsettings; Kalk/KClock render the dark A21s palette. GTK/Chromium only follow `color-scheme` (not verified visually) |
| M7.3 desktop surface | **not redesigned** | existing HomeScreen (clock, grid, dock, Steam card) kept; widgets/icons-on-desktop not built |
| M7.4 swap base apps | **done** | home/drawer use `org.kde.*`; Calculator/Clock/Notes/Gallery/Files/Spotify QML + wrappers deleted (~4 k lines); Spotify removed entirely (owner request; Widevine is unavailable so the web player could not play anyway). Dialer/SMS/Settings QML kept (D1/ADR-6) |
| M7.5 form factors | not started | only `handset` is in scope (D3) |
| M7.6 close morph | not started | S1 says ScreencopyView is available, untested |
| M7.7 input + Steam | **Big Picture verified** (logged in, online, touch taps land), game launch reaches Proton/FEX but the game renders black | ADR-8; fixed `SDL_VIDEODRIVER=x11` (was black screen); RAM 82 MB free / 547 MB available; teardown <=14 s |
| M7.8 Settings v2 | not started | S2: plasma-settings lacks Wi-Fi/BT/sound/battery |
| M7.10 home editing (drag & drop, align) | not started | `HANDOFF-DESKTOP-SHELL.md` §7.1 |
| M7.11 clipboard system | not started | §7.2; `wl-clipboard`/`cliphist` not installed, labwc has data-control |
| M7.12 on-screen keyboard rework | not started | §7.3; triage squeekboard bugs first |
| Theme v2 (light/dark + glass) | **landed** | `Theme.light` switches the whole palette (bg/surface/card/text/tiles) and the shell chrome follows live; glass tokens (#AARRGGBB) give translucent shade/drawer/dock/cards. The shell now starts through `a21s-shell` → checksum-gated `a21s-qt-gpu`; dock/drawer use live scene blur, shade blurs one fresh pre-open screen capture. Apps keep software rendering through `Appearance.appEnvironment`. GPU-unavailable startup falls back to translucent software glass. [Implementation and device verification](a21s-qt-gpu-scanout.md#shell-integration). Text drawn on the wallpaper uses `onWallpaper` so the clock stays readable in light mode. Legacy Dialer/SMS do not bind `Theme.light` yet |
| review findings (7 open) | **done, 2 gaps** | WWAN radio not modem power; no `rfkill`; FIFO queues; one `a21s-telemetry` process (stops asleep); "Rotate" label; `schedutil` default; PSK via stdin pty (`a21s-wifi-connect`). New polkit rule allows `gamer` NM only (ADR-9). Verified: `nmcli radio wifi off/on` as gamer, join of an unsaved network (HTTP 301), `nmcli connection add`. Open: PSK update of an existing profile (pty timeout), Settings UI path not exercised |
| M7.8 Settings v2 | **done with gaps** | app already wired to services; verified Wi-Fi list + join dialog, accent live update, volume slider -> `wpctl` 0.71, new light/dark switch (`color-scheme` + `kdeglobals`), Display&S sleep. Fixed: `a21s-appearance` never set `accent-color`. Open: BT pair (no device in range), typed Wi-Fi join |
| M7.6 close morph | **done** | `ScreencopyView` of the output (SHM, no GL), one frozen frame shrunk onto the app icon; capture before minimize; fallback = gradient+icon. Verified with slowed morph + burst screenshots |
| Shade split (owner request) | **done** | left-half pull → Notifications + Now Playing card, right-half pull → Controls (QS tiles/brightness/volume), tabs and sideways swipe switch pages; threshold 30 % so a normal pull opens it. Verified with injected touch over home AND over a fullscreen app |
| M7.9 gesture system | **core + polish** | back chip (display-only surface, follows the finger, accent at threshold), close morph snapshot; remaining: real-finger thresholds, Kirigami conflicts, Steam resume |
 Android-style home/recents/switch/back verified with injected touch over Kalk and fullscreen Steam; polish listed in `HANDOFF-DESKTOP-SHELL.md` §4 |

Handoff for other agents: [HANDOFF-DESKTOP-SHELL.md](HANDOFF-DESKTOP-SHELL.md); ready-to-paste prompt: [PROMPT-DESKTOP-SHELL.md](PROMPT-DESKTOP-SHELL.md).

Review of the old shell code reported 10 findings (3 fixed, 7 open): mobile-data toggle disables the modem, single-slot command queues in Network/Bluetooth, 3 s telemetry fork storm, airplane mode `rfkill unblock all`, fake auto-rotate, Wi-Fi password in argv, governor defaults to `performance`.
