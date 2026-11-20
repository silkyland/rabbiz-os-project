# Deep Plan: RabbizOS SDK (`rabbiz-sdk`) & 3-Tier Emulator (`rabbiz-emu`)

- **Date:** 2026-11-20
- **Lane:** `de` (plus `userland` for Tier 3 VM rootfs image scripts) · **Device:** `any` (profiles in [devices/samsung/a21s/device.toml](../devices/samsung/a21s/device.toml), [devices/xiaomi/muyu/device.toml](../devices/xiaomi/muyu/device.toml))
- **Status:** Approved architecture & Wave S1–S7 execution plan (Wave S1 implemented alongside this doc)
- **Seam Contract:** Must preserve frozen `de-api` v1.1 ([DE-SEAMS.md](DE-SEAMS.md)) and pass [check_seams.py](../tools/rabbiz-de/scripts/check_seams.py)

---

## 1. Feasibility & Existing Inventory

Building an official RabbizOS SDK + Emulator is practical because most of the core runtime pieces already exist in the repository:

| Capability | Existing Foundation | Gap to Close |
|---|---|---|
| **App UI & Runtime** | [rabbiz-app-kit](../tools/rabbiz-apps/crates/rabbiz-app-kit/src/lib.rs): `AppState`, `AppEvent`, `Canvas` (`tiny-skia` + `cosmic-text`), MD3 `widgets`, `ScrollView`, Thai-safe `TextBuffer`, `Intent`, `ThemeWatcher`, built-in `mcp` server | Replace host panic stub (`wayland_stub.rs`) with a live host window (`host_window.rs`) + headless `TestHarness` (`harness.rs`); publish stable facade `rabbiz-sdk` + multi-language bindings |
| **Compositor & Software Renderer** | [de-render](../tools/rabbiz-de/crates/de-render/src/target.rs) already shares one scene pipeline across `Gl` (`GlesRenderer`) and `Soft` ([PixmanRenderer](../tools/rabbiz-de/crates/de-render/src/soft.rs)); [de-sim](../tools/rabbiz-de/crates/de-sim/src/main.rs) + [golden.rs](../tools/rabbiz-de/crates/de-sim/src/golden.rs) validate screens on macOS | Decouple `OutputBackend::{Drm, Virtual}` in [de-core/src/runtime/state.rs](../tools/rabbiz-de/crates/de-core/src/runtime/state.rs) and [event_loop.rs](../tools/rabbiz-de/crates/de-core/src/runtime/frame/event_loop.rs) so `rabbiz-de` runs headlessly or streams frames |
| **Control & Mirroring Protocols** | [ctl_ipc.rs](../tools/rabbiz-de/crates/de-core/src/runtime/ctl_ipc.rs) implements JSON-lines runtime control (`get_state`, `lock`, `launch`, `screenshot`, `show_osd`, `reload_theme`, `set_panel`); [DE-MIRROR-PROTOCOL.md](DE-MIRROR-PROTOCOL.md) specifies adaptive frame/input streaming | Add `ReloadApps` + virtual touch/key/hardware-button injection (virtual backend only), plus `rabbiz-emu-proto` zstd dirty-rect streaming + host viewer (`rabbiz-emu-viewer`) |
| **ARM64 Native Speed on Apple Silicon** | Host Mac (`arm64`) matches Galaxy A21s / Pad 7 Pro (`aarch64-unknown-linux-gnu`), with existing [Dockerfile](../tools/rabbiz-de/Dockerfile) (`rabbiz-de-build:arm64`) | Add `Dockerfile.emu` (`rabbiz-emu-runtime:arm64`) bundling `rabbiz-de`, `rabbiz-osk`, 12 `rabbiz-apps`, fonts, `dbus-daemon`, and `llvmpipe` — running ARM64 Linux binaries natively at full speed without QEMU CPU translation |

### Honest Boundaries (Emulator vs. Real Hardware)

