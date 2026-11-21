# RabbizOS SDK v1 (`rabbiz-sdk`) Developer Guide

- **Status:** v1 (`0.1.0`) · **Lane:** `de`
- **Architecture Plan:** [PLAN-SDK-EMULATOR.md](PLAN-SDK-EMULATOR.md)
- **Companion Guide:** [EMULATOR.md](EMULATOR.md)
- **Seam & Lifecycle Contracts:** [DE-SEAMS.md](DE-SEAMS.md) · [DE-SCENE-INSETS.md](DE-SCENE-INSETS.md) · [DE-APP-LIFECYCLE.md](DE-APP-LIFECYCLE.md)

---

## 1. Architecture Overview

`rabbiz-sdk` is the official application development kit for **RabbizOS**. It provides a stable, SemVer-versioned facade over the first-party application runtime (`rabbiz-app-kit`) and the frozen `de-api` v1.1 seam contract ([DE-SEAMS.md](DE-SEAMS.md)) exposed by the `rabbiz-de` Wayland compositor.

```text
+-------------------------------------------------------------------------+
|                       Your RabbizOS Application                         |
|   Rust (rabbiz-sdk) | C/C++/Zig (C ABI) | WASM (WIT) | Flutter | GTK/Qt |
+-------------------------------------------------------------------------+
           |                    |               |           |         |
           v                    v               v           v         v
+-------------------------------------------------------------------------+
|                   rabbiz-sdk / rabbiz-app-kit Runtime                   |
|  AppState · Canvas (tiny-skia + cosmic-text) · MD3 Widgets · ScrollView |
|  Thai TextBuffer · WindowInsets · ThemeWatcher · Intent · MCP Server    |
+-------------------------------------------------------------------------+
           |                                                |
     [Host macOS/Linux]                              [RabbizOS / Tier 2+3]
           v                                                v
+----------------------------------+      +-------------------------------+
| Tier 1 Host App Simulator        |      | rabbiz-de Wayland Compositor  |
| winit + softbuffer + TestHarness |      | OutputBackend::{Drm, Virtual} |
+----------------------------------+      +-------------------------------+
```

### Core Runtime Guarantees

Every `rabbiz-sdk` target language participates in the same desktop contract:

1. **Physical-Pixel Rendering:** Direct rendering via `Canvas` (`tiny-skia` + `cosmic-text` with Noto Sans + Noto Sans Thai shaping) or Wayland `wl_shm` / `wl_egl` buffers at native device resolution (`720x1600` on Galaxy A21s, `1600x2560` on Xiaomi Pad 7 Pro).
2. **Explicit `WindowInsets` ([DE-SCENE-INSETS.md](DE-SCENE-INSETS.md)):** Status bar, navigation gesture bar, display cutout, and on-screen keyboard (`rabbiz-osk`) insets are delivered via `WindowInsets` (`system_bars`, `cutout`, `ime`, `app_frame()`, `immersive_frame()`).
3. **Cream Glass Live Theme Reactivity:** Automatic live switching between Cream Glass Light and Dark themes (`ThemeWatcher`) without restarting the app.
4. **Thai-Safe Text & IME (`zwp_text_input_v3`):** Extended grapheme cluster cursor navigation and commit handling so base consonants, above/below vowels, and tone marks (`ก้า`, `น้ำ`) never split mid-cluster.
5. **Lifecycle & Freeze Compliance ([DE-APP-LIFECYCLE.md](DE-APP-LIFECYCLE.md)):** Apps flush state before transitioning `Background -> Frozen` (`cgroup.freeze`).
6. **AI-Native MCP Tooling:** Every app can expose a `--mcp` JSON-RPC 2.0 stdio server (`rabbiz_sdk::mcp::serve`) for Universal Search and automated verification.

---

## 2. Quickstart (`rabbiz` CLI)

