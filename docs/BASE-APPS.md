# Base apps (in-house) — spec, tone, and file ownership

> Started 2026-11-16. Owner decision: reduce third-party dependence for the six base apps
> (photos, music, notes, clock, calculator, terminal) by writing them in-house, one owner per app,
> with a single visual/behavioural tone. This narrows **D2** ("replace home-grown QML apps with real
> Linux apps") for those six roles only; Settings, Dialer and SMS were already shell-owned.

## 1. Rules every base app follows

| Rule | Why |
|---|---|
| `import "../kit"` + `A21s.Theme` / `A21s.Services` / `A21s.Widgets`; **no literal colors or sizes** | one tone; Theme owns tokens |
| Window: `ApplicationWindow`, `visible: true`, `width/height: Screen.*`, `color: Theme.bg`, `title: "<App>"` | labwc maximizes it like every other app |
| Bind the theme: `Binding { target: Theme; property: "light"; value: ShellPrefs.lightTheme }` | light/dark switch must work inside the app |
| Accents come from `ShellPrefs.accentColor` / `ShellPrefs.onAccentColor`, never `Theme.primary` | user-picked accent |
| Page layout: margins `Theme.spaceM`, section labels in `fontCaption` + `Theme.textDim`, titles `fontTitle` | matches the shell |
| Cards: `Theme.card`, radius `Theme.radiusL`, 1 px border `Qt.alpha(Theme.text, 0.08)` | glass/soft look |
| No blur: the shell runs with the software backend and `BackdropBlur` is a no-op without GL | honesty, not sloppiness |
| Empty states use `EmptyState` (icon + one sentence + optional action) | never a blank screen |
| Keyboard: text fields must work with squeekboard (`QT_IM_MODULE` unset); leave room for the OSK (bottom padding) | phone first |
| Navigation: **no in-app back button** — the system back gesture minimizes the app; use tabs/segmented controls for sections | one back affordance |
| Persistence: JSON under `~/.local/share/a21s-<app>/`, written atomically (`tmp` + `mv`), never in the repo | survives updates |
| Locale: UI copy in English; Thai input/output must render (Noto Sans) | owner writes Thai |
| Performance: no per-frame full-screen rescaling, no shader effects; list with `ListView` + `cacheBuffer` | software renderer |

## 2. Where files go (disjoint ownership)

```
apps/kit/            shared components — LEAD owns, agents must not edit
apps/<app>/<App>.qml one app — exactly one agent owns the whole directory
rootfs-overlay/usr/local/bin/a21s-<app>        wrappers — LEAD
rootfs-overlay/usr/share/applications/a21s-<app>.desktop  entries — LEAD
shell/A21s/Desktop/HomeScreen.qml (home/dock lists)       LEAD
scripts/deploy_shell.sh (apps/ -> /usr/share/a21s-apps/)  LEAD
```

Apps are deployed to `/usr/share/a21s-apps/<app>/` and launched with
`QML_IMPORT_PATH=/usr/share/a21s-shell quickshell -p /usr/share/a21s-apps/<app>/<App>.qml`.

## 3. Per-app MVP and exit criteria

| App | Replaces | MVP (must work) | Later |
|---|---|---|---|
| Calculator | `kalk` | keypad (0-9 . ± × ÷ = C ⌫), live expression, result, history list (last 20, tap to reuse), clear history | scientific functions, unit conversion |
| Clock | `kclock` | big clock + date, **Alarm** list (add/delete, persisted, fires a notification through the shell's `org.freedesktop.Notifications`), **Timer** (start/pause/reset, ring + notification), **Stopwatch** (start/lap/reset) | world clock, bedtime mode |
| Notes | `marknote` | list (title + preview + edited time), tap to edit, autosave to `~/.local/share/a21s-notes/<id>.json`, new/delete, search box | folders, markdown preview, export |
| Photos | `pix` | grid of thumbnails from `~/Pictures` (+ `~/Downloads`), tap for full-screen viewer with pinch-free zoom (double-tap zoom, drag), swipe left/right between photos, file name + date | albums, rotate/delete, share |
| Music | `elisa` | list audio files from `~/Music` (+ `~/Downloads`), play with `mpv --no-video --input-ipc-server=<runtime>/a21s-music.sock`, transport (play/pause/next/prev/seek), now-playing bar, volume follows the system sink | playlists, artwork, MPRIS export |
| Terminal | `qmlkonsole` | command input + scrollback (monospace), runs one command at a time through `script -qfec` (PTY), history up/down, clear, ANSI escapes stripped | full PTY session, tabs |

Exit criteria for every app: launches from the drawer **and** the home icon; light/dark follows the
shell; accent follows the shell; back minimizes (not kills); screenshot captured on the device;
`./scripts/check.sh` green.

## 4. Verification recipe (per app)

```
./scripts/deploy_shell.sh            # or shell_reload.sh for shell-only changes
quickshell -p ... ipc call launcher launch a21s-calculator
grim /tmp/shot.png                   # after waking with a21s-power-key fp-unlock
```

## 5. Device verification log (2026-11-16)

All six apps were launched under the shared device lock (`scripts/device_session.sh`) and
screenshotted in the shell tone. Interaction spot checks:

| App | Verified | Still unverified |
|---|---|---|
| Calculator | window + keypad render, live result | tap-through (2+3=), history round-trip |
| Clock | Alarm panel → **Save alarm wrote** `~/.local/share/a21s-clock/alarms.json` (`{"enabled":true,"hour":7,"minute":30,...}`), empty state | alarm actually ringing (needs a due alarm + wait), timer zero, stopwatch laps |
| Notes | New note → editor with OSK → typed → **autosave wrote** `~/.local/share/a21s-notes/<id>.json` (`{"v":1,...,"title":"bk"}`) | Thai typing, delete confirm, list round-trip after relaunch |
| Photos | grid (4 photos) → viewer (1/4, name/date/path) → **swipe → 2/4** | double-tap zoom/pan, arrow buttons |
| Music | library found `test-tone.wav`, tap → **playing, progress 0:03/0:08**, transport shown | next/prev, stop, mpv-missing banner |
| Terminal | OSK input focus works, **command ran through the PTY** (`bash: sx: command not found` + red status), chips/OSK layout | successful command output (typed letters landed as `sx`), history persistence |

Notes for the next pass: the OSK changes the vertical layout, so scripted taps must be
re-coordinated after the keyboard appears; `~/Music` was created by root during testing and had to
be `chown gamer:gamer` before ffmpeg could write into it.

### KDE originals removed (same session)
`pacman -Rns kalk kclock marknote elisa qmlkonsole`, then `koko` and `maui-pix` (the photo
apps behind `org.kde.pix.desktop` / `org.kde.koko.desktop`; the "pix" package name did not exist,
the entry belonged to maui-pix). All had no dependents. The home grid's `byId("org.kde.x") || fallback` pattern now resolves to the in-house
apps for Clock/Calculator/Notes; Gallery and Terminal fall back to ids that no longer existed, so
two `NoDisplay=true` compatibility entries were added: `a21s-gallery.desktop` (→ `a21s-photos`)
and `org.kde.qmlkonsole.desktop` (→ `a21s-terminal`). Replace them with real ids the next time
`HomeScreen.qml` is free to edit (another task was editing it), then delete the shims.