- **Cannot be emulated 1:1:** Exynos 850 vendor silicon (`scsc` WLBT Wi-Fi/BT, `abox` Calliope DSP, `s2mu106` charger/FG, `decon`/`dsi` CMA scanout quirks, `cpif` baseband). Those are simulated via `de-system::mock` for DE/app testing.
- **Evidence levels ([CONTRIBUTING.md](../CONTRIBUTING.md) & [LANES.md](LANES.md)):** Passing in `sim` or `emu` counts as **L0 (host-tested)** or **L1 (built/container-verified)** only — never **L2 (on-device)**. Device verification still runs serialized through [device_session.sh](../scripts/device_session.sh).
- **Third-party app rule ([AGENTS.md](../AGENTS.md)):** Never restyle third-party apps; keep upstream names and icons, using only the sand-tile backdrop rule where appropriate.

---

## 2. Four Architectural Pillars

```text
Developer / AI Agent
   │
   ├── `rabbiz` CLI (doctor / new / run / build / pack / install / emu / ctl / test)
   │        │
   │        ├── [Pillar A] Multi-Language `rabbiz-sdk` v1
   │        │      ├─ Rust (`rabbiz-sdk`)
   │        │      ├─ C ABI (`rabbiz-sdk-c` -> `librabbiz_sdk` + `rabbiz_sdk.h`)
   │        │      ├─ WASM (`rabbiz-wasm-runner` + `wit/rabbiz-app.wit`)
   │        │      ├─ Flutter (`rabbiz-flutter-runner` + Dart `rabbiz_sdk` FFI)
   │        │      └─ Wayland Desktop Contract templates (GTK4 / Qt6 / SDL / Electron)
   │        │
   │        ├── [Pillar B] 3-Tier `rabbiz-emu`
   │        │      ├─ Tier 1 (<0.5s): Host App Simulator (`winit` + `softbuffer` + `TestHarness`)
   │        │      ├─ Tier 2 (~1-2s): Full DE Wayland Emulator (`rabbiz-de --backend=virtual`)
   │        │      └─ Tier 3 (~6-12s): Full System VM (`qemu`/`vfkit` + systemd + cgroup v2 + virtio-gpu)
   │        │
   │        ├── [Pillar C] `.rbz` App Bundle & App Lifecycle Contract
   │        │
   │        └── [Pillar D] AI-Native MCP (`rabbiz_sdk::mcp` per app + `rabbiz emu --mcp`)
   │
   └── Real Device (Galaxy A21s / Xiaomi Pad 7 Pro) over USB/Wi-Fi SSH
```

---

## 3. Pillar A — Multi-Language `rabbiz-sdk` (v1 Scope)

All v1 language targets share the exact same runtime contract: **physical-pixel rendering (`Canvas` or `wl_shm`/EGL), `WindowInsets` ([DE-SCENE-INSETS.md](DE-SCENE-INSETS.md)), live Light/Dark theme reactivity, Thai-safe IME (`zwp_text_input_v3`), typed `Intent` routing, and built-in MCP tool exposure.**

### A.1 Rust SDK Facade (`rabbiz-sdk`)

Today apps in [tools/rabbiz-apps/Cargo.toml](../tools/rabbiz-apps/Cargo.toml) reference `../rabbiz-de/crates/de-*` by relative path. `rabbiz-sdk` provides a single public SemVer crate (`0.1` -> `1.0`) re-exporting:

- `rabbiz_sdk::app` — `AppState`, `AppEvent`, `AppFrame`, `AppMeta`, `TextInputCtl`, `run_app`
- `rabbiz_sdk::ui` — `Canvas`, `Theme`, `tokens`, `motion`, MD3 `widgets`, `ScrollView`, Thai-safe `TextBuffer`, `icons`
- `rabbiz_sdk::insets` — `EdgeInsets`, `WindowInsets` (`system_bars`, `cutout`, `ime`, `app_frame()`, `immersive_frame()`)
- `rabbiz_sdk::intent` — `Intent` (`tel:`, `sms:`, `file://`, `rabbiz://`, custom URIs)
- `rabbiz_sdk::storage` — app-scoped XDG helpers (`data_dir(app_id)`, `config_dir(app_id)`, `cache_dir(app_id)`)
- `rabbiz_sdk::services` — client-safe bridges: `notify()`, clipboard read/write, `ThemeWatcher`, save-state lifecycle hook (before `Background -> Frozen` per [DE-APP-LIFECYCLE.md](DE-APP-LIFECYCLE.md))
- `rabbiz_sdk::mcp` — `Tool`, `serve()` from [mcp.rs](../tools/rabbiz-apps/crates/rabbiz-app-kit/src/mcp.rs)
- `rabbiz_sdk::testing` — `TestHarness<S>` + `GoldenCompare` for L0 host tests