The unified `rabbiz` CLI (`tools/rabbiz-sdk/crates/rabbiz-cli`) manages project scaffolding, host simulation, emulator sessions, `.rbz` packaging, and installation.

### 2.1 Check Your Environment (`rabbiz doctor`)

```bash
rabbiz doctor
```

`rabbiz doctor` verifies:
- Rust toolchain (`cargo`, `aarch64-unknown-linux-gnu`, `wasm32-wasip2`)
- C ABI header generator (`cbindgen`) and C compiler (`clang` / `zig`)
- Flutter SDK (optional, for `--template flutter`)
- Docker / OrbStack (`rabbiz-emu-runtime:arm64` for Tier 2 Emulator) and QEMU / `vfkit` (for Tier 3 VM)
- USB/Wi-Fi SSH reachability for physical hardware (`root@172.16.42.1`)

### 2.2 Create a New App (`rabbiz new`)

```bash
# Rust native app (default)
rabbiz new pomodoro --template rust --id dev.example.pomodoro

# Other v1 templates:
rabbiz new notes-c       --template c
rabbiz new habit-wasm    --template wasm
rabbiz new weather-fl    --template flutter
rabbiz new reader-gtk4   --template gtk4
rabbiz new player-qt6    --template qt6
```

### 2.3 Run on Simulator, Emulator, or Device (`rabbiz run`)

```bash
# Tier 1: Native macOS/Linux Host App Simulator (< 0.5 s startup)
rabbiz run --target sim

# Tier 2: Full DE Wayland Emulator (~1-2 s startup)
rabbiz run --target emu

# Physical Device (Galaxy A21s / Pad 7 Pro over USB/Wi-Fi SSH)
rabbiz run --target device
```

### 2.4 Package & Install (`rabbiz pack` & `rabbiz install`)

```bash
# Build release binary/component and package into dist/dev.example.pomodoro-0.1.0.rbz
rabbiz pack

# Install into active Tier 2/3 Emulator and hot-reload the app drawer
rabbiz install dist/dev.example.pomodoro-0.1.0.rbz --target emu

# Install onto a connected device
rabbiz install dist/dev.example.pomodoro-0.1.0.rbz --target device
```

---

## 3. Language Targets (v1)

### 3.1 Rust (`rabbiz-sdk`)

The `rabbiz-sdk` crate (`tools/rabbiz-sdk/crates/rabbiz-sdk`) is the idiomatic Rust API re-exporting `rabbiz-app-kit` and `de-api` types under clean modules:

| Module | Key Exports | Purpose |
|---|---|---|
| `rabbiz_sdk::app` | `AppState`, `AppEvent`, `AppFrame`, `AppMeta`, `TextInputCtl`, `run_app` | Core event loop, lifecycle, and IME control |
| `rabbiz_sdk::ui` | `Canvas`, `Theme`, `tokens`, `motion`, `widgets`, `ScrollView`, `TextBuffer`, `icons` | Cream Glass rendering, MD3 widgets, kinetic scrolling, Thai text editing |
| `rabbiz_sdk::insets` | `EdgeInsets`, `WindowInsets` | Safe-area geometry (`system_bars`, `cutout`, `ime`, `app_frame()`, `immersive_frame()`) |
| `rabbiz_sdk::intent` | `Intent` | Typed URI/action dispatch (`tel:`, `sms:`, `file://`, `rabbiz://`) |
| `rabbiz_sdk::storage` | `data_dir`, `config_dir`, `cache_dir` | App-scoped XDG directory helpers |
| `rabbiz_sdk::services` | `ThemeWatcher`, `notify`, `clipboard` | Live Light/Dark theme watcher, notifications, clipboard |
| `rabbiz_sdk::mcp` | `Tool`, `serve` | Built-in `--mcp` JSON-RPC 2.0 tool server |
| `rabbiz_sdk::testing` | `TestHarness`, `GoldenCompare` | Headless pixel rendering and golden PNG assertions |

