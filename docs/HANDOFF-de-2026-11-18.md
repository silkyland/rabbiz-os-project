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
