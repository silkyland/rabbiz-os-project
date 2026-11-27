# RabbizOS 3-Tier Emulator (`rabbiz-emu`) Guide

- **Status:** v1 (`0.1.0`) · **Lanes:** `de`, `userland`
- **Architecture Plan:** [PLAN-SDK-EMULATOR.md](PLAN-SDK-EMULATOR.md)
- **Companion SDK Guide:** [SDK.md](SDK.md)
- **Device Profiles:** [../devices/samsung/a21s/device.toml](../devices/samsung/a21s/device.toml) · [../devices/xiaomi/muyu/device.toml](../devices/xiaomi/muyu/device.toml)

---

## 1. Overview: Why Three Tiers?

Mobile Linux development involves three distinct feedback loops: single-app UI iteration, full Wayland compositor + shell + multi-app interaction, and kernel/systemd/cgroup userland integration. `rabbiz-emu` addresses each loop with a dedicated tier so you never pay a 10-second VM boot cost just to tweak a button layout.

| Tier | Name | Startup | What Runs | Primary Use Case |
|---|---|---|---|---|
| **Tier 1** | **Native Host App Simulator** | `< 0.5 s` | Single app natively on macOS/Linux via `winit` + `softbuffer` (`host_window.rs`) or headless `TestHarness` | Fast app UI iteration, Thai IME testing, Light/Dark theme toggle, L0 golden pixel tests |
| **Tier 2** | **Full DE Wayland Emulator** | `~1–2 s` | Real `rabbiz-de --backend=virtual`, real `rabbiz-osk`, all 12 `rabbiz-apps`, `dbus-daemon`, streamed to `rabbiz-emu-viewer` | Home/Recents/Shade touch gestures, multi-window morphs, notifications/HUN, `.rbz` hot-install, MCP agent runs |
| **Tier 3** | **Full System VM** | `~6–12 s` | Full Arch Linux ARM64 guest via QEMU (`hvf`/`kvm`) or `vfkit` with `virtio-gpu-pci`, `systemd --user`, `cgroup.freeze`, `polkit`, `libseat` | Testing `systemd --user` transient app scopes, `cgroup.freeze`, `systemd-oomd`, `polkit` rules, and DRM/KMS boot |

---

## 2. Tier 1 — Native Host App Simulator (`< 0.5 s`)

Tier 1 compiles and runs any `rabbiz-sdk` or `rabbiz-app-kit` application directly on your host machine (macOS ARM64/x86_64 or Linux) with zero container or VM overhead.

### 2.1 Launching an App in Tier 1

```bash
# Via the unified rabbiz CLI inside an app project:
rabbiz run --target sim

# Or directly via cargo for any first-party app in tools/rabbiz-apps:
cargo run --manifest-path tools/rabbiz-apps/Cargo.toml -p rabbiz-calculator
cargo run --manifest-path tools/rabbiz-apps/Cargo.toml -p rabbiz-notes
cargo run --manifest-path tools/rabbiz-apps/Cargo.toml -p rabbiz-settings
```

### 2.2 Simulator Environment Variables

Configure the Tier 1 simulator viewport, theme, and overlays via environment variables (`tools/rabbiz-apps/crates/rabbiz-app-kit/src/sim_config.rs`):

| Variable | Values | Default | Description |
|---|---|---|---|
| `RABBIZ_SIM_DEVICE` | `a21s` (`720x1600`), `muyu` (`1600x2560`), `desktop` (`1280x800`) | `a21s` | Selects physical resolution and form-factor preset |
| `RABBIZ_SIM_SIZE` | `<W>x<H>` (e.g., `720x1600`, `1080x2400`) | Preset size | Overrides the simulated physical pixel dimensions |
| `RABBIZ_SIM_THEME` | `light` \| `dark` | `light` | Initial Cream Glass theme palette |
| `RABBIZ_SIM_CHROME` | `1` \| `0` (`true` \| `false`) | `1` | Draws simulated status bar (`top: 48px`), punch-hole cutout, and gesture pill (`bottom: 32px`) |
| `RABBIZ_SIM_OSK` | `1` \| `0` (`true` \| `false`) | `0` | Opens the simulated bottom on-screen keyboard (`ime.bottom = 520px`) on startup |

Example:

```bash
RABBIZ_SIM_DEVICE=a21s RABBIZ_SIM_THEME=dark RABBIZ_SIM_CHROME=1 \
  cargo run --manifest-path tools/rabbiz-apps/Cargo.toml -p rabbiz-calculator
```

