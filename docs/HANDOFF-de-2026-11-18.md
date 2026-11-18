# DE lane handoff, 2026-11-18 (evening)

Lane `de`, device A21s. Everything below is committed on `main` (`0f72e70`, `d756f42`, `ec02ff3` and earlier). Evidence level: host + container gates pass; the behaviours marked "device" were checked on the A21s with screenshots/logs.

## What exists now

| Area | State | Where |
|---|---|---|
| Home pager | finger-tracked, rubber-band, projected release, spring settle with a wake timer. While paging the home is drawn as **layers moved by the compositor** (no per-frame raster; it was ~59 ms/frame). device | `de-ui/src/pager.rs`, `de-core/src/home_ui.rs`, `frame.rs` (`push_home_layers`) |
| Icon rearrange | long-press lifts, drop on grid cell or dock slot, persisted; dock/grid membership is exclusive; full dock swaps. The lifted icon is its own texture. device | `home_ui.rs`, `de-config/src/prefs.rs` (`drop_on_dock/grid`) |
| Home layout | Stitch light home at 720 px: big clock, search card (mic/camera), 112 px tiles, "Search" pill (opens drawer), 5-slot dock. Goldens re-blessed. device | `de-ui/src/home.rs` |
| Icons | frameless icons sit on the sand tile (AGENTS.md rule amended), Chrome PWA icons resolve across the `gamer`→`rabbiz` rename, glyphs centred on ink bounds | `de-apps/src/icons.rs`, `chrome.rs::draw_ink_centered` |
| Glass | `de_paint::glass`: backdrop blurred once, matte tinted fill, no shadow (no live blur: too costly on the A21s) | `de-paint/src/glass.rs` |
| Panels | pull from top-left = notifications, top-right = quick actions; pull maths in `de-ui/src/pull.rs`; texture over home or app. device | `de-ui/src/panels.rs`, `de-core/src/panel_ui.rs` |
| Policy / menus / input registry | see earlier docs (`DE-PRIVILEGED-PROTOCOLS.md`, `DE-DESKTOP-MODE.md`) | unchanged |

## Honest gaps

- Quick tiles: only Do-Not-Disturb and Auto-rotate toggle, **and only in memory**. Wi-Fi, Bluetooth, airplane, flashlight, hotspot, location, battery saver, screenshot, record, NFC are drawn dimmed with "Not available". Brightness/volume sliders move but control nothing.
- Notifications: the panel is real UI but there is **no `org.freedesktop.Notifications` server**; it always shows "No notifications". `zbus` is already a dependency of `de-system`.
- Glass is only on the two panels. Home search card, pill, dock and the other screens still use flat translucent fills.
- Layered paging is not shown by `grim` before `ec02ff3`; captures now include it.
- Row activation / click-launches-app issue in the shell menu (see `findings/2026-11-18-de-shell-menu-device.md`) is still open.
- `composite_output` vs renderer predicate mismatch (dark capture when no window) still open.
- Phase A acceptance with GTK/Qt/Chrome popups was never run.

## Queue the owner asked for (not started)

Navigation gestures, in the owner's words, restated:
1. Fast swipe up from the bottom = go Home **without killing** the app.
2. Swipe up and hold = the app card follows the finger, background is the **blurred Home** (never the app being closed).
3. Slow swipe stopping mid-screen = Recents, from an app **and** from Home.
4. Swipe up anywhere on Home = App Drawer with the search field (as in the old Qt shell).
5. Swipe left (edge) = Back with a "water drop" effect following the finger; Back **minimizes** (app keeps its state), it does not close.

Findings already made for this (all in `de-core/src/morph.rs`, `input.rs`, `de-ui/src/gestures.rs`):
- The drag-to-close morph only works when the window's recorded launch origin equals the current screen **and** home page (`origin_for`). Apps launched from the dock, another page, the drawer or search get no morph. Plan: synthesize an origin (dock slot, grid cell, or a centred fallback) and force `nav.current = Home` when the drag begins.
- Tracker today: NavBar release emits `Gesture::Home`; stall after 96 px emits `Recents`. Plan: NavBar release becomes `Released{dy,vy}` (like top edge and content) and a pure `decide(dy,vy,h)` picks Home (fast, or past ~75 %), Recents (slow, 20–75 %), Cancel (<20 %); stall-Recents needs ≥ 20 % rise.
- A blurred Home for the background: `glass::blurred_backdrop(&shell_px)` once at drag start, cross-faded by progress.

Backends (owner asked "one at a time"): notification server (zbus) → Wi-Fi/Bluetooth/airplane (NetworkManager / rfkill) → brightness (`/sys/class/backlight`, needs the right privilege) → volume (PipeWire) → the rest.

## Device notes (A21s, `172.16.42.1`)

- Serialize with `scripts/device_session.sh` (waits up to `DEVICE_LOCK_WAIT` s). Another actor sometimes holds the slot with a `bash -se` `sleep 1` loop; do not kill it, wait.
- Deploy: build in `rabbiz-de-build:arm64` (`RUSTUP_TOOLCHAIN=1.99.1-aarch64-unknown-linux-gnu`, `CARGO_TARGET_DIR=…/target-linux-arm64`, `--locked`), `scp` to `/tmp`, `install -m 755` to `/usr/local/bin/rabbiz-de` under the lock, then **reboot** (a session restart is not enough).
- Touch tests: `scripts/device_touch_path.py` (copy to `/root/`, runs on the device): `touch_path.py 'x,y,ms;x,y,ms;…'`, dwell points jitter ±1 px so long-press fires, `HOLD_END_MS` keeps the finger down and writes `/tmp/tp.hold` when it arrives.
- Screenshots: `grim -o DSI-1` as `rabbiz` with `WAYLAND_DISPLAY=wayland-0`, `XDG_RUNTIME_DIR=/run/user/1001`.
- Debug logging: `DE_GESTURE_DEBUG=1` (gesture events), `DE_INPUT_DEBUG`, `RABBIZ_DE_POLICY_LOG=1`; set through a systemd drop-in, not left installed. "home raster slow" is logged when a paging frame exceeds 25 ms.
- Lock/idle: the session user is `rabbiz` (uid 1001).