#### Minimal Rust Application

```rust
use rabbiz_sdk::app::{AppEvent, AppFrame, AppMeta, AppState, TextInputCtl, run_app};
use rabbiz_sdk::insets::WindowInsets;
use rabbiz_sdk::ui::{Canvas, ScrollView, TextBuffer, Theme, widgets};

pub struct CounterApp {
    count: i32,
    note: TextBuffer,
    scroll: ScrollView,
    insets: WindowInsets,
}

impl Default for CounterApp {
    fn default() -> Self {
        Self {
            count: 0,
            note: TextBuffer::with_text("สวัสดี RabbizOS"),
            scroll: ScrollView::new(),
            insets: WindowInsets::default(),
        }
    }
}

impl AppState for CounterApp {
    fn meta(&self) -> AppMeta {
        AppMeta {
            app_id: "dev.example.counter".into(),
            title: "Counter".into(),
            default_width: 720,
            default_height: 1600,
        }
    }

    fn handle_event(&mut self, event: AppEvent, ime: &mut dyn TextInputCtl) -> bool {
        match event {
            AppEvent::InsetsChanged(insets) => {
                self.insets = insets;
                true
            }
            AppEvent::Tap { x, y } => {
                if y > 240 && y < 320 && x > 48 && x < 320 {
                    self.count += 1;
                    ime.activate("normal");
                    return true;
                }
                false
            }
            AppEvent::TextCommit(text) => {
                self.note.insert_str(&text);
                true
            }
            _ => false,
        }
    }

    fn render(&mut self, canvas: &mut Canvas, theme: &Theme, frame: AppFrame) {
        canvas.clear(theme.bg);
        let safe = self.insets.app_frame(frame.width, frame.height);
        widgets::draw_top_app_bar(canvas, theme, safe.x, safe.y, safe.width, "Counter");
        widgets::draw_filled_button(
            canvas,
            theme,
            safe.x + 24,
            safe.y + 96,
            240,
            56,
            &format!("Count: {}", self.count),
        );
        canvas.draw_text(
            safe.x + 24,
            safe.y + 184,
            self.note.as_str(),
            20.0,
            theme.text,
        );
    }
}
```

#### Headless `TestHarness` & Golden PNG Testing

Every Rust app can run deterministic L0 tests on macOS or Linux without a display server:

```rust
use rabbiz_sdk::testing::TestHarness;

#[test]
fn counter_increments_and_matches_golden() {
    let mut harness = TestHarness::new(CounterApp::default(), 720, 1600);
    harness.render_frame();
    harness.tap(120, 280);
    harness.commit_text(" ก้า");
    let frame = harness.render_frame();
    assert_eq!(frame.width(), 720);
    harness.assert_golden("tests/goldens/counter_after_tap.png", 0.005);
}
```

---

### 3.2 C ABI (`librabbiz_sdk` & `include/rabbiz_sdk.h`)

For C, C++, Zig, Go, or Python applications, `tools/rabbiz-sdk/crates/rabbiz-sdk-c` exports a stable C99 ABI (`RABBIZ_SDK_ABI_VERSION = 1`) declared in `tools/rabbiz-sdk/include/rabbiz_sdk.h`. All FFI `unsafe` code is isolated inside `rabbiz-sdk-c`.

#### Key C ABI Types and Entry Points

- **Version Constant:** `RABBIZ_SDK_ABI_VERSION` (`1`)
- **Callbacks Struct (`RabbizAppCallbacksC`):**
  - `on_init(void *userdata)`
  - `on_event(void *userdata, const RabbizEventC *event, RabbizTextInputC *ime)`
  - `on_render(void *userdata, RabbizCanvasC *canvas, const RabbizThemeC *theme, uint32_t width, uint32_t height)`
  - `on_save_state(void *userdata)`