### 2.3 Interactive Hotkeys (`Cmd` on macOS / `Ctrl+Shift` on Linux)

While a Tier 1 simulator window is focused:

| Hotkey (macOS) | Action |
|---|---|
| `Cmd+L` | Toggle Cream Glass **Light / Dark** theme live (`AppEvent::ThemeChanged`) |
| `Cmd+K` | Toggle simulated **On-Screen Keyboard (OSK)** inset (`WindowInsets.ime.bottom` `0 <-> 520px`) |
| `Cmd+R` | Rotate orientation (**Portrait <-> Landscape**) and recompute `WindowInsets` |
| `Cmd+B` | Toggle simulated system bar & punch-hole **Chrome** overlay |
| `Cmd+S` | Save a pixel-exact PNG **Screenshot** of the current frame to `./sim-screenshot-<timestamp>.png` |
| `Cmd+Q` | Trigger graceful save-state hook and **Quit** simulator |

### 2.4 macOS Thai IME Support

`host_window.rs` enables `winit` IME event forwarding (`window.set_ime_allowed(true)`). Switching your macOS input source to **Thai (Kedmanee or Pattachote)** forwards composed UTF-8 grapheme clusters via `Ime::Commit` directly into `AppEvent::TextCommit`, matching `zwp_text_input_v3` behavior on physical hardware.

---

## 3. Tier 2 — Full DE Wayland Emulator (`~1–2 s`)

Tier 2 boots the real **RabbizOS Desktop Environment (`rabbiz-de`)** using `OutputBackend::Virtual`, launching real Wayland clients (`rabbiz-osk`, all 12 first-party apps, and third-party GTK/Qt apps) and streaming zstd-compressed dirty rectangles over `rabbiz-emu-proto` to the host `rabbiz-emu-viewer`.

### 3.1 Starting Tier 2

```bash
# Recommended: start via the rabbiz CLI (auto-launches viewer + container/daemon)
rabbiz emu start --device samsung/a21s

# Or invoke rabbiz-de directly on a Linux host / inside the ARM64 container.
# IMPORTANT: build with `--features emu` — touch/key/button injection
# (`inject_touch` & friends) is compiled out without it, so viewer taps and
# drags silently do nothing (`emu_feature_disabled` on the ctl socket).
#   cargo build --release -p de-bin --features emu
rabbiz-de run \
  --backend=virtual \
  --size=720x1600 \
  --refresh=60 \
  --emu-listen=127.0.0.1:5910 \
  --ctl-listen=127.0.0.1:5911
```

> **Security Guardrail ([DE-IPC.md](DE-IPC.md) & [DE-PRIVILEGED-PROTOCOLS.md](DE-PRIVILEGED-PROTOCOLS.md)):** `--emu-listen` and `--ctl-listen` bind to loopback (`127.0.0.1`) by default and are strictly rejected by `rabbiz-de` unless `--backend=virtual` is active. Physical devices (`--backend=drm`) only accept authenticated local Unix domain sockets.

### 3.2 Zero Compositor Drift: Shared Scene Pipeline

`OutputBackend::Virtual` does **not** use a separate mock compositor. In `tools/rabbiz-de/crates/de-core/src/runtime/frame/render.rs`, `assemble_elements<R>()` builds the exact same render graph for both `OutputBackend::Drm` and `OutputBackend::Virtual`:
- `DisplayStack` (wallpaper, home grid, app drawer, recents carousel)
- `WindowMorph` (spring-physics launch/dismiss transitions)
- Wayland client surfaces (`xdg_toplevel`, `wlr_layer_shell_v1` status bar, navigation bar, `rabbiz-osk`)
- Heads-Up Notifications (HUN), Quick Settings Shade, Volume/Brightness OSD, Lock/Bouncer, and touch cursor

`VirtualOutput` renders that scene graph through `DeRenderer::Soft(PixmanRenderer)` with Smithay's `OutputDamageTracker`, encoding only damaged tiles via `rabbiz-emu-proto` ([DE-MIRROR-PROTOCOL.md](DE-MIRROR-PROTOCOL.md)) so idle frames transmit `0 B`.

### 3.3 Native ARM64 Speed on Apple Silicon (`Dockerfile.emu`)