### A.2 C ABI (`crates/rabbiz-sdk-c` -> `librabbiz_sdk` + `rabbiz_sdk.h`)

A versioned C99 shared/static library (`RABBIZ_SDK_ABI_VERSION = 1`) generated with `cbindgen`, isolating all FFI `unsafe` boundaries inside `rabbiz-sdk-c` so C, C++, Zig, Go, Python, and Dart can build native RabbizOS apps without Rust toolchains:

- **Opaque handles:** `RabbizApp`, `RabbizCanvas`, `RabbizTextInput`, `RabbizTheme` (copied value snapshot with `bg`, `canvas`, `surface1..3`, `text`, `accent`, `danger`, etc.)
- **App entry:** `rabbiz_app_run(const RabbizAppConfig *cfg, const RabbizAppCallbacks *cb, void *userdata)` mapping directly to `rabbiz_app_kit::run_app` (both macOS Tier 1 simulator and Linux Wayland!)
- **Canvas & text ops:** `rabbiz_canvas_clear`, `rabbiz_canvas_fill_rect`, `rabbiz_canvas_fill_rounded_rect`, `rabbiz_canvas_blit_rgba`, `rabbiz_canvas_draw_text`, `rabbiz_canvas_size`
- **System & IME ops:** `rabbiz_text_input_activate(ti, purpose)`, `rabbiz_text_input_set_surrounding(ti, utf8, cursor_bytes)`, `rabbiz_text_input_deactivate(ti)`, `rabbiz_intent_open(uri)`, `rabbiz_storage_path(kind, app_id, buf, cap)`, `rabbiz_notify(title, body)`
- **MCP export:** `rabbiz_mcp_serve(server_name, version, tools, count)`

### A.3 WebAssembly Component Runtime (`crates/rabbiz-wasm-runner` + `wit/rabbiz-app.wit`)

Architecture-independent, sandboxed `.wasm` apps hosted by `rabbiz-wasm-runner` (`wasmtime` Component Model + WASI Preview 2) implementing `rabbiz_app_kit::AppState`:

- **Single artifact everywhere:** A `.wasm` inside `.rbz` runs unmodified on macOS ARM64/x86_64 (Tier 1), Docker ARM64 (Tier 2), Galaxy A21s, and Xiaomi Pad 7 Pro.
- **WIT interface (`rabbiz:app@0.1.0`):** imports `canvas`, `theme`, `insets`, `text-input`, `intent`, `storage`, `notifications`; exports `app { init, handle-event, render, handle-mcp-tool }`.
- **Capability sandbox:** `rabbiz.toml` `[permissions]` (`network`, `notifications`, `storage`, `intents`) maps directly to WASI/host capabilities enforced by `rabbiz-wasm-runner` (default-deny).
- **Language freedom:** Developers can compile Rust (`wasm32-wasip2`), TinyGo, C/C++, or TypeScript/AssemblyScript components targeting the WIT contract.

### A.4 Flutter Embedder (`rabbiz-flutter-runner` + Dart `rabbiz_sdk`)

First-class Flutter UI support on Wayland tailored for RabbizOS's hardware constraints:

- **Wayland Embedder (`rabbiz-flutter-runner`):** C++/Rust host using `smithay-client-toolkit` + `libflutter_engine.so`:
  - **Default `kSoftware` renderer (`wl_shm`):** Avoids the A21s CMA memory leak ([ROADMAP.md](../ROADMAP.md)) and works seamlessly inside Tier 2 (`PixmanRenderer`) and on the A21s today.
  - **Optional `--gl` (`wl_egl` / EGL):** Enabled per-device via `device.toml` (e.g., Xiaomi Pad 7 Pro `muyu` or post-CMA-fix devices).
