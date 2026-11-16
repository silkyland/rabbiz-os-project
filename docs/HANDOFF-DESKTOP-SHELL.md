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
| M7.3 desktop surface (icons/widgets on the desktop, new dock) | **not started** (old HomeScreen kept) |
| M7.4 base apps are real Linux apps; QML Calc/Clock/Notes/Gallery/Files/Spotify deleted | **done, verified** |
| M7.5 form factor seam (`handset` only, D3) | not started |
| M7.6 close morph via snapshot (S1: `ScreencopyView` exists, untested) | not started |
| M7.7 mouse/keyboard/gamepad nav; Steam as an app | **Steam splash works** (ADR-8), input not started |
| M7.8 Settings v2 (thin shell-owned panels; `plasma-settings` lacks Wi-Fi/BT/sound/battery) | not started |
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
- Disk is ~95 % full (1.2 GB free). Check `df -h /` before installing. `linux-aarch64` (386 MiB, dead weight) and
  `gcc`/`clang` (~190 MiB) are removable; **do not remove `llvm-libs` (mesa needs it)**.

## 3. Task backlog (priority order, each with exit criteria)

1. **Verify Steam end to end** (ADR-8). Wait ~90 s after launch; screenshot must show Big Picture, not black/splash.
   Then: touch works inside nested Weston, a game starts (`steam://rungameid/433950` Bit Blaster XL), free RAM stays
   > 100 MB, closing the window kills Weston+Steam (`pgrep -u gamer weston` empty). If touch is dead, forward input
   (weston `--xwayland` + seat) or try `--renderer=gl`. Update ADR-8 "Not verified" list.
2. **Fix the 7 open review findings** (details in `git log` message of 89cdc3a and `docs/DESKTOP-SHELL.md` §10):
   mobile data toggle disables the whole modem (`SystemControl.qml` ~146); single-slot pending command in
   `Network.qml:95` / `Bluetooth.qml:85` (use a FIFO); 3 s telemetry fork storm (`SystemControl.qml` ~732, stop while
   asleep, read static values once); airplane mode `rfkill unblock all` (~95); fake auto-rotate (~135); Wi-Fi password
   in argv (`Network.qml:68`, use stdin/`--ask` or a keyfile); CPU governor default `performance` (~247).
3. **M7.8 Settings v2.** Keep `shell/apps/settings/Settings.qml` shell-owned but rebuild as thin panels over NM,
   BlueZ, PipeWire, UPower, appearance. Reuse `plasma-settings` pages for Colors/Fonts/Default apps/Keyboard only if
   they work when opened (they list fine; opening was not tested). Exit: Wi-Fi join, BT pair, volume, accent/dark
   switch (calls `Appearance`) all work from it.
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
**Still to do:** back-arrow chip + card that follows the finger (use `ShellState.gestureProgress/gestureKind`), proper reverse morph (S1 snapshot), tune thresholds on a real finger (injected swipes are perfectly straight), conflict check with Kirigami edge gestures, and resuming a minimized Steam from its icon (`Launcher.findToplevel` cannot match the nested Weston window: empty app_id, title "Weston Compositor - wayland-fullscreen").

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
- "Never overwrite /lib", usb0 unmanaged, secrets, `clk_ignore_unused`, ABOX 768 KiB, `QT_QUICK_BACKEND=software`
  all still apply. `QT_IM_MODULE` stays unset.
- Rules are hypotheses (owner's CLAUDE.md): if evidence says one is wrong, show it, propose the change, record an ADR.

## 6. Open product questions for the owner

1. Which widgets belong on the home screen? 2. Is external display + mouse/keyboard (`desktop` mode) a real use case on
the A21s? 3. Is a KDE-looking app set acceptable instead of Material? 4. Do you want the dead `linux-aarch64` package
removed (frees 386 MiB)?
