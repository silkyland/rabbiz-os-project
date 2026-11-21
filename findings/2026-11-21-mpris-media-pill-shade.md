# MPRIS media pill + shade media card (rabbiz-de)

Lane: `de` · Device: device-independent · Evidence: L1 (native `virtual` backend, live session bus)

## What landed

- **`de-system::mpris::Mpris`** — session-bus watcher thread polling
  `org.mpris.MediaPlayer2.*` every 500 ms via `zbus::blocking`
  (`tools/rabbiz-de/crates/de-system/src/mpris.rs`). Feeds the existing
  `MprisTracker`; `generation()` bumps only when the *active* snapshot changes
  (playing player preferred, else most-recently-updated). `request()` forwards
  `PlayPause`/`Next`/`Previous` to the worker — the compositor never blocks on
  D-Bus. `mpris:artUrl` still passes `validate_art_url` (file:// + allowed
  roots only, ≤2 MiB).
- **Status pill** (`de-ui/src/media.rs` + `de-core/shell_ui/media_ui.rs`) —
  ~400×36 capsule centred at y=10 in the 56 px band, drawn as its own
  `DisplayArea::SystemBars` texture (like `gesture_bar`), so the marquee
  re-rasters the pill only — never the whole shell frame. Contents:
  28 px thumbnail (PNG `file://` art via `de_paint::image::decode_png`,
  music-note glyph fallback), ping-pong marquee (`marquee_offset`, 45 px/s,
  1.4 s dwell), equalizer glyph when playing / pause glyph when paused.
  Marquee re-rasters per 50 ms step **only while playing** — paused/stopped
  rasters once and freezes (bounds CPU; also visually honest).
- **Shade media card** — `media_card` (already in `de-ui`, was unwired) now
  pins at the top of the notification panel (`media_notif_rect`); notification
  cards shift below it. `NotifHit::Media` + `media_card::hit_card` route
  prev/play-pause/next taps to `Mpris::request`; body tap is a no-op.
  Cards stay frosted via `render_card_contents` (bg split out of
  `render_card`).

## Verified (native emu, 2026-11-21)

- GNOME Showtime `Stopped` → pill shows title, pause glyph, frozen marquee.
- `PlayPause` → `Playing` → equalizer glyph + marquee advances
  (frames differ, text ping-pongs within clip).
- Shade: pinned card "Video Player / Omelette - Clean Story V3-withsong"
  with prev/pause/next; tap pause → `PlaybackStatus = Paused` via busctl.
- `cargo test -p de-ui -p de-system -p de-core` all green; release build ok.

## Notes / limits

- The emu shares the host session bus (`DBUS_SESSION_BUS_ADDRESS=/run/user/1000/bus`),
  so **host** players appear in the pill — handy for dev; on-device it will be
  the device bus.
- Chrome registers MPRIS per playback; remote `http(s)` art is rejected by
  `validate_art_url` (note-glyph fallback shown). JPEG `file://` art is not
  decoded (`decode_png` only) → same fallback.
- Capture path (`composite_output`) blits the pill above the shade while the
  real display order keeps it under Shade (SystemBars < Shade) — cosmetic
  mismatch in emu captures only.
- Pill is display-only (no touch target) by design; controls live on the
  shade card.