- **Dart Package (`packages/rabbiz_sdk`):**
  - `dart:ffi` binding to `librabbiz_sdk` for `ThemeWatcher` (syncs Flutter `ThemeData` / `ColorScheme` live with Cream Glass Light/Dark), `WindowInsets` (`MediaQuery.viewPadding` + `viewInsets.bottom` from IME), `Intent`, `Storage`, and optional headless `--mcp` registration via platform channel.
- **Host workflow:** During development, Flutter apps can run natively on macOS (`flutter run -d macos`) with `librabbiz_sdk.dylib` (Theme + Intent + Storage parity) or inside Tier 2 Emulator (`rabbiz run --target emu`) as `aarch64` Linux bundles.

### A.5 Wayland Desktop Contract Templates (GTK4 / Qt6 / SDL / Electron)

For existing Linux desktop apps packaged as `.rbz`:
- Honor environment variables in [de-system/src/env.rs](../tools/rabbiz-de/crates/de-system/src/env.rs) (`QT_QPA_PLATFORM=wayland`, `QT_QUICK_BACKEND=software`, `GDK_DPI_SCALE=1.5`, `ELECTRON_OZONE_PLATFORM_HINT=wayland`, `RABBIZ_FORCE_SOFTWARE=1`).
- Implement `xdg_toplevel`, `wl_touch`, and `zwp_text_input_v3`.
- Follow the third-party rule in [AGENTS.md](../AGENTS.md): keep upstream app names and icons untouched.

---

## 4. Pillar B — `rabbiz-emu` (3-Tier Emulator Architecture)

### Tier 1: Native Host App Simulator (`rabbiz run --target sim`, `< 0.5 s`)
- Replaces `wayland_stub.rs` with [host_window.rs](../tools/rabbiz-apps/crates/rabbiz-app-kit/src/host_window.rs) (`winit` 0.30 + `softbuffer` 0.4, zero `unsafe`) + pure cross-platform core [sim_config.rs](../tools/rabbiz-apps/crates/rabbiz-app-kit/src/sim_config.rs) + headless [harness.rs](../tools/rabbiz-apps/crates/rabbiz-app-kit/src/harness.rs).
- Supports macOS/Linux Thai IME (`Ime::Commit`), HiDPI pixel-accurate scaling (`A21s 720×1600`, `Muyu 1600×2560`), safe-area chrome + mock OSK overlay (`Cmd+K`), live Light/Dark toggle (`Cmd+L`), orientation/device cycling (`Cmd+R`), PNG screenshot (`Cmd+S`), and automated pixel/golden assertions (`TestHarness`).

### Tier 2: Full DE Wayland Emulator (`rabbiz emu start`, `~1–2 s`, Primary DE Emulator)
Runs the **real `rabbiz-de` compositor, real `rabbiz-osk`, real `rabbiz-apps`, and third-party Wayland clients**:

1. **Backend Abstraction in `de-backend` & `de-core`:**
   - Replace direct DRM fields in [de-core/src/runtime/state.rs](../tools/rabbiz-de/crates/de-core/src/runtime/state.rs) with `OutputBackend::{Drm { .. }, Virtual(VirtualOutput)}`.
   - Default remains `Drm` so on-device A21s behavior is unchanged.
   - Add CLI flags in [flags.rs](../tools/rabbiz-de/crates/de-config/src/flags.rs): `--backend=drm|virtual`, `--size=WxH`, `--refresh=HZ`, `--emu-listen=ADDR:PORT`, `--ctl-listen=ADDR:PORT`.
2. **Shared Scene Pipeline (Zero Compositor Drift):**
   - Factor `assemble_elements<R>()` out of [frame/render.rs](../tools/rabbiz-de/crates/de-core/src/runtime/frame/render.rs) so `Drm` and `Virtual` submit the exact same `DisplayStack`, `WindowMorph`, layers, HUN, OSD, Bouncer, and Cursor elements.
   - `VirtualOutput` renders via `DeRenderer::Soft(PixmanRenderer)` + Smithay `OutputDamageTracker` into an offscreen pixel target and paces frames with a `calloop` timer (`refresh_hz`).
