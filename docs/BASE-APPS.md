# Base apps (in-house) — spec, tone, and file ownership

> Started 2026-11-16 as six in-house QML apps. **Cut over 2026-11-20 (owner
> decision):** the whole app family — including Settings, Dialer and SMS — is
> now **pure-Rust Wayland apps** in `tools/rabbiz-apps/crates/` (one
> `rabbiz-<app>` binary per crate). The QML apps (`apps/`, `apps/RabbizKit/`,
> `shell/apps/`) and their quickshell wrapper scripts were removed; see git
> history for the old tree. This narrows **D2** ("replace home-grown QML apps
> with real Linux apps") for the in-house roles; GTK/Chromium/Steam stay
> third-party.

## 1. Rules every base app follows

| Rule | Why |
|---|---|
| Built on `rabbiz-app-kit` (`tools/rabbiz-apps/crates/rabbiz-app-kit`): implement `AppState`, render through `ui::render` / `Canvas`, run with `rabbiz_app_kit::run_app` | one runtime: `wl_shm` + `xdg_toplevel` + `wl_touch` + `zwp_text_input_v3` on Linux, host window (`winit`/`softbuffer`) elsewhere |
| Design tokens come from `de_design` (`de-design` crate) via `rabbiz_app_kit::{tokens, Theme}` — **no literal colors** in app code | one tone; theme owns tokens |
| Theme + accent load through `rabbiz_app_kit::load_theme()` / `ThemeWatcher`, which reads the shared `de-config::Prefs` | light/dark + user accent switch works inside every app |
| MD3 widgets, `ScrollView`, Thai-safe `TextBuffer`, `MaterialIcon`-style `icons` come from the kit — don't re-implement them per app | consistent feel, less duplicated physics |
| Window is one xdg toplevel; `app_id()` returns `rabbiz-<app>`, `title()` the display name | labwc / rabbiz-de maximize it like every other app |
| Navigation: **no in-app back button** — the system back affordance maps to `AppEvent::Back`; use tabs/segmented controls for sections | one back affordance |
| Persistence: JSON under `~/.local/share/rabbiz-<app>/`, written atomically (`tmp` + `mv`), never in the repo | survives updates |
| Locale: UI copy in English; Thai input/output must render (`cosmic-text` + Noto Sans) | owner writes Thai |
| Every app ships the headless layer: `--cli <args>` runs pure logic without a window, `--mcp` serves the app's MCP tools (`rabbiz-app-kit/src/mcp.rs`) | testable + agent-drivable without a display |
| Tests are headless: `#[cfg(test)]` units plus `TestHarness` (`harness.rs`) driving real key/touch/render paths — no Wayland needed | `cargo test --workspace` gates in `scripts/check.sh` |
| Performance: no per-frame full-screen rescaling; lists scroll via the kit's fling physics | software raster path on the phone |

## 2. Where files go (disjoint ownership)

```
tools/rabbiz-apps/Cargo.toml                 workspace — LEAD owns members/lints
tools/rabbiz-apps/crates/rabbiz-app-kit/     shared runtime + widgets — LEAD owns, agents must not edit
tools/rabbiz-apps/crates/rabbiz-<app>/       one app crate — exactly one agent owns the whole crate
rootfs-overlay/usr/share/applications/rabbiz-<app>.desktop   entries — LEAD
scripts/de_apps_deploy.sh (binaries -> /usr/local/bin/)      LEAD
shell/Rabbiz/Desktop/IconLayout.js (home/dock seed lists)    LEAD
```

Binaries are built for the device with an arm64 container
(`scripts/de_apps_deploy.sh --build`) and installed to `/usr/local/bin/rabbiz-<app>`
by `scripts/de_apps_deploy.sh` — the `.desktop` `Exec=rabbiz-<app>` resolves
them through `PATH`. There is no per-app launcher script and no QML.

## 3. Per-app MVP and exit criteria

| App | Replaces | MVP (must work) | Later |
|---|---|---|---|
| Calculator | `kalk` | keypad (0-9 . ± × ÷ = C ⌫), live expression, result, history list (last 20, tap to reuse), clear history | scientific functions, unit conversion |
| Clock | `kclock` | big clock + date, **Alarm** list (add/delete, persisted, fires a notification through the shell's `org.freedesktop.Notifications`), **Timer** (start/pause/reset, ring + notification), **Stopwatch** (start/lap/reset) | world clock, bedtime mode |
| Notes | `marknote` | list (title + preview + edited time), tap to edit, autosave to `~/.local/share/rabbiz-notes/<id>.json`, new/delete, search box | folders, markdown preview, export |
| Photos | `pix` | grid of thumbnails from `~/Pictures` (+ `~/Downloads`), tap for full-screen viewer with pinch-free zoom (double-tap zoom, drag), swipe left/right between photos, file name + date | albums, rotate/delete, share |
| Music | `elisa` | list audio files from `~/Music` (+ `~/Downloads`), play with `mpv --no-video --input-ipc-server=<runtime>/rabbiz-music.sock`, transport (play/pause/next/prev/seek), now-playing bar, volume follows the system sink | playlists, artwork, MPRIS export |
| Terminal | `qmlkonsole` | command input + scrollback (monospace), runs one command at a time through `script -qfec` (PTY), history up/down, clear, ANSI escapes stripped | full PTY session, tabs |
| Dialer | `gnome-calls` | Keypad / Recents / Contacts tabs, T9 labels, simulated call progression until a real telephony backend exists (`backend.rs` documents the ModemManager/`mmcli` path) | real calls when M6 lands |
| SMS | `chatty` | conversation list → thread view, honest Queued/Sent/Failed states, persisted locally; `mmcli` probed once at startup | real SMS when M6 lands |
| Settings | `plasma-settings` | Wi-Fi/BT/Appearance/Session panels over `Rabbiz.Services`-equivalent system backends | more KCM-style panels |
| Contacts / Files / PDF / Store | `gnome-contacts` / `dolphin` / `papers` / — | per-crate READMEs in `tools/rabbiz-apps/crates/` | — |

Exit criteria for every app: launches from the drawer **and** the home icon;
light/dark and accent follow the desktop contract; back minimizes (not kills);
`--cli` and `--mcp` headless paths work; `cargo test --manifest-path
tools/rabbiz-apps/Cargo.toml --workspace` green; screenshot captured on the device.

## 4. Verification recipe (per app)

```
# Host (no device needed — the kit has a Tier 1 simulator + headless harness):
cargo run --manifest-path tools/rabbiz-apps/Cargo.toml -p rabbiz-calculator
cargo test --manifest-path tools/rabbiz-apps/Cargo.toml --workspace
rabbiz-calculator --cli "2+3"        # pure logic, no window
rabbiz-calculator --mcp              # MCP tool server for agents

# Device (serialized on the shared lock by de_apps_deploy.sh):
./scripts/de_apps_deploy.sh --build root@172.16.42.1   # arm64 build + install
rabbiz-de launch rabbiz-calculator                     # via the launch path
grim /tmp/shot.png                                     # after waking with a21s-power-key fp-unlock
```

## 5. Device verification log

| Date | Apps | Verified |
|---|---|---|
| 2026-11-16 | QML Calculator/Clock/Notes/Photos/Music/Terminal | rendered in shell tone; Clock persisted `alarms.json`; Notes autosave; Music playback via mpv; Terminal PTY ran a command — *the QML tree this log covers was removed 2026-11-20 (see git history)* |
| — | Rust `rabbiz-apps` | Not yet verified on device — record the first run here |