## Rules worth remembering

- Never `cargo fmt` a whole crate (it reformats other lanes' files); `rustfmt` only the new files.
- Goldens: `scripts/de_golden.sh --bless`, then look at the PNGs; the home golden changed a lot today on purpose.
- Frame loop facts: a frame with no damage requests no vblank, so any animation needs a wake timer (`home_wake_at`, `panel_wake_at`, morph's `next_frame_at`).

## Continuation: navigation release policy (2026-11-18)

Implemented the first navigation slice in `gestures.rs` and `input.rs`:

- Bottom drags release with recent velocity (px/s); a rested finger drops stale velocity.
  A fast upward release (at least 2% rise, ≥500 px/s) or ≥75% rise goes Home;
  a slow 20–75% rise opens Recents; a shorter slow drag cancels.
- Hold-to-Recents requires ≥20% rise and 400 ms at a stable position, measured
  from the last movement beyond the slop, rather than from touch-down.
- An upward content swipe on Home opens Drawer. Back from an app uses the same
  shell handoff as Home; it no longer sends a close request to the client.

Validation: `./scripts/check.sh` passed on the Linux host, including the DE
workspace tests and unchanged golden images. Gesture tests: 14 passed.
**Not tested on device.** No deploy, session restart or reboot was performed.

Remaining navigation work: origin synthesis across launch surfaces/pages,
blurred Home behind the finger-tracked app card, the water-drop Back effect,
and real-device gesture tuning/verification. The notification/backend queue
above has not been started.

## Continuation 2: panel glass, status indicators, device-verified (2026-11-18 late evening)

- **Frosted sheet veil**: the panels were rendering transparent between
  controls (owner asked for a real frosted shade). `panels::render_veil`
  bakes blurred backdrop + `t.scrim`; `frame.rs` pushes it as a separate
  element clipped to the covered rows so the blur always aligns with the
  scene beneath — the controls texture still slides and keeps alpha-0 gaps.
- **Perf, A21s** (`build/evidence/panel-glass-2026-11-18/`): baseline 11.9 %
  of one core while open/close-dragging; glass-v1 blur 112–116 ms/open,
  first raster 77–115 ms; after the `upsample`/no-temp-canvas rework (v2)
  blur 88–89 ms, raster p50 18 ms, frame CPU p95 4.98 ms, queue gap p95
  13.6 ms. Open-startup cost remains the hotspot.
- **Status bar indicators** (`de-api::services`, `de-ui::chrome`): new
  `CellularSlot` (carrier/data_type/volte/strength 0–4), `RingerMode`,
  `alarm_set`; status bar draws carrier label, per-SIM signal-cluster
  (bars + type + VoLTE + slot superscript), wifi level glyph, ringer,
  alarm — Android `SignalClusterView` ordering. Live wiring into
  `status_info()` is deferred (de-system providers: modem absent → hidden).
- **Backends**: `de-system::notifications` (org.freedesktop.Notifications
  server), `controls` (NM/rfkill/backlight snapshot+actions), `power`,
  `fingerprint`; power menu hold-to-confirm + sleep/wake paths in
  `de-core::{power,input,lock_ui,shot_ui}` — all authored by the other
  agent session this evening, review at next pull.
- `check.sh` + golden matrix re-blessed (`status`, `panel-*`, `lock`,
  `power`, `screenshot` scenes changed on purpose).
- Thai locale gaps filled (`lock.fingerprint_hint`, `power.hold_hint`,
  `shot.*`).

Deploy this round: arm64 release built in `rabbiz-de-build:handoff`,
installed `/usr/local/bin/rabbiz-de`, reboot required (session restart
doesn't pick it up).

## Update 21:55 — frosted sheet baked in, pull path is cheap (`6d002be`)

- The separate veil element is gone. Smithay consumed the element list
  front-to-back but the second full-screen texture never landed on top of
  the scene on device (all four elements "rendered" per the trace, yet the
  veil never composited visibly). `panels::backdrop_with_veil` now bakes
  blurred scene + `t.scrim` into the panel texture itself, so the shade is
  a single frosted surface end to end — verified on device for both
  notification and quick sheets.
- Pull latency rework (`panel_ui.rs`): pull-begin no longer does
  `composite_output` + `blurred_backdrop` + a full raster inside the input
  handler (~200-300 ms of dead input). Both panel kinds are pre-rendered
  per-kind at boot, the scene capture is deferred to `panel_prepare`, and
  both the CPU blur and the panel raster run on worker threads
  (`blur_rx`/`render_rx` channels, `content_gen` guards stale landings).
  Measured on A21s: first panel frame ~3 ms after gesture start, drag
  frames ~3-8 ms apart, zero heavy frames mid-pull.
- `push_panel` tracks `tex_kind` so switching panel kinds re-uploads the
  right texture; `pixels`/`stale` are per-kind arrays.