On Apple Silicon Macs (`arm64`), `tools/rabbiz-sdk/Dockerfile.emu` (`rabbiz-emu-runtime:arm64`) runs `aarch64-unknown-linux-gnu` binaries directly on the Apple M-series CPU via Linux virtualization without QEMU CPU instruction translation:
- Boots to the lock screen or home screen in `~1–2 seconds`.
- Bundles `rabbiz-de`, `rabbiz-osk`, all 12 `rabbiz-apps`, `dbus-daemon`, `llvmpipe`, `Noto Sans`, `Noto Sans Thai`, and `Material Symbols Rounded`.
- Exports `RABBIZ_FORCE_SOFTWARE=1`, `LIBGL_ALWAYS_SOFTWARE=1`, and `QT_QUICK_BACKEND=software` so all clients submit `wl_shm` buffers compatible with `PixmanRenderer`.

### 3.4 Host Viewer (`rabbiz-emu-viewer`)

`rabbiz-emu-viewer` (`tools/rabbiz-sdk/crates/rabbiz-emu-viewer`) connects to `127.0.0.1:5910` (`--emu-listen`) and `127.0.0.1:5911` (`--ctl-listen`) and provides:
- **Device Bezel & Safe-Area Frame:** Displays the `720x1600` (A21s) or `1600x2560` (Pad 7 Pro) viewport with optional cutout and gesture zone guides.
- **Real `wl_touch` Slot Forwarding:** Mouse drags inside the viewport inject genuine `touch_down_raw` / `touch_motion_raw` / `touch_up_raw` events into `rabbiz-de`'s `GestureTracker`, so bottom-edge Home swipe, swipe-and-hold Recents, left/right-edge Back swipe, top Shade pull, and `rabbiz-osk` touch typing behave identically to physical A21s glass.
- **Hardware Buttons:** Side buttons for **Power** (short press: screen lock/wake; long press: power menu), **Volume Up**, and **Volume Down** (including `Power + Vol-` screenshot chord).
- **Quick Gesture Triggers:** One-click buttons for Home swipe, Recents hold, Back gesture, Shade pull-down, and Lock/Unlock.
- **Mock Hardware Sidebar:** Live sliders and triggers for battery level, charging state, Wi-Fi RSSI, Dual-SIM carrier bars, incoming call/HUN notifications, fingerprint pass/fail, and PSI memory pressure.

### 3.5 Runtime Control via `rabbiz ctl`

Use `rabbiz ctl` (or `rabbiz-de ctl`) to inspect and script the running Tier 2 or Tier 3 session:

```bash
# Query compositor shell state (active app, lock state, theme, form factor, FPS)
rabbiz ctl state

# Launch an installed application by desktop ID
rabbiz ctl launch org.rabbizos.Calculator

# Inject a single tap or swipe gesture in physical pixels
rabbiz ctl touch tap 360 1240
rabbiz ctl touch swipe 360 1580 360 900 220

# Inject keyboard text or keycodes
rabbiz ctl key text "สวัสดี"
rabbiz ctl key press Escape

# Press hardware buttons (power | volume-up | volume-down)
rabbiz ctl button power
rabbiz ctl button volume-up

# Mutate mock hardware/system state (see Section 5)
rabbiz ctl mock battery --percent 15 --charging false

# Hot-reload the application catalog after installing an .rbz package
rabbiz ctl reload-apps

# Capture a PNG screenshot of the compositor output
rabbiz ctl screenshot --out /tmp/emu-home.png
```

---

## 4. Tier 3 — Full System VM (`~6–12 s`)

Tier 2 runs inside a container or host process and therefore does not run a full Linux kernel boot, `systemd --user` cgroup v2 scope hierarchy, or DRM/KMS scanout driver. **Tier 3 (`rabbiz emu vm`)** boots a complete Arch Linux ARM64 virtual machine when you need to validate userland and system services:

- **`systemd --user` Transient Scopes:** Validates `app-rabbiz-<id>-<uuid>.scope` creation (`MemoryHigh`, `MemoryMax`, `CPUWeight`) per [DE-APP-LIFECYCLE.md](DE-APP-LIFECYCLE.md).
- **`cgroup.freeze` & `systemd-oomd`:** Verifies that backgrounded apps are frozen (`1` written to `/sys/fs/cgroup/.../cgroup.freeze`) after their save-state deadline and thawed on resume.
- **`polkit` & `libseat` / `logind`:** Tests seat acquisition, brightness/power polkit rules, and `rabbiz-session.service` crash-fallback recovery (`EXIT_CRASH_FALLBACK = 75`).
- **DRM/KMS via `virtio-gpu-pci`:** Runs `rabbiz-de --backend=drm` against the kernel `virtio_gpu` DRM driver.

