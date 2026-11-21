# DE Standards Audit — Standard API coverage of `rabbiz-de`

- **Date:** 2026-11-21 · **Lane:** `de` · **Device:** device-independent (a21s notes flagged)
- **Scope:** which freedesktop/Wayland/D-Bus/XDG standards RabbizOS already speaks,
  which are missing, and which custom paths should migrate to a standard backend —
  so third-party native apps behave like they do on GNOME/KDE without app-specific
  workarounds.
- **Method:** read `de-core` global registration, `de-system` backends, `de-apps`
  catalog/launch, `rabbiz-session` + unit, rootfs-overlay, DE-PLAN M5.

## 1. Already standard (keep)

| Layer | Standard | Where |
|---|---|---|
| Shell | `xdg_wm_base`, `xdg_popup`, `zxdg_decoration_v1`, `xdg_activation_v1` | `de-core/protocols/xdg.rs` |
| Shell | `zwlr_layer_shell_v1` (privileged-gated) | `de-core`, `de-config/privileged.rs` |
| Input | `wl_seat` ptr/kbd/**touch** (incl. `wl_touch.frame` — verified w/ Chrome) | `runtime/input/` |
| IME | `zwp_text_input_v3`, `zwp_input_method_v2`, `zwlr_virtual_keyboard_v1` | `runtime/osk.rs` |
| Selection | `wl_data_device`, `zwp_primary_selection_v1`, `ext_data_control_v1` (gated) | `protocols/selection.rs` |
| Buffer | `wl_shm`, `zwp_linux_dmabuf_v1` (+feedback), `wp_viewporter`, `wp_fractional_scale_v1` | `runtime/state.rs` |
| Output | `wl_output`, `zxdg_output_manager_v1` | `runtime/state.rs` |
| Session | `ext_session_lock_v1`, `zwp_idle_inhibit_v1`, `zwp_relative_pointer_v1`, `zwp_pointer_constraints_v1` | `runtime/state.rs` |
| Privileged | `zwlr_screencopy_v1`, `zwlr_virtual_pointer_v1` behind exe-path allow-list | `protocols/screencopy.rs`, `privileged.rs` |
| D-Bus | **`org.freedesktop.Notifications`** — full server w/ actions, urgency, hints, persistence | `de-system/notifications.rs` |
| D-Bus | **`org.mpris.MediaPlayer2.*`** — watcher + transport control | `de-system/mpris.rs` (new) |
| Apps | `.desktop` entry spec — XDG dirs, `Hidden`/`NoDisplay`, actions, dedupe | `de-apps/catalog.rs` |
| Env | `XDG_SESSION_TYPE/CURRENT_DESKTOP`, toolkit vars; `dbus-update-activation-environment` | `de-system/env.rs`, `rabbiz-session` |
| System | seatd, NetworkManager (nmcli), BlueZ (bluetoothctl), PipeWire (wpctl), polkit | unit + `controls.rs` |
| Icon | XDG icon dirs + pixmaps fallback | `de-apps/icons.rs` |

## 2. Missing standards — ranked by "app breaks/degrades without it"

### P0 — standard apps actively need these

| Gap | Why apps need it | Effort | Note |
|---|---|---|---|
| **`xdg-desktop-portal` + backend** | Chrome/GTK/Qt file dialogs, OpenURI, Settings(color-scheme/accent), Screenshot, Inhibit. Today: no `rabbiz-portals.conf`, no backend in rootfs. File picker in Chrome = dead. | M | Already planned **M5-T5** (xdg-desktop-portal-gtk + conf). Needs `xdg_foreign` (below). Do it. |
| **`zxdg_foreign_v1`** | Portal dialogs need a parent handle; required by M5-T5. | S | Smithay has `xdg_foreign` support — register + export. |
| **Portal `Settings` (color-scheme)** | GTK4/Electron dark-mode propagation. Our `ThemeWatcher` is SDK-internal; third-party apps read `org.freedesktop.impl.portal.Settings`. Without it: Chrome stays light under dark theme. | S-M | Either ship portal-gtk or implement `Settings` iface in a `rabbiz-settingsd` user service over our existing theme source — feeds *all* toolkits. |
| **Secret Service `org.freedesktop.secrets`** | Chrome/Chromium stores passwords via it; absent → falls back to plaintext-ish basic store + a warning. gnome-keyring (or `keepassxc --secret-service`) as a `systemd --user` unit. | S (pkg) | Rootfs packaging, not code. |
| **dbus activation env is a subset** | `rabbiz-session:156` pushes only `WAYLAND_DISPLAY DISPLAY XDG_*_DESKTOP`. D-Bus-activated apps (GApplication, kdeinit-style helpers, portal backends) miss `QT_QPA_PLATFORM`, `MOZ_ENABLE_WAYLAND`, `GDK_DPI_SCALE`, `ELECTRON_OZONE_PLATFORM_HINT`, `XKB_*`. | XS | Pass full `env::EXPORTS` list to `dbus-update-activation-environment` (and `systemctl --user import-environment`). |

### P1 — mobile-parity / UX gaps

| Gap | Why | Effort |
|---|---|---|
| **`org.freedesktop.ScreenSaver`** | Video players/browsers `Inhibit`/`SimulateUserActivity` → maps to our idle-inhibit/blank policy. Small zbus shim in de-system. | S |
| **`iio-sensor-proxy`** | Standard accelerometer/ALS → auto-rotate + auto-brightness, the API GNOME/KDE/Android-parity tools expect. Currently no rotation handling at all. | M (driver + service) |
| **UPower D-Bus** | We read sysfs directly (`upower.rs` — fine for us) but *other* apps/widgets read `org.freedesktop.UPower`. Install `upower`; keep sysfs fast-path for status bar. | S (pkg) |
| **power-profiles-daemon** | `battery_saver` toggle writes `scaling_governor` sysfs directly (`controls.rs:301`). Standard = `org.freedesktop.UPower.PowerProfiles` — apps (games) can read active profile; NM/GNOME show it. | S (pkg + worker swap) |
| **MIME associations (`mimeapps.list`)** | No "open with" / default-browser resolution exists; catalog ignores `MimeType=`. Needed for Files app, portal OpenURI, and Chromium's default-browser checks. | S-M |
| **`~/.config/autostart` (XDG Autostart)** | User-installed agents (e.g. `mako`, `kdeconnect`, input tools) expect it; nothing scans it. | S |
| **`wp_presentation` (presentation-time)** | mpv/browser A/V sync + frame feedback; smooth video on 60 Hz panel. | M |
| **`wp_single_pixel_buffer_v1`** | 1×1 solid buffers — cheaper panel/pill/solid surfaces; trivially hand-implemented. | XS |
| **`wp_cursor_shape_v1`** | Apps set standard cursor shapes (desktop-mode pointer UX); today cursor is compositor-fixed. | S |

### P2 — ecosystem breadth

| Gap | Why |
|---|---|
| `ModemManager` | Dialer/SMS standard backend (today: `a21s-modem-init` + custom); also unlocks telephony API for apps. |
| `geoclue2` | `privacy.location` flag is stubbed `false` (`privacy.rs:36`); browser geolocation needs it. |
| `org.freedesktop.StatusNotifier{Watcher,Item}` | Tray icons for Electron/legacy apps (Discord, kdeconnect). Mobile UX hides tray — optional unless desktop mode. |
| `ext_image_copy_capture_v1` | Successor to wlr-screencopy; needed for portal **ScreenCast** (consent-based capture, option E in DE-PRIVILEGED-PROTOCOLS §4). |
| `xdg_toplevel_icon_v1`, `xdg_dialog_v1`, `ext_idle_notify_v1`, `zwp_tablet_v2` (stylus for Pad 7 Pro), `zwp_keyboard_shortcuts_inhibit_v1` (games) | Small compat wins; none blocking. |
| `XWayland` | Planned **M5-T1** — not started. |
| `gvfs`, `tumbler` (thumbnails), `udisks2` (USB storage in Files) | Needed when the Files app gets real; defer to that milestone. |

## 3. Custom paths that could move to a standard backend

| Today | Standard alternative | Verdict |
|---|---|---|
| `upower.rs` sysfs battery poll | `org.freedesktop.UPower` | Keep sysfs (worker-side, zero-cost, works without UPower running); **add** upower package so third-party widgets see battery too. |
| `controls.rs` nmcli/bluetoothctl/wpctl CLI shell-outs | NM/BlueZ/PipeWire D-Bus APIs | Keep — CLIs are already the standard tool surface; a Rust D-Bus rewrite buys little. Only swap if CLI latency shows in quick-settings. |
| `controls.rs` governor sysfs for battery-saver | power-profiles-daemon | **Swap** — same sysfs under the hood, but gives apps the standard `PowerProfiles` property. |
| `fingerprint.rs` et5xx sysfs ABI | fprintd | Keep — no upstream fprintd driver for this sensor; documented device contract. Revisit only if an fprintd backend is ported. |
| Custom `Privileged` allow-list | `wp_security_context_v1` + per-app sandbox | Keep B (implemented + device-verified); D is the real fix tied to M5 sandboxing discussion — not compositor work yet. |
| `rabbiz-de ctl` JSON IPC | — | Internal dev/emu surface; not an app-facing API. Fine as-is. |

## 4. Recommended order

1. **XS this week:** full env into dbus-activation (§2 P0), `wp_single_pixel_buffer`, autostart scan, xdg-user-dirs-update at session start.
2. **S:** `rabbiz-portals.conf` + `xdg_foreign` + portal-gtk (M5-T5), gnome-keyring user unit, `ScreenSaver` shim, `wp_cursor_shape`, UPower + power-profiles-daemon packages, `mimeapps` resolution in catalog.
3. **M:** `Settings` portal backend (theme → all toolkits), presentation-time, iio-sensor-proxy + auto-rotate, XWayland (M5-T1).
4. **L/track with M5+:** security_context sandboxing, ScreenCast capture, ModemManager, geoclue, StatusNotifier (desktop mode), tablet protocol.

## 5. SDK implications (`docs/SDK.md`)

- §3.5 Wayland contract table should list the D-Bus surface apps can rely on
  (`org.freedesktop.Notifications`, `org.mpris.MediaPlayer2`, portals once landed)
  and the env contract it already documents — one table = "the desktop contract".
- `rabbiz doctor` should check portal backend + Secret Service presence on the
  target, same as it checks QEMU/Docker today.
- WIT/C ABI: `storage` already XDG-scoped; add `media` (MPRIS publish for SDK
  apps — a music app written with rabbiz-sdk should appear in the pill/shade
  without hand-rolling zbus).

## 6. Landed in the standards wave (same day)

**Compositor globals** (`de-core/protocols/standards.rs`): `wp_single_pixel_buffer`,
`wp_cursor_shape_v1`, `zxdg_foreign_v1`, `wp_content_type_v1`,
`xdg_toplevel_icon_v1`, `xdg_toplevel_tag_v1`,
`zwp_keyboard_shortcuts_inhibit_v1` (activate-on-request), `xdg_system_bell_v1`
(→ haptic tick), `wp_presentation` + feedback beside `send_frames_surface_tree`.
Deferred by design: `ext_idle_notify_v1` (needs `EventLoop<De>` timers — ours is
`EventLoop<Run>`), `zwp_pointer_gestures_v1` (no libinput gesture forwarding yet),
`wp_security_context_v1` (M5 sandbox work).

**de-system**: `screensaver.rs` — `org.freedesktop.ScreenSaver`
(Inhibit/UnInhibit/SimulateUserActivity/SetActive + GetSessionIdleTime wired to
the compositor's real idle clock); `controls.rs` battery-saver →
`powerprofilesctl` with cpufreq-sysfs fallback.

**de-apps**: `mime.rs` (`mimeapps.list` defaults + added/removed + `MimeType=`
merge), `autostart.rs` (Hidden/OnlyShowIn/NotShowIn/TryExec/X-GNOME-*), plus
`launch_command` and the `rabbiz-de autostart` subcommand.

**Session/rootfs**: `dbus-update-activation-environment --systemd --all`,
`xdg-user-dirs-update`, once-per-boot `rabbiz-de autostart`,
`usr/share/xdg-desktop-portal/rabbiz-portals.conf` (gtk backend; Screenshot/
ScreenCast refused — shell owns capture).

**Remaining from §4:** portal-gtk + gnome-keyring + upower +
power-profiles-daemon package installs are rootfs-image work; `Settings`
portal coverage arrives with portal-gtk; M-track items (presentation is now
landed — iio-sensor-proxy, XWayland, security_context, ModemManager, geoclue,
StatusNotifier) stay on the M5+ plan.

## 7. Runtime verification (host emulator, 2026-11-21)

Post-wave release build hit a main-thread self-deadlock: `MutexGuard` from
`layer_map_for_output` was still held when `emit_presentation_feedback` re-locked
the same `LayerMap` mutex — ctl/emu sockets bound but never responded
(`findings/2026-11-21-layermap-mutex-self-deadlock.md`). Fixed with `drop(map)`
before both emit calls in `render.rs`.

Verified live on the fixed binary:

- `wayland-info` (emu): all nine new globals advertised —
  `wp_single_pixel_buffer_manager_v1`, `wp_cursor_shape_manager_v1` v2,
  `zxdg_exporter_v2`/`zxdg_importer_v2`, `wp_content_type_manager_v1`,
  `xdg_toplevel_icon_manager_v1`, `xdg_toplevel_tag_manager_v1`,
  `zwp_keyboard_shortcuts_inhibit_manager_v1`, `xdg_system_bell_v1`,
  `wp_presentation` v2 (CLOCK_MONOTONIC).
- ctl TCP + emu frame socket: `get_state` → `ok:true`; `frame 720 1600` streams.
- `org.freedesktop.ScreenSaver` under `dbus-run-session`: name claimed, full
  interface introspected (Inhibit/UnInhibit/SetActive/SimulateUserActivity/
  GetActive/GetActiveTime/GetSessionIdleTime). On the host session bus the name
  stays owned by kwin — expected collision, not an implementation failure.
- `cargo test` workspace green; release build clean.

Still unverified end-to-end (host-limited): `rabbiz-de autostart` real launch
(unit tests cover the filters; device e2e where `XDG_CURRENT_DESKTOP=Rabbiz`),
power-profiles sysfs fallback path, portal OpenURI against a real backend.

### Follow-up fixes during §7 verification

- `render.rs`: `drop(map)` before `emit_presentation_feedback` — the held
  `MutexGuard<LayerMap>` self-deadlocked the event loop on first frame
  (`findings/2026-11-21-layermap-mutex-self-deadlock.md`).
- Vendored `smithay 0.7.0` → `vendor/smithay` + `[patch.crates-io]`:
  `ShmBufferUserData::remove_destruction_hook` predicate was `!=` (upstream
  fixed on main, unreleased). Stale icon-buffer hooks posted `NoBuffer` and
  killed Chrome on first commit. Verified: Chrome + GTK apps map and accept
  input on the patched build.