3. **Real Touch & Hardware Button Injection:**
   - Factor raw handlers (`touch_down_raw`, `touch_motion_raw`, `touch_up_raw`, `key_raw`, `hw_button_raw`) in [input/mod.rs](../tools/rabbiz-de/crates/de-core/src/runtime/input/mod.rs) and [input/touch.rs](../tools/rabbiz-de/crates/de-core/src/runtime/input/touch.rs).
   - Unlike `zwlr_virtual_pointer_v1` (which intentionally bypasses `GestureTracker`), the emulator injects genuine `wl_touch` slots so Home swipe, Recents swipe-and-hold, edge back, Shade pull, HUN swipe, and OSK typing behave identically to A21s hardware.
4. **Frame & Input Wire (`rabbiz-emu-proto`):**
   - Small `forbid(unsafe_code)` crate aligned with [DE-MIRROR-PROTOCOL.md](DE-MIRROR-PROTOCOL.md):
     - Server → Viewer: `Hello`, zstd-compressed dirty-rect `Frame` tiles (`0 B` when idle)
     - Viewer → Server: `Touch`, `Pointer`, `Key`, `HwButton` (Power/Vol+/Vol-), `Resize`
   - Doubles as L0/L1 validation for **Rabbiz Link**, allowing Swift `Rabbiz Mirror` to connect later.
5. **Host Execution & Cross-Platform Viewer (`rabbiz-emu-viewer`):**
   - On Linux hosts: runs `rabbiz-de --backend=virtual` directly with isolated `XDG_*` paths under `~/.rabbiz-sdk/avds/<name>/`.
   - On macOS Apple Silicon: runs inside container image `rabbiz-emu-runtime:arm64` (`tools/rabbiz-sdk/Dockerfile.emu`, based on [tools/rabbiz-de/Dockerfile](../tools/rabbiz-de/Dockerfile) + `dbus-daemon`, `Noto Sans Thai`, `Material Symbols`, `rabbiz-de`, `rabbiz-osk`, 12 `rabbiz-apps`, `llvmpipe`) with TCP `127.0.0.1` ports bridged to `rabbiz-emu-viewer` (Rust `winit` + `softbuffer` + bezel/mock-control sidebar).
   - Enables headless CI (`--backend=virtual --headless`) running `de-smoke` and E2E scripts on real `rabbiz-de` without physical hardware.

### Tier 3: Full System VM (`rabbiz emu vm`, `~6–12 s`)
For testing userland layers that containers cannot reproduce:
- `systemd --user` transient scopes, `cgroup.freeze`, `MemoryHigh`, `systemd-oomd` ([DE-APP-LIFECYCLE.md](DE-APP-LIFECYCLE.md)), `polkit`, `libseat`/`logind`, `rabbiz-session.service` crash fallback (`EXIT_CRASH_FALLBACK = 75`), and DRM/KMS via **`virtio-gpu`**.
- Uses QEMU aarch64 (`-accel hvf` on macOS / `-accel kvm` on Linux) or `vfkit` on Apple Silicon with an Arch Linux ARM64 `qcow2` image builder (`scripts/build_emu_image.sh`), sharing `rabbiz-emu-viewer` and `rabbiz ctl`.

---

## 5. Pillar C — `.rbz` App Bundle & `rabbiz` CLI

### Manifest (`rabbiz.toml`) & `.rbz` Bundle
```toml
[app]
id = "dev.example.pomodoro"
name = "Pomodoro"
version = "0.1.0"
kind = "rust"          # rust | c | wasm | flutter | wayland
exec = "bin/pomodoro"  # or app.wasm / flutter_bundle
icon = "assets/icon.svg"
categories = ["Utility"]

[runtime]
sdk_version = "0.1"
arch = ["aarch64"]     # wasm uses ["any"]
form_factors = ["handset", "tablet", "desktop"]

[permissions]
network = false
notifications = true
intents = ["rabbiz://pomodoro/*"]
storage = "app-private"

[mcp]
enabled = true
```
- `.rbz` is a zstd-compressed tar archive containing `manifest.toml`, generated `<id>.desktop`, `icon.svg`, `bin/` (or `.wasm` / Flutter bundle), `assets/`, and `SHA256SUMS`.
- `rabbiz-de ctl reload-apps` refreshes `de-apps` catalog in-place so newly installed `.rbz` apps appear immediately in the Drawer without restarting the session.