- **Lifecycle & Headless Rendering:**
  - `int32_t rabbiz_app_run(const RabbizAppConfigC *cfg, const RabbizAppCallbacksC *cb, void *userdata)`
  - `int32_t rabbiz_app_render_headless(const RabbizAppConfigC *cfg, const RabbizAppCallbacksC *cb, void *userdata, uint32_t width, uint32_t height, uint8_t *out_rgba, size_t out_len)`
- **Canvas Drawing Primitives (`rabbiz_canvas_*`):**
  - `rabbiz_canvas_clear(RabbizCanvasC *canvas, uint32_t argb)`
  - `rabbiz_canvas_fill_rect(RabbizCanvasC *canvas, int32_t x, int32_t y, uint32_t w, uint32_t h, uint32_t argb)`
  - `rabbiz_canvas_fill_rounded_rect(RabbizCanvasC *canvas, int32_t x, int32_t y, uint32_t w, uint32_t h, float radius, uint32_t argb)`
  - `rabbiz_canvas_draw_text(RabbizCanvasC *canvas, int32_t x, int32_t y, const char *utf8_text, float size_px, uint32_t argb)`
  - `rabbiz_canvas_blit_rgba(RabbizCanvasC *canvas, int32_t x, int32_t y, uint32_t w, uint32_t h, const uint8_t *rgba, size_t len)`
  - `rabbiz_canvas_size(const RabbizCanvasC *canvas, uint32_t *out_w, uint32_t *out_h)`
- **IME, Intent, Storage & Notifications:**
  - `rabbiz_text_input_activate`, `rabbiz_text_input_set_surrounding`, `rabbiz_text_input_deactivate`
  - `rabbiz_intent_open(const char *uri)`
  - `rabbiz_storage_path(uint32_t kind, const char *app_id, char *buf, size_t cap)`
  - `rabbiz_notify(const char *title, const char *body)`

#### Compiling with C, C++, or Zig

```bash
# Build librabbiz_sdk (.dylib on macOS, .so / .a on Linux)
cargo build --manifest-path tools/rabbiz-sdk/Cargo.toml -p rabbiz-sdk-c --release

# Compile with clang (C99 / C++20)
clang -std=c99 -O2 main.c \
  -Itools/rabbiz-sdk/include \
  -Ltools/rabbiz-sdk/target/release -lrabbiz_sdk \
  -o my-c-app

# Cross-compile for RabbizOS ARM64 with Zig CC
zig cc -target aarch64-linux-gnu -std=c99 -O2 main.c \
  -Itools/rabbiz-sdk/include \
  -Ltools/rabbiz-sdk/target/aarch64-unknown-linux-gnu/release -lrabbiz_sdk \
  -o bin/my-c-app
```

---

### 3.3 WebAssembly (`rabbiz-wasm-runner` & `wit/rabbiz-app.wit`)

WebAssembly apps use the WASI Preview 2 Component Model defined in `tools/rabbiz-sdk/wit/rabbiz-app.wit` (`package rabbiz:app@0.1.0`) and execute inside `rabbiz-wasm-runner`.

#### Write-Once-Run-Everywhere

A single `.wasm` binary inside a `.rbz` archive (`arch = ["any"]`) runs unmodified across:
1. **macOS ARM64 / x86_64** in the Tier 1 Host App Simulator
2. **Docker / Linux ARM64** in the Tier 2 Full DE Wayland Emulator
3. **Samsung Galaxy A21s** (`SM-A217F`, Exynos 850 ARM64)
4. **Xiaomi Pad 7 Pro** (`muyu`, Snapdragon 8s Gen 3 ARM64)

#### WIT Contract (`rabbiz:app@0.1.0`)