### 4.1 Validating & Building the VM Image ([../scripts/build_emu_image.sh](../scripts/build_emu_image.sh))

Use [../scripts/build_emu_image.sh](../scripts/build_emu_image.sh) to validate the device profile and build the `rabbizos-emu-arm64.qcow2` disk image and launch manifest:

```bash
# Dry-run check: verifies device.toml, rootfs-overlay (/usr/lib rule), and writes
# build/emu-vm/rabbiz-emu-vm.json + systemd drop-in + launch-qemu.sh without root
./scripts/build_emu_image.sh --dry-run --device samsung/a21s

# Build the full 8 GB qcow2 VM image for Galaxy A21s profile
./scripts/build_emu_image.sh --out-dir build/emu-vm --size 8G --device samsung/a21s

# Build for Xiaomi Pad 7 Pro tablet profile
./scripts/build_emu_image.sh --out-dir build/emu-vm-muyu --size 8G --device xiaomi/muyu
```

> **Arch Linux ARM `/usr/lib` Rule ([../AGENTS.md](../AGENTS.md)):** [../scripts/build_emu_image.sh](../scripts/build_emu_image.sh) stages all systemd drop-ins under `/usr/lib/systemd/user/rabbiz-session.service.d/` and verifies that `rootfs-overlay/lib` does not exist, preserving Arch Linux ARM's `/lib -> usr/lib` directory symlink.

### 4.2 Booting Tier 3 (`rabbiz emu vm`)

```bash
# Boot the Tier 3 VM (uses QEMU -M virt -accel hvf on macOS / -accel kvm on Linux, or vfkit)
rabbiz emu vm start --image-dir build/emu-vm

# Or run the generated QEMU launcher directly:
./build/emu-vm/launch-qemu.sh
```

Once booted, Tier 3 forwards TCP ports `127.0.0.1:5910` (`rabbiz-emu-proto`), `127.0.0.1:5911` (`rabbiz ctl`), and `127.0.0.1:2222` (SSH), so `rabbiz-emu-viewer`, `rabbiz ctl`, and `rabbiz install --target emu` work identically across Tier 2 and Tier 3.

---

## 5. Mock Hardware & System State (`de-system::mock`)

When `RABBIZ_MOCK_SYSTEM=1` is active (automatically enabled in Tier 2 and Tier 3), `tools/rabbiz-de/crates/de-system/src/mock.rs` replaces hardware sysfs/D-Bus polling with deterministic, scriptable state that updates the Status Bar, Quick Settings Shade, Lock Screen, and App Lifecycle manager in real time.

### 5.1 Controllable Subsystems

| Subsystem | CLI (`rabbiz ctl mock ...`) | Effects in `rabbiz-de` |
|---|---|---|
| **Battery & Charger** | `rabbiz ctl mock battery --percent 12 --charging false` | Updates battery glyph/percentage in Status Bar, triggers low-battery warning at `<= 15%` |
| **Wi-Fi** | `rabbiz ctl mock wifi --enabled true --ssid "Rabbiz-5G" --strength 85` | Updates Wi-Fi icon bars and Quick Settings tile subtitle |
| **Dual-SIM (AIS / TRUE-H)** | `rabbiz ctl mock cellular --sim1 "AIS:4:5G" --sim2 "TRUE-H:3:LTE"` | Populates Dual-SIM carrier labels (`AIS`, `TRUE-H`), signal bars (`0–4`), and radio technology in Status Bar & Shade |
| **Live HUN Notifications** | `rabbiz ctl mock notify --app "Messages" --title "สมชาย" --body "เจอกัน 18:00 น."` | Slides down a Heads-Up Notification (HUN) banner and adds an entry to the Notification Shade |
| **Incoming Call** | `rabbiz ctl mock call --caller "+66 81 234 5678" --name "กานดา" --state ringing` | Triggers high-priority incoming call HUN with Answer/Decline actions |
| **Fingerprint Sensor** | `rabbiz ctl mock fingerprint --result pass` (or `fail` / `lockout`) | Simulates biometric unlock success, shake-on-fail feedback, or 5-attempt PIN bouncer lockout |
| **Memory PSI Pressure** | `rabbiz ctl mock psi --level high` (`none` \| `low` \| `medium` \| `high` \| `critical`) | Drives `de-core` memory pressure monitor to freeze or reclaim background app scopes ([DE-APP-LIFECYCLE.md](DE-APP-LIFECYCLE.md)) |