### Unified `rabbiz` CLI
- `rabbiz doctor` — checks Rust toolchain, `cbindgen`, `wasm32-wasip2`, Flutter SDK, Docker/QEMU, `rabbiz-emu-runtime:arm64`, and device SSH (`172.16.42.1`)
- `rabbiz new <name> --template rust|c|wasm|flutter|gtk4|qt6`
- `rabbiz run [--target sim|emu|device]`
- `rabbiz build [--release] [--arch aarch64|wasm]`
- `rabbiz pack` / `rabbiz install <pkg.rbz> --target emu|device`
- `rabbiz emu create|start|stop|list|shell|logs|screenshot|record|mock|vm ...`
- `rabbiz ctl <subcmd>` — unified IPC command targeting active `emu` or USB/Wi-Fi device
- `rabbiz test [--golden] [--e2e]`

---

## 6. Pillar D — AI-Native / MCP-First Workflows

- **Per-App MCP (`rabbiz_sdk::mcp`):** Every template ships a `--mcp` tool table integrated with RabbizOS Universal Search ([PLAN-SHELL-APPS-NEXT-2026-11-20.md](PLAN-SHELL-APPS-NEXT-2026-11-20.md)).
- **Emulator MCP (`rabbiz emu --mcp`):** Exposes `emu_screenshot`, `emu_tap`, `emu_swipe`, `emu_type`, `emu_hw_button`, `emu_ctl`, `emu_set_mock`, `emu_install_launch`, and `emu_logs`, letting AI agents build, boot, interact, and visually verify UI changes end-to-end without physical hardware.

---

## 7. Crate & Workspace Layout (Seam-Safe)

```text
tools/
├── rabbiz-de/crates/
│   ├── de-config/src/{flags.rs,ipc.rs}         # + virtual backend flags & emu-gated IPC
│   ├── de-system/src/mock.rs                   # + RABBIZ_MOCK_SYSTEM=1 state overrides
│   ├── de-backend/src/{virtual_output.rs,emu_server.rs}
│   └── de-core/src/runtime/                    # + OutputBackend::{Drm,Virtual} & raw input
│
├── rabbiz-apps/crates/rabbiz-app-kit/src/
│   ├── app.rs                                  # shared AppState/AppEvent/AppFrame
│   ├── sim_config.rs                           # DevicePreset, ViewportFit, chrome, XKB helpers
│   ├── host_window.rs                          # Tier 1 winit + softbuffer runner (zero unsafe)
│   └── harness.rs                              # cross-platform headless TestHarness + golden check
│
└── rabbiz-sdk/                                 # New workspace (lane: de)
    ├── Cargo.toml
    ├── Dockerfile.emu
    ├── wit/rabbiz-app.wit
    ├── include/rabbiz_sdk.h
    ├── packages/rabbiz_sdk_dart/
    ├── templates/{rust,c,wasm,flutter,gtk4,qt6}/
    └── crates/
        ├── rabbiz-emu-proto/                   # wire framing + zstd dirty tiles (forbid(unsafe_code))
        ├── rabbiz-sdk/                         # public Rust SDK facade
        ├── rabbiz-sdk-c/                       # versioned C99 ABI (isolated FFI unsafe)
        ├── rabbiz-wasm-runner/                 # wasmtime Component Model host
        ├── rabbiz-flutter-runner/              # Wayland Flutter embedder (kSoftware default / GL opt)
        ├── rabbiz-pack/                        # rabbiz.toml & .rbz bundler/installer
        ├── rabbiz-emu-viewer/                  # cross-platform host viewer + mock hardware panel
        └── rabbiz-cli/                         # `rabbiz` CLI + `emu --mcp`
```

---

## 8. Execution Waves (S1–S7)