- **Host Imports:**
  - `canvas`: `clear`, `fill-rect`, `fill-rounded-rect`, `draw-text`, `blit-rgba`, `size`
  - `theme`: `current-theme` (returns Cream Glass Light/Dark palette tokens)
  - `insets`: `current-insets` (`system-bars`, `cutout`, `ime`)
  - `text-input`: `activate`, `set-surrounding`, `deactivate`
  - `intent`: `open-uri` (gated by `[permissions].intents`)
  - `storage`: `read-file`, `write-file`, `list-dir` (scoped to app-private XDG storage)
  - `notifications`: `send` (gated by `[permissions].notifications`)
- **Guest Exports (`app`):**
  - `init() -> app-meta`
  - `handle-event(ev: app-event) -> bool`
  - `render(width: u32, height: u32)`
  - `handle-mcp-tool(name: string, args-json: string) -> result<string, string>`

#### Fuel Sandboxing & Capability Permissions

`rabbiz-wasm-runner` enforces deterministic sandboxing:
- **Per-Frame Fuel Budget:** Each `handle-event` and `render` call receives a bounded execution fuel quota so a runaway guest loop cannot freeze the 60 Hz frame clock or lock up the compositor.
- **Default-Deny Capabilities:** Host imports check the bundle's `rabbiz.toml` `[permissions]` table at runtime:
  - `network = false` denies socket creation.
  - `notifications = false` blocks `notifications.send`.
  - `intents = ["rabbiz://pomodoro/*"]` restricts `intent.open-uri` to matching URI prefixes.
  - `storage = "app-private"` jails filesystem access to `~/.local/share/<app-id>/` and `~/.config/<app-id>/`.

---

### 3.4 Flutter (`rabbiz-flutter-runner` & `packages/rabbiz_sdk_dart`)

RabbizOS provides first-class Flutter support through the `rabbiz-flutter-runner` Wayland embedder (`tools/rabbiz-sdk/crates/rabbiz-flutter-runner`) and the Dart FFI package (`tools/rabbiz-sdk/packages/rabbiz_sdk_dart`).

#### `kSoftware` (`wl_shm`) Default vs. `--gl` Opt-In

- **Default `kSoftware` (`wl_shm`):** On the Galaxy A21s (where Panfrost CMA buffer allocation can exhaust contiguous memory under heavy multitask load; see [../ROADMAP.md](../ROADMAP.md)) and inside the Tier 2 Emulator (`PixmanRenderer`), `rabbiz-flutter-runner` defaults to Flutter's `kSoftware` rasterizer backed by `wl_shm` shared-memory buffers. This guarantees zero CMA leaks and 100% pixel parity between Tier 2 and the physical phone.
- **Opt-In Hardware GL (`--gl` / `wl_egl`):** On devices with ample CMA/IOMMU headroom (such as Xiaomi Pad 7 Pro `muyu` or when testing GPU paths), pass `--gl` or set `flutter_renderer = "gl"` in `rabbiz.toml` to enable `wl_egl` hardware acceleration.

#### Dart FFI Bindings (`packages/rabbiz_sdk_dart`)

The Dart package loads `librabbiz_sdk` via `dart:ffi` and exposes:
- `RabbizThemeWatcher`: streams live Cream Glass Light/Dark palette updates directly into Flutter's `ThemeData` and `ColorScheme`.
- `RabbizWindowInsets`: maps `WindowInsets` (`system_bars`, `cutout`, `ime`) to Flutter `MediaQueryData.viewPadding` and `viewInsets`.
- `RabbizIntent.open(String uri)` & `RabbizStorage`: native intent dispatch and XDG path lookup.
- `RabbizMcpServer`: registers Dart callbacks as `--mcp` tools.

---

### 3.5 Wayland Desktop Contract (GTK4 / Qt6 / SDL / Electron)

Standard Linux Wayland applications (GTK4, Qt6, SDL2/SDL3, Electron) run natively on `rabbiz-de` when packaged or launched under the RabbizOS desktop environment contract (`tools/rabbiz-de/crates/de-system/src/env.rs`).

#### Environment Contract (`de-system/src/env.rs`)

When `rabbiz-de` spawns an application scope, it injects:

| Variable | Value | Purpose |
|---|---|---|
| `XDG_SESSION_TYPE` | `wayland` | Forces native Wayland backend |
| `WAYLAND_DISPLAY` | `wayland-0` | Compositor socket name |
| `QT_QPA_PLATFORM` | `wayland` | Qt6 native Wayland client |
| `QT_QUICK_BACKEND` | `software` | Avoids CMA dmabuf exhaustion on A21s / Tier 2 |
| `GDK_BACKEND` | `wayland` | GTK4 native Wayland client |
| `GDK_DPI_SCALE` | `1.5` (handset) | Scales GTK typography for mobile DPI |
| `SDL_VIDEODRIVER` | `wayland` | SDL native Wayland backend |
| `ELECTRON_OZONE_PLATFORM_HINT` | `wayland` | Chromium/Electron Ozone Wayland |
| `RABBIZ_FORCE_SOFTWARE` | `1` (Tier 2 / A21s fallback) | Instructs clients to use `wl_shm` software buffers |

The full contract — not a subset — is pushed into the D-Bus activation
environment (`dbus-update-activation-environment --systemd --all` in
`rabbiz-session`), so D-Bus-activated helpers, GApplication instances and
portal backends inherit it too.

#### Session D-Bus Surface

Standard Linux applications can rely on these names on the session bus — the
same interfaces they probe for on GNOME/KDE (`AUDIT-DE-STANDARDS-2026-11-21`):

| Well-known name | Provider | Apps get |
|---|---|---|
| `org.freedesktop.Notifications` | `rabbiz-de` (`de-system`) | Shade + heads-up notifications, actions, urgency, hints |
| `org.freedesktop.ScreenSaver` | `rabbiz-de` (`de-system`) | Idle inhibit + activity pokes (video players, calls) |
| `org.mpris.MediaPlayer2.*` | any MPRIS-capable app | Status-bar media pill + shade transport controls |
| `org.freedesktop.UPower` / `.PowerProfiles` | `upower` / `power-profiles-daemon` | Battery state, battery-saver profile |
| `org.freedesktop.portal.*` | `xdg-desktop-portal` + gtk backend (`rabbiz-portals.conf`) | FileChooser, OpenURI, Settings (color-scheme → dark mode), Inhibit, Wallpaper |
| `org.freedesktop.secrets` | `gnome-keyring` (XDG Autostart) | Credential storage (Chromium password store) |
| `org.freedesktop.impl.portal.Notifications` | portal → `rabbiz-de` | Sandboxed-app notifications |

#### Wayland Globals Beyond Core Shell

Besides `xdg_shell`, `layer_shell` (privileged), `text_input_v3`,
`data_control` (privileged) and friends, `rabbiz-de` also advertises:
`wp_single_pixel_buffer`, `wp_cursor_shape_v1`, `zxdg_foreign_v1` (portal
parents), `wp_content_type_v1`, `xdg_toplevel_icon_v1`, `xdg_toplevel_tag_v1`,
`zwp_keyboard_shortcuts_inhibit_v1`, `xdg_system_bell_v1`, and
`wp_presentation` (frame feedback, CLOCK_MONOTONIC).

#### Third-Party Branding Rule ([../AGENTS.md](../AGENTS.md))

> **Never restyle a third-party app.** RabbizOS names and Cream Glass artwork are for first-party `rabbiz-*` apps only. Third-party packages keep the exact `Name=` and `Icon=` their upstream desktop entry ships (e.g., Google Chrome remains `"Google Chrome"` with the Chrome logo; KDE and GNOME apps keep their upstream branding). The only compositor presentation adjustment (`tools/rabbiz-de/crates/de-apps/src/icons.rs`) is seating non-tile icons at ~64% scale on the shared sand tile backdrop so the home grid reads as one cohesive family.

---

## 4. `.rbz` Package Format & `rabbiz.toml` Manifest Reference