---

## 6. AI Agent Loop (`rabbiz emu --mcp`)

`rabbiz emu --mcp` starts a Model Context Protocol (MCP) JSON-RPC 2.0 server over stdio so AI coding agents can autonomously build, install, interact with, and visually inspect RabbizOS applications in a closed loop without needing physical hardware.

### 6.1 Starting the Emulator MCP Server

```bash
rabbiz emu --mcp
```

### 6.2 Exposed MCP Tools

| MCP Tool | Arguments (JSON) | Description |
|---|---|---|
| `emu_screenshot` | `{ "out_path": "/tmp/screen.png" }` | Captures the current Tier 2/3 frame as a PNG and returns dimensions + path |
| `emu_tap` | `{ "x": 360, "y": 1240 }` | Injects a `wl_touch` tap at physical pixel coordinates `(x, y)` |
| `emu_swipe` | `{ "x1": 360, "y1": 1580, "x2": 360, "y2": 800, "duration_ms": 220 }` | Injects a smooth multi-step `wl_touch` drag gesture (e.g., Home swipe, Shade pull, scroll) |
| `emu_key` | `{ "text": "สวัสดี" }` or `{ "keysym": "BackSpace" }` | Commits UTF-8 text (including Thai clusters) or presses an XKB keysym |
| `emu_button` | `{ "button": "power" \| "volume_up" \| "volume_down" }` | Triggers a physical hardware button press |
| `emu_set_mock` | `{ "battery_percent": 20, "charging": false, "wifi_ssid": "RabbizLab", "psi": "medium" }` | Atomically updates `de-system::mock` hardware and sensor state |
| `emu_ctl` | `{ "command": "state" \| "lock" \| "unlock" \| "reload-apps" }` | Executes a compositor `ctl` control command and returns the JSON response |
| `emu_pack_install` | `{ "project_dir": ".", "launch": true }` | Packs the project into an `.rbz` archive, verifies `SHA256SUMS`, installs it, triggers `reload-apps`, and launches it |

---

## 7. Evidence Level Guardrails (L0/L1 vs. L2 Hardware)

Per [../CONTRIBUTING.md](../CONTRIBUTING.md) and [LANES.md](LANES.md), every PR and verification report must state its evidence level honestly:

1. **L0 (`host-tested`) & L1 (`built / container / VM verified`):**
   - Passing tests in **Tier 1 (`rabbiz run --target sim` / `TestHarness`)**, **Tier 2 (`rabbiz-de --backend=virtual`)**, or **Tier 3 (`rabbiz emu vm`)** qualifies as **L0** or **L1**.
   - Never claim **L2 (`on-device verified`)** or **L3 (`on-device reproduced`)** based solely on simulator or emulator runs.
2. **What the Emulator Cannot Prove:**
   - Exynos 850 vendor silicon drivers (`scsc` Maxwell WLBT Wi-Fi/BT, `abox` Calliope audio DSP, `s2mu106` fuel gauge/charger, `decon`/`dsi` scanout + Panfrost CMA allocation quirks, `cpif` Shannon modem) only exist on physical hardware.
3. **Serializing Real Hardware Verification (L2/L3):**
   - When verifying on the physical Samsung Galaxy A21s (`root@172.16.42.1`), always serialize deployment, touch injection, and power/reboot steps through [../scripts/device_session.sh](../scripts/device_session.sh) as documented in [DEVICE-QUEUE.md](DEVICE-QUEUE.md).

---

## 8. Related Documentation

- **RabbizOS SDK v1 Guide:** [SDK.md](SDK.md)
- **Deep Architecture Plan:** [PLAN-SDK-EMULATOR.md](PLAN-SDK-EMULATOR.md)
- **Mirror & Frame Streaming Protocol:** [DE-MIRROR-PROTOCOL.md](DE-MIRROR-PROTOCOL.md)
- **App Lifecycle & cgroup Freeze:** [DE-APP-LIFECYCLE.md](DE-APP-LIFECYCLE.md)
- **Compositor Control IPC:** [DE-IPC.md](DE-IPC.md)
- **Hardware Test Serialization Queue:** [DEVICE-QUEUE.md](DEVICE-QUEUE.md)
- **Contributing & Evidence Levels:** [../CONTRIBUTING.md](../CONTRIBUTING.md)
