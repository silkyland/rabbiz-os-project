# Handoff — desktop shell rewrite (M7 v2)

> Written 2026-11-16 by the agent that ran the first session. Branch `feat/desktop-shell-rewrite`.
> Read first: [DESKTOP-SHELL.md](DESKTOP-SHELL.md) (plan, decisions D1–D5, spike results, progress log),
> [ARCHITECTURE.md](ARCHITECTURE.md) "Shell seams" + ADR-6/7/8, [../AGENTS.md](../AGENTS.md) rules.
> Everything marked **verified** was run on the device; everything else is unverified. Trust the repo, not memory.

## 1. State of the world

| Milestone | State |
|---|---|
| M7.0 split `Components` -> `Widgets`/`Windowing`/`Desktop`/`Panels` | **done, verified** |
| M7.1 `Launcher` singleton + morph driven by real toplevel map (`LaunchFlow.js`) | **done, verified** (kalk, kclock, index, marknote) |
| M7.2 desktop contract v1 (`a21s-appearance`, `Appearance` service, kde platform theme) | **done for Qt apps, verified**; GTK/Chromium unchecked |
| M7.3 desktop surface (icons/widgets on the desktop, new dock) | **mostly there** — code-inspected 2026-11-16: wallpaper w/ parallax, morphing clock, swipeable hero deck (Steam + Now Playing MPRIS + telemetry), 2x4 grid, dock, drawer overlay. Not yet device-verified this session; free-position widgets and S5 frame-time numbers still open |
| M7.4 base apps are real Linux apps; QML Calc/Clock/Notes/Gallery/Files/Spotify deleted | **done, verified** |
| Launcher-IPC crash 2026-11-16 ~15:03 | **not the base apps** — `ipc call launcher launch a21s-clock` produced a Quickshell **segfault** (crash `064lx4lot`, signal 11) whose qslog ends in dmabuf format probing, and the device was then **cleanly rebooted** by another task's GPU/CMA testing (systemd "Reached target System Reboot", not a panic). The five apps render fine started through their wrappers with `QT_QUICK_BACKEND=software`. Re-verify the launcher path once the GPU task is settled; do not attribute this to the apps without evidence |
| Base apps in-house (owner decision 2026-11-16) | **all six render, integration pending** — `apps/{calculator,clock,notes,photos,music,terminal}` share `A21sKit` ([BASE-APPS.md](BASE-APPS.md)); each launched on the device under the device lock and screenshotted in the shell tone (no QML errors in their logs). Wrappers + `A21s <App>` desktop entries deployed. Still owed: `HomeScreen` home/dock lists point at the new ids, remove the KDE apps (kalk/kclock/marknote/pix/elisa/qmlkonsole) so the drawer has no duplicates, and per-app interaction passes (alarm fires, note autosave round-trip, photo viewer swipe/zoom, mpv play, a terminal command) |
| Status bar battery (owner request 2026-11-16) | **UI ready, no sensor** — bar shows the percentage + charging icon as soon as UPower reports a present battery; today `/sys/class/power_supply` is empty and UPower only has a fake DisplayDevice (`0%`, `battery-missing`), so it shows "AC". Real percentage needs the s2mu106 fuel-gauge/charger driver (not in the kernel tree, no DTS node) |
| Theme: light/dark + glass (owner request 2026-11-16) | **done for the shell + Settings** — `Theme.light` palette, `glass/glassHigh/glassBorder` tokens, `onWallpaper` text tokens; app roots must bind `Theme.light` from `ShellPrefs.lightTheme` (Theme may not import Services). Shell GPU blur is integrated: dock/drawer sample a live sibling scene; shade takes one fresh frozen output capture before showing itself. `a21s-shell` selects the pinned GPU path with software fallback; apps retain software. [Device verification](a21s-qt-gpu-scanout.md#shell-integration) |
| M7.5 form factor seam (`handset` only, D3) | not started |
| M7.6 close morph via snapshot | **done, verified** (S1 answered: `ScreencopyView` works via the SHM fallback even without GL) |
| M7.7 mouse/keyboard/gamepad nav; Steam as an app | **Big Picture verified** (logged in, touch taps work, game launch reaches Proton/FEX); game renders black — ADR-8. Input (mouse/keyboard/gamepad) not started |
| Shade split (owner request 2026-11-16) | **done, verified** — pull down on the left half of the status bar = notifications + media page, right half = quick-settings page; a real horizontal pager follows the finger (pageShift + Behavior) and settles with animation, tabs animate too; open threshold 30 % |
| M7.8 Settings v2 (thin shell-owned panels; `plasma-settings` lacks Wi-Fi/BT/sound/battery) | **app ships and is wired** (pre-existing, not a rewrite): verified Wi-Fi list + join dialog, accent live update, volume slider, new light/dark switch; BT pair untested |
| M7.10 **home editing** (long-press, drag & drop, snap/align, persisted layout) | **not started** — spec §7.1 |
| M7.11 **clipboard system** | **not started** — spec §7.2 |
| M7.12 **on-screen keyboard rework** (owner: many bugs, poor UX) | **not started** — triage first, spec §7.3 |
| M7.9 **gesture system** (Android-style) | **core done, verified with injected touch** (§4): bottom swipe-up=home, swipe-up+hold=recents, swipe along bar=prev/next app, side-edge swipe=back; all work over fullscreen windows (surfaces moved to the Overlay layer). Missing: back-arrow chip + live card following the finger, reverse morph from a window snapshot, Steam resume matching |

Owner decisions (2026-11-16): home = touch-first desktop; replace home-grown apps with real Linux apps (Settings,
Dialer, SMS stay ours); Pad 7 Pro is **not** planned now; Steam is a normal app; may override app looks with our own
theme and rewrite `Theme.qml`; Spotify removed; the device is a lab device (flash/restart/package changes OK,
AGENTS.md hard rules still apply).

## 2. How to work (device loop)

- Device link: USB only (`172.16.42.1`, Mac gets `172.16.42.2`; Wi-Fi is usually disconnected). SSH key
  `~/.ssh/id_ed25519` has a passphrase: the human must `ssh-add --apple-use-keychain ~/.ssh/id_ed25519` (and
  `ssh-copy-id` once). Agents cannot type passwords. If the device is in Samsung download mode (`04e8:685d`),
  see memory/`device-access` or send Odin end-session+reboot over `/dev/cu.usbmodem*`; do NOT hold Volume Down
  while rebooting (enters `debug_shell`).
- Host checks: `./scripts/check.sh` (seam checker + its fixtures + LaunchFlow node tests). Run before every commit.
- Push shell + restart it + screenshot: `./scripts/shell_reload.sh` -> `/tmp/a21s-shot.png`. Full overlay deploy:
  `./scripts/deploy_shell.sh` (prunes replaced apps; chowns `~/.local`).
- Drive the UI without touching the phone: `quickshell -p /usr/share/a21s-shell/shell.qml ipc call launcher launch <id|name>`,
  `... launcher state`, `... launcher toplevels` (run as `gamer` with `XDG_RUNTIME_DIR=/run/user/1001`,
  `WAYLAND_DISPLAY=<newest wayland-N socket>`, `QML_IMPORT_PATH=/usr/share/a21s-shell`).
- Wake/unlock the fake lock screen: `runuser -u gamer -- env XDG_RUNTIME_DIR=/run/user/1001 a21s-power-key wake` then `fp-unlock`.
  The screen locks after ~2 min idle; do that before every `grim` screenshot (`timeout 10 grim ...`).
- **Traps found the hard way:** `pkill -f` patterns match your own ssh command line (use `[s]team` style);
  `grep -E '^(...|QT_)='` style env filters silently drop `QT_*` (see `shell_reload.sh`); the labwc Wayland socket is
  not always `wayland-0` after a session restart; never run `steamos-session` from SSH while labwc owns the seat;
  the shell process must run with `QT_QPA_PLATFORMTHEME=` + `QT_QUICK_CONTROLS_STYLE=Basic` but apps must NOT
  inherit that (the `Launcher` starts apps with the contract env via `Quickshell.execDetached`).
  `SDL_VIDEODRIVER=wayland` is exported session-wide by `a21s-phone-session`, and Steam's CEF dies on it — see ADR-8.
  There is no `fp-unlock` binary: it is `a21s-power-key fp-unlock`, and the screen re-locks after ~2 min, so unlock
  before every grim.
  **The phone needs Wi-Fi for anything online** (Steam login, package installs): `usb0` has a default route
  (metric 700) that wins over Wi-Fi, and it points at the Mac, which does not route. Check with
  `curl https://client-update.akamai.steamstatic.com/`; if Wi-Fi is disconnected, fix the route metric
  (`nmcli connection modify <ssid> ipv4.route-metric 50`) or the network looks "connected" but times out.
  **Privileges:** the shell runs as `gamer` and labwc has no polkit agent, so without
  `/etc/polkit-1/rules.d/51-a21s-shell.rules` every `nmcli` action fails with "Insufficient privileges" (radio
  toggles only worked because NM's default allows `allow_active`). After adding/editing a rule run
  `systemctl restart polkit`; `nmcli connection load` stays root-only no matter what.
  **Never pre-fill a pty for a password prompt**: the tty echoes it before `nmcli` disables echo (this leaked a PSK
  into a session log once). Use `stty -echo` first, as `a21s-wifi-connect` does, and never `cat` the helper's raw
  output — grep only for `Error:`/`activated`.
  **Wi-Fi devices:** `p2p0` is typed `wifi` too. Pass `ifname wlan0` to `nmcli device wifi connect`, and skip
  `p2p*` when discovering the Wi-Fi device, or NM activates on p2p0 and fails with "ssid-not-found".
  **Duplicate profiles:** `nmcli device wifi connect <ssid>` creates `<ssid> 1`, `<ssid> 2`... when a profile with
  that name exists; `a21s-wifi-connect` looks profiles up by SSID to avoid that.
  **USB link drops under load**: `en18` lost `172.16.42.2` and `ssh root@172.16.42.1` hung twice in one session
  (device stayed up, no reboot). The phone also listens on Wi-Fi: `ssh root@192.168.50.6` kept working, `shell_reload.sh
  root@192.168.50.6` too — use it when USB is flaky.
  **ShellPrefs only reloads on the atomic save**: writing `~/.config/a21s-shell/prefs.json` in place (truncate) is
  not picked up by the shell; `printf > tmp && mv` is (that is what `ShellPrefs.save()` does). To test a pref live,
  change it through the Settings UI, not with a text edit.
  **Gestures:** a slow swipe-up (300 ms) opens Recents ("swipe up and hold"); a fast one (~120 ms, start y>=1590)
  goes home.
  **Frame time (S5):** `./scripts/frame_time.sh [user@host] [--gesture drawer|shade|none]` restarts the shell with
  `QSG_RENDER_TIMING=1`, drives a gesture and prints p50/p90/max from Qt's "frame rendered in N ms" lines. On this
  Qt/Quickshell build under `QT_QUICK_BACKEND=software` that profiler only emits a handful of lines (3 for a whole
  gesture) and `PanelWindow` exposes no `frameSwapped` signal to QML, so per-frame fps cannot be read directly — treat
  the script as a startup/one-shot latency check and judge smoothness by feel (the owner is the reference).
  Drawer stutter fixes 2026-11-16: the wallpaper no longer `scale`s with `drawerProgress` and the whole home subtree no
  longer scales down (both forced a full-screen bilinear resample every frame); the dim/opacity cues remain.
- **Disk (investigated 2026-11-16):** the rootfs is a 22.3 GiB ext4 on `/dev/loop0p37`, which ends 5 MB before the
  end of the eMMC — there is **no partition headroom**, `resize2fs` cannot grow it. Free went from 285 MB to 2.2 GB by
  deleting only stale/unused data: `/home/gamer/.local/share/Steam-arm64` (1.4 GB duplicate Steam install, nothing
  points at it: `.steam/steam` → `Steam/`), `/opt/build/box64` (364 MB build tree; the binary is `/usr/local/bin/box64`),
  `Steam/appcache` (150 MB), pacman cache + journal. Big remaining consumers: Steam 7.8 GB, `/usr` 7.2 GB, `/opt` 1 GB,
  Chrome profile 464 MB, opencode ~1 GB.
  **Do not** `pacman -Rns linux-aarch64` blindly: it cascades to `mkinitcpio`, `mkinitcpio-busybox`, `diffutils` and
  deletes the Arch initramfs from `/boot`. Booting is unaffected (the flashed `mmcblk0p18` carries its own initramfs —
  verified by the `TRAILER!!!`/`initramfs` strings in the image), but the tools were reinstalled to keep initramfs
  building possible. `pacman -Rns` also left `/usr` owned by uid 502 (a macOS tar artefact); fixed with
  `chown -R root:root /usr` and removing `._*` AppleDouble files — do that after any tar push that touches `/usr`.
  **Do not remove `llvm-libs` (mesa needs it).** The owner keeps `gcc`/`clang`/`cmake`/`git` on the device on purpose
  (they build there), so the ~690 MB toolchain, the ~300 MB unused linux-firmware splits, qt5/mgba and
  plasma-workspace/portal-kde were examined and **deliberately not removed** (owner decision 2026-11-16).

## 3. Task backlog (priority order, each with exit criteria)

1. ~~**Verify Steam end to end** (ADR-8).~~ **Done 2026-11-16, one gap open.** Verified on the device: Big Picture
   renders logged-in with online content, a touch tap opens a game page, `steam://rungameid/433950` reaches Proton/FEX,
   `steam -shutdown` kills Steam+Weston in <=14 s, RAM 82 MB free / 547 MB available at Big Picture. Fixed the black
   screen: `SDL_VIDEODRIVER=x11` in `a21s-steam` (the session exports `wayland`, which Steam's CEF/SDL cannot use).
   **Open:** the game itself is black (FEX ~100 % on one core, no frames) — likely panvk/Direct3D; also a FEX server is
   orphaned after exit, and a second `a21s-steam <url>` can start a second Weston. Needs Wi-Fi to be up (see §2).
2. ~~**Fix the 7 open review findings.**~~ **Done 2026-11-16, two gaps open.** All seven changed, verified on the
   device where a device check exists: mobile data now toggles the WWAN radio (`nmcli radio wwan`), not modem power;
   airplane mode is `nmcli radio wifi/wwan` + BlueZ power and no longer calls `rfkill unblock all`; FIFO queues in
   `Network`/`Bluetooth`/`SystemControl`; telemetry is one `a21s-telemetry` process (bash builtins, no per-value forks)
   and stops while asleep; the rotation tile is honestly labelled "Rotate"; the governor defaults to `schedutil`;
   the Wi-Fi PSK goes over stdin to `a21s-wifi-connect` (nmcli pty prompt, echo off) instead of argv.
   **The blocker behind all of it:** labwc has no polkit agent, so `gamer` got "Insufficient privileges" for every NM
   action. New `rootfs-overlay/etc/polkit-1/rules.d/51-a21s-shell.rules` allows *only* `gamer` and *only*
   `org.freedesktop.NetworkManager.*` — recorded in `docs/ARCHITECTURE.md` ADR-9. Verified: `nmcli radio wifi off/on`
   as gamer, joining an unsaved network through `a21s-wifi-connect` (connection up, `192.168.50.6`, HTTP 301),
   `nmcli connection add` as gamer. Also fixed on the way: activate on `wlan0`, never `p2p0` (NM otherwise fails with
   "association took too long / ssid-not-found"), and `Network.qml` no longer treats `p2p0` as the Wi-Fi device.
   **Open:** updating the PSK of an *existing* profile via `nmcli --ask connection modify` hit the 45 s pty timeout in
   one run (not verified); the Settings UI path (`Network.wifiConnect`) was not exercised end to end.
3. ~~**M7.8 Settings v2.**~~ **Done 2026-11-16, with two gaps.** `shell/apps/settings/Settings.qml` (1904 lines)
   already binds Network/Bluetooth/SystemControl/ShellPrefs, so this was verification, not a rewrite. Verified with
   injected taps: live Wi-Fi/BT/cellular rows; Wi-Fi list (signal/security) and the join dialog + OSK; accent tap
   updates shell chrome **live** (no restart) and rewrites the app contract; volume slider -> PipeWire 0.71 with the UI
   agreeing; Display & Sleep (brightness, timeouts); new Theme > Light theme switch flips
   `gsettings color-scheme` prefer-dark/prefer-light and `kdeglobals` BackgroundNormal 10,10,12 <-> 246,246,248.
   Fixed here: `a21s-appearance` wrote `color-scheme` but never `accent-color`, so GTK/Chromium ignored the shell
   accent (now mapped to the nearest libadwaita accent; green/amber/red verified); added the missing
   `ShellPrefs.lightTheme` + `Appearance.light` binding + the Settings switch (no dark/light control existed).
   **Open:** BT pair/connect needs a device in range (MX Keys and the Xbox pad are paired but powered off); the Wi-Fi
   password field was not typed (agents must not type secrets), so `Network.connectTo` is verified only down to
   `a21s-wifi-connect`; shell chrome stays dark in light mode because `Theme.qml` is still literal (item 9).
4. **M7.9 gestures** (§4) then **M7.6 close morph** (S1: `ScreencopyView` of the toplevel -> shrink to its icon;
   fallback icon-only morph; `LaunchFlow.close()` already has a 600 ms timeout).
5. **M7.3 desktop surface:** wallpaper + icons/widgets on the desktop + dock + drawer overlay. Measure frame time
   first (S5; software Qt Quick). Ask the owner which widgets matter (clock, weather, media, calendar).
6. **M7.7 input:** hover + focus rings, labwc keybinds, gamepad -> arrows/Enter/Esc bridge (uinput) while the shell has
   focus; shell QML must use `KeyNavigation`/`activeFocus`.
7. **M7.5** one `formFactor` property (`handset` only now) driving labwc window rules.
8. **GTK/Chromium theming check:** `gsettings get org.gnome.desktop.interface color-scheme` works; open Chrome and a GTK
   app and screenshot. If ugly, add per-app CSS (D5 allows overriding).
9. **Housekeeping:** `Theme.qml` rewrite as a thin reader of one palette (D5, keep token names as aliases), remove
   `linux-aarch64`, keep `docs/ARCHITECTURE.md` seam table and `scripts/check_shell_seams.mjs` rules in sync when
   adding a module, update `shell/README.md` (still describes the old layout and the parallel Steam session), push.

## 4. Gesture system (M7.9) — implemented core + remaining polish

**Implemented (2026-11-16, verified on the device with `scripts/touch_swipe.py` -> `/usr/local/bin/a21s-touch`):** `Windowing/Gesture.js` (pure classifier, `tests/gesture.test.mjs`), `Windowing/EdgeBack.qml` (side strips), `Panels/NavBar.qml` bottom MouseArea (continuous swipe), `ShellState.navAction/gestureProgress`. NavBar/Shade/StatusBar are layer `Overlay` so a fullscreen window (nested Weston/Steam) no longer hides them (root cause of "no gestures inside Steam"). Verified: Kalk swipe-up -> home; Steam fullscreen swipe-up -> home (Steam keeps running); left-edge swipe closes the app; swipe-up+hold shows Recents.
**Done since (2026-11-16, second session):** the reverse/close morph now shrinks a real screenshot of the app
(`ScreencopyView` of the output, one frozen frame) onto its home icon, with the gradient+icon morph as fallback.
`NavBar.goHome()` requests the capture *before* minimizing toplevels (140 ms later) so the snapshot sees the app, not
the home screen. S1 answered: on this device Qt Quick runs software (`No GL context`), Quickshell falls back to SHM
buffers and the capture still works (`MORPH close snapshot=true hasContent=true`); a frame arrives well inside the
200 ms fallback timer. Launch morphs are unchanged (no snapshot).
**Back semantics (owner request 2026-11-16):** the system back gesture now *minimizes* the focused app
(`toplevel.minimized = true` + `activeAppId` cleared) instead of `toplevel.close()`. The process keeps running and
Recents resumes it; a deliberate kill is still the swipe-up on a Recents card (`TaskSwitcher` calls `close()`).
Verified: after back on Kalk the app count was unchanged and the previously open app was revealed.

**Also done:** the back-arrow chip. `Windowing/EdgeBack.qml` keeps 22 px input strips and maps a display-only
chip surface per side only while a back gesture is in flight (the pointer is already grabbed, so it cannot steal a
fresh touch); it follows the finger's Y, slides in with progress and turns accent-filled at the trigger distance.
Verified: at ~40 % a neutral circle with the arrow sits under the finger, at 100 % it is accent-filled, and releasing
closes the app. Recents cards already follow the finger (a `DragHandler` on each card closes past a quarter height).
**Still to do:** tune thresholds on a real finger (injected swipes are perfectly straight), conflict check with
Kirigami edge gestures, and resuming a minimized Steam from its icon (`Launcher.findToplevel` cannot match the nested
Weston window: empty app_id, title "Weston Compositor - wayland-fullscreen").

Original proposal kept below for reference.

Handset only. Edge zones are 24 px (the labwc bottom margin already reserves it).
- Swipe up from bottom edge in an app -> go home with reverse morph (M7.6); in home -> open the drawer.
- Swipe up and hold -> task switcher (existing `TaskSwitcher`); swipe card up to close.
- Edge swipe from left/right -> back (existing `AppGestureLayer`); must not fire inside Kirigami apps' own
  drawer/page gestures: only trigger when the swipe starts in the outer 24 px and moves > 80 px horizontally.
- Pull down from the top edge -> shade (exists). Long-press on icon -> quick actions (new).
- Implementation home: `A21s.Windowing` (state in a pure `Gesture.js` with node tests like `LaunchFlow.js`, QML
  `GestureLayer.qml` rendering only). Keep the existing NavBar pill as the visible affordance.

## 5. Rules that were changed or confirmed this session

- `steamos-session` as a parallel session is no longer the Steam UX (ADR-8). Recovery only.
- **2026-11-16 (security decision):** polkit now lets user `gamer`, and only `gamer`, control
  `org.freedesktop.NetworkManager.*` without authentication (ADR-9). Reason: no auth agent exists under labwc, so
  Wi-Fi/BT could not be changed from the shell at all. Scope is NM only; systemd/package actions still need auth.
- "Never overwrite /lib", usb0 unmanaged, secrets, `clk_ignore_unused`, ABOX 768 KiB, `QT_QUICK_BACKEND=software`
  all still apply. `QT_IM_MODULE` stays unset.
- Rules are hypotheses (owner's CLAUDE.md): if evidence says one is wrong, show it, propose the change, record an ADR.

## 6. Open product questions for the owner

1. Which widgets belong on the home screen? 2. Is external display + mouse/keyboard (`desktop` mode) a real use case on
the A21s? 3. Is a KDE-looking app set acceptable instead of Material? 4. Do you want the dead `linux-aarch64` package
removed (frees 386 MiB)?


## 7. New work requested on 2026-11-16 (planned only — nothing below is implemented)

The owner asked for three things after the gesture work. Facts gathered from the device on 2026-11-16 are marked
**(verified)**; everything else is a design hypothesis.

### 7.1 M7.10 Home editing: long-press, drag & drop, align

What the owner wants: icons and apps can be dragged onto the home screen; press-and-hold starts drag; icons align.
Today **(verified in code)**: `Desktop/HomeScreen.qml` builds a fixed `homeApps` array (8-9 entries) and `dockApps`
(4); nothing is persisted and there is no edit mode.

Design:
- **Layout model** in a pure `Desktop/HomeLayout.js` (node-tested like `LaunchFlow.js`): pages -> cells
  `{id, col, row, spanX, spanY, kind: app|folder|widget}`, 4 columns x N rows (match today's tile size),
  functions `place`, `moveTo` (collision -> push/reflow neighbours to the next free cell), `snap(x,y)`,
  `compact()` (the "align" action, fills gaps), `swap`, `groupIntoFolder`, `remove`, `serialize/parse`.
- **Persistence:** extend `ShellPrefs.qml` JSON (`homeLayout`) — it already saves atomically
  (`mkdir && printf > tmp && mv`); keep layout separate from prefs if it grows (`~/.config/a21s-shell/home.json`).
  Unknown/uninstalled ids are dropped on load; first run seeds from the current default list.
- **Interaction (Android-like):** long-press 400 ms on an icon -> lift (scale 1.08 + shadow, haptic via `feedbackd` if
  available) and enter edit mode (other icons wiggle or show a grid); drag follows the finger; target cell is
  highlighted (snap + live reflow); drop on the dock row moves it to the dock; dwell on the left/right screen edge for
  600 ms flips the page; a "Remove" drop zone at the top (removes from home only, never uninstalls); tap on empty
  space or swipe-up exits edit mode. Long-press without moving opens quick actions (open, remove, app info).
- **From the drawer:** long-press an app in `AppDrawer.qml` and drag it onto the home screen (drawer closes while
  dragging, home page shown). "Align" = snap + compact; add a toggle in edit mode for "auto-arrange".
- **Gesture conflicts:** the bottom/edge gesture strips (M7.9) are 22-28 px; keep drop targets away from them.
- **Where:** `Desktop/HomeLayout.js` (+ `tests/homelayout.test.mjs`), `Desktop/HomeGrid.qml` (render + drag),
  small changes in `HomeScreen.qml`/`AppDrawer.qml`/`AppIcon.qml` (`AppIcon` is already in `Desktop`).
- **Test with injected touch:** `HOLD_MS=500 a21s-touch x1 y1 x2 y2 700` (long-press then drag; the tool now
  supports `HOLD_MS`). Exit criteria: drag Calculator to another cell and it stays after
  `shell_reload.sh` and after a reboot; drop onto the dock works; page flip works; `check.sh` green;
  frame time during drag measured (S5, software Qt Quick) and reported.
- **Risks:** software rendering makes drag animations janky — prefer transforms on a single lifted item, no blur.

### 7.2 M7.11 Clipboard system

Facts **(verified 2026-11-16):** nothing installed: `wl-clipboard`, `cliphist` are MISSING (both are in the ALARM
`extra` repo); `labwc` has `wlr-data-control` and `ext-data-control` built in (needed by `wl-paste --watch`);
Quickshell has **no** data-control binding (no such strings in the binary), so the shell must talk to a helper process.

Design:
- Install `wl-clipboard` + `cliphist` (small). Service `Services/Clipboard.qml` runs
  `wl-paste --watch cliphist store` (text) and `wl-paste --type image --watch cliphist store` (images) as `gamer`,
  started from labwc `autostart` (or a user systemd unit) so history survives shell restarts.
- **Privacy rules (do not skip):** keep the store in `$XDG_RUNTIME_DIR` (tmpfs, wiped on reboot) by default; cap
  history (e.g. 50 items, 1 MB per item); skip items advertising `x-kde-passwordManagerHint=secret`; "Clear" in the UI;
  clear on a real lock if a lock exists. Never log clipboard contents.
- **UI:** a clipboard sheet (Panels) reachable from the shade tile, a long-press on text fields is handled by the app
  toolkit (Qt/GTK touch handles — do not replace), and a clipboard key/row in the new OSK (7.3). Tapping an entry runs
  `cliphist decode <id> | wl-copy` (re-owns the selection, because a Wayland selection dies with its source app).
  Paste into the focused app = the user's long-press/paste menu or the OSK paste key (`wtype`-style synthetic
  Ctrl+V needs `zwp_virtual_keyboard`, which labwc has) — decide after testing which apps honour it.
- **Exit criteria:** copy in Marknote, switch to Chrome, open the history sheet, see the entry, tap it, paste works;
  an image copied in Pix shows a thumbnail; history cleared by the button; nothing written to persistent storage by
  default; documented in `docs/ARCHITECTURE.md` (new ADR) and `AGENTS.md`.

### 7.3 M7.12 On-screen keyboard rework

Owner: the current keyboard has many bugs and bad UX. **I have not reproduced the bugs yet** — the first step is a
triage, not a rewrite (use the `reproduce-my-bug` skill).
Facts **(verified):** `squeekboard 1.43.1` runs; custom layouts are `~/.local/share/squeekboard/keyboards/{us,th}.yaml`
(84 + 113 lines); control via `a21s-osk-ctl` (`toggle` over `sm.puri.OSK0`, `lang-next` reorders gsettings
input-sources); `QT_IM_MODULE`/`GTK_IM_MODULE` must stay unset (AGENTS.md); `fcitx5 5.1.23` and `fcitx5-libthai` are in
the ALARM repo (not installed); labwc supports `zwp_input_method_v2`, `zwp_text_input_v3`, `zwp_virtual_keyboard`;
Quickshell has no binding for any of them, so a custom keyboard cannot be written purely in the shell's QML.

Plan:
1. **Triage (half a day):** type into Marknote, Chrome, Index search, Kirigami dialogs, XWayland (Steam login), a
   terminal, using `a21s-touch` taps + screenshots. Record every bug with steps and a screenshot in a table
   (auto-show/hide, hides behind or covers the field, wrong height/size, language switch, Thai layout errors, key
   repeat, no haptics/key preview, theme mismatch, stuck visible after app close, interferes with the new gestures).
2. **Fix squeekboard first (cheap, evidence driven):** layout yaml (key sizes for a 720 px panel, Thai Kedmanee
   correctness, number row, emoji), behaviour (auto-show/hide, exclusive zone so maximized windows resize — check
   with the gesture bar), restyle through the desktop contract (`Appearance` colours), add a clipboard/paste row (7.2).
3. **Decide with evidence whether squeekboard is the ceiling.** If bugs are structural (no theming, no clipboard
   key, no swipe typing, poor Thai input), the realistic rewrite is **fcitx5 (engine, `fcitx5-libthai`) + our own
   keyboard UI as a separate Wayland client** that talks `zwp_input_method_v2`/`virtual_keyboard` (Qt Quick app, software
   renderer) — not in Quickshell. That needs an ADR because it changes the AGENTS.md rule about `QT_IM_MODULE` and the
   "dead ends on labwc" list. Do not start it before step 1 shows squeekboard cannot be fixed.
4. **Exit criteria:** a written bug table with every item either fixed or consciously deferred; typing a Thai and an
   English sentence into Marknote, Chrome and a Kirigami dialog works; the keyboard never covers the focused field;
   hide/show is predictable; theme follows the accent; verified with screenshots.

### 7.4 Order and parallelism

Suggested order: Steam resume matching + gesture polish (finish M7.9) -> 7.3 triage (cheap, unblocks UX judgement) ->
7.2 clipboard (small, independent) -> 7.1 home editing (largest) -> OSK rewrite only if triage demands it.
Disjoint files for parallel agents: 7.1 `Desktop/*` + `HomeLayout.js`; 7.2 `Services/Clipboard.qml`,
`Panels/ClipboardSheet.qml`, `rootfs-overlay` autostart; 7.3 `rootfs-overlay/.../squeekboard`, `a21s-osk-ctl`.