| Wave | Scope & Deliverables | Effort | Verification Gate |
|---|---|---|---|
| **S1** | Tier 1 Host App Simulator (`host_window.rs` winit+softbuffer, `sim_config.rs`, Thai IME, presets, mock OSK/chrome) + `TestHarness` (`harness.rs`) | S (2–3 d) | **L0:** All 12 apps compile & launch interactively on macOS; `cargo test`, `clippy`, `./scripts/check.sh`, and Docker ARM64 `cargo check` pass with 0 warnings |
| **S2** | `rabbiz-de --backend=virtual` (`OutputBackend`, shared `assemble_elements`, `PixmanTarget` + damage tracker, raw input hooks, TCP `ctl`) | M (4–6 d) | **L0+L1:** `check_seams.py` passes; `rabbiz-de --backend=virtual` boots in ARM64 container & passes `de-smoke` + `ctl` |
| **S3** | Tier 2 Interactive Emulator (`rabbiz-emu-proto`, `Dockerfile.emu`, `rabbiz-emu-viewer`) | M (4–6 d) | **L0+L1:** Interactive desktop session on macOS/Linux at 60 FPS with Home/Recents/Shade touch gestures and Thai `rabbiz-osk` input |
| **S4** | `rabbiz-sdk` Rust facade, `rabbiz-pack` (`.rbz` + `ctl reload-apps`), `rabbiz` CLI (`doctor/new/run/build/pack/install/emu/ctl/test`) | M (4–5 d) | **L0+L1:** `rabbiz new hello && rabbiz run --target sim && rabbiz run --target emu` end-to-end |
| **S5** | Multi-Language SDK v1: **S5a** C ABI (`rabbiz-sdk-c` + `rabbiz_sdk.h`), **S5b** WASM (`rabbiz-wasm-runner` + `wit/rabbiz-app.wit`), **S5c** Flutter (`rabbiz-flutter-runner` + Dart `rabbiz_sdk`) | L (6–9 d) | **L0+L1:** Sample C, WASM, and Flutter `.rbz` apps build, pack, pass golden tests, and run inside Tier 2 Emulator |
| **S6** | `de-system::mock` (battery, Wi-Fi/SIM, HUN, calls, fingerprint/lockout, PSI), viewer Mock Hardware sidebar, `rabbiz emu --mcp` | M (3–5 d) | **L0+L1:** Trigger mock notifications/battery/PSI and complete agent tap/screenshot verification loop via MCP |
| **S7** | Tier 3 Full System VM (`scripts/build_emu_image.sh`, `rabbiz emu vm` via QEMU/vfkit + `virtio-gpu`) & GTK4/Qt6 templates + `docs/SDK.md`, `docs/EMULATOR.md` | L (5–7 d) | **L1:** VM boots `systemd --user` scopes, `cgroup.freeze`, `virtio-gpu` DRM probe; **L2** A21s check queued via `device_session.sh` |

---

## 9. Guardrails & Risks

1. **Zero DRM regression:** `OutputBackend::Drm` stays the default path on device hardware; every Wave touching `rabbiz-de` must verify `cargo build --release -p de-bin` for `aarch64-unknown-linux-gnu` and queue L2 verification when hardware is free.
2. **Strict privilege isolation:** TCP listeners (`--emu-listen`, `--ctl-listen`) MUST bind loopback by default and MUST be rejected unless `--backend=virtual` is active (never expose unauthenticated TCP on physical devices; preserve [DE-IPC.md](DE-IPC.md) and [DE-PRIVILEGED-PROTOCOLS.md](DE-PRIVILEGED-PROTOCOLS.md)).
3. **Software rendering parity in Tier 2:** Export `RABBIZ_FORCE_SOFTWARE=1`, `LIBGL_ALWAYS_SOFTWARE=1`, `QT_QUICK_BACKEND=software` inside `Dockerfile.emu` so third-party Wayland clients do not submit opaque `dmabuf` frames to `PixmanRenderer`.
4. **C ABI & Flutter safety:** Isolate all `unsafe` FFI in `rabbiz-sdk-c` (`rabbiz-app-kit`, `rabbiz-sdk`, and `rabbiz-emu-proto` remain free of unnecessary `unsafe`); Flutter defaults to `kSoftware` (`wl_shm`) on A21s/Emulator until CMA/dmabuf issues are resolved.