Every RabbizOS application is distributed as an `.rbz` bundle managed by `rabbiz-pack` (`tools/rabbiz-sdk/crates/rabbiz-pack`).

### 4.1 `rabbiz.toml` Schema Reference

```toml
[app]
id = "dev.example.pomodoro"           # Reverse-DNS identifier (matches desktop entry & cgroup scope)
name = "Pomodoro"                     # Display name in App Drawer and Recents
version = "0.1.0"                     # SemVer version string
kind = "rust"                         # "rust" | "c" | "wasm" | "flutter" | "wayland"
exec = "bin/pomodoro"                 # Relative path to executable, .wasm component, or bundle
icon = "assets/icon.svg"              # SVG or 256x256 PNG icon
categories = ["Utility"]              # FreeDesktop categories

[runtime]
sdk_version = "0.1"                   # Target rabbiz-sdk contract version
arch = ["aarch64"]                    # ["aarch64"], ["x86_64"], or ["any"] for wasm
form_factors = ["handset", "tablet", "desktop"]

[permissions]
network = false                       # Outbound network access
notifications = true                  # Allow posting desktop/HUN notifications
intents = ["rabbiz://pomodoro/*"]     # Allowed URI schemes/prefixes
storage = "app-private"               # "app-private" | "none"

[mcp]
enabled = true                        # Supports `<exec> --mcp` stdio tool discovery
```

### 4.2 `.rbz` Archive Layout & SHA-256 Verification

An `.rbz` file is a zstd-compressed POSIX tar archive (`.tar.zst`) with deterministic file ordering:

```text
dev.example.pomodoro-0.1.0.rbz
├── manifest.toml                     # Normalized copy of rabbiz.toml
├── dev.example.pomodoro.desktop      # Generated FreeDesktop entry
├── icon.svg                          # App icon
├── bin/
│   └── pomodoro                      # ELF binary, .wasm component, or Flutter AOT bundle
├── assets/                           # Optional bundled assets
└── SHA256SUMS                        # Cryptographic SHA-256 manifest of every file in the archive
```

During `rabbiz install <pkg.rbz>`:
1. `rabbiz-pack` extracts the archive into a temporary staging directory.
2. Every entry listed in `SHA256SUMS` is hashed and verified before any file is moved into place; any mismatch aborts installation immediately.
3. Files are atomically installed into `~/.local/share/rabbiz-apps/<app-id>/`, with the desktop file linked into `~/.local/share/applications/<app-id>.desktop`.

### 4.3 Live Catalog Refresh (`rabbiz-de ctl reload-apps`)

After verifying and installing an `.rbz` bundle, `rabbiz install` sends the `reload-apps` control command to the running compositor:

```bash
rabbiz ctl reload-apps
# or directly inside the session:
rabbiz-de ctl reload-apps
```

`rabbiz-de` rescans `~/.local/share/applications` and `/usr/share/applications` in-place via `de-apps`, updating the App Drawer and Universal Search catalog immediately without restarting the Wayland session or disturbing running windows.

---

## 5. Related Documentation

- **3-Tier Emulator Guide:** [EMULATOR.md](EMULATOR.md)
- **SDK & Emulator Architecture Plan:** [PLAN-SDK-EMULATOR.md](PLAN-SDK-EMULATOR.md)
- **Seam Rules & `de-api` Contract:** [DE-SEAMS.md](DE-SEAMS.md)
- **Scene & Safe-Area Insets:** [DE-SCENE-INSETS.md](DE-SCENE-INSETS.md)
- **App Lifecycle & cgroup v2 Freeze:** [DE-APP-LIFECYCLE.md](DE-APP-LIFECYCLE.md)
- **Contributing & Evidence Levels (L0–L3):** [../CONTRIBUTING.md](../CONTRIBUTING.md)
- **OS-Wide Ground Rules:** [../AGENTS.md](../AGENTS.md)
