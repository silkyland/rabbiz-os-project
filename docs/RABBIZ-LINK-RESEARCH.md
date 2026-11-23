# Rabbiz Link — Deep Dive & Architecture Research (Part A)

- **Lanes:** `de` (server, compositor integration), `userland` (packaging, services), `docs` (spec)
- **Target device:** `samsung/a21s` (SM-A217F, Exynos 850, Mali-G52 MC1 / Panfrost) first; nothing assumes Exynos-only paths outside `devices/`
- **Base commit:** `6ccbc6b` (spec prompt base `9a908d7`)
- **Status:** **Part A in progress — A1, A2, A6 complete from code & verified findings; A3, A4, A5, A7 L2 on-device measurements pending A21s USB/Wi-Fi connection.**

> **Rule:** Per the Part A / Part B gate, no product code is written in Part A, and Part B does not start until the L2 measurements in A3, A4, A5, and A7 are collected and attached.

---

## A1. Inventory of What Exists

### A1.1 Current Pixel-Producing and Input-Injecting Paths

| Path | Location & Owner | Mechanism | Cost / Frame | Security Today | Fate in Rabbiz Link |
|---|---|---|---|---|---|
| **`EmuFrameServer`** (`--emu-listen`) | `de-core/src/runtime/ctl_ipc.rs` (`de` lane) | Non-blocking TCP server (`virtual` backend only). Accepts `HELLO`, `FRAME` (PNG), `RAW`, `RAW_IF_NEWER <seq>`, and JSON `IpcRequest`s. Calls `composite_emu_pixels()` → `composite::composite_output()` | CPU full-output composite (`Canvas::filled` + `blit_surface` of shm buffers) + optional `de_paint::encode_png`. Runs synchronously on compositor thread! | Unauthenticated TCP listener; gated to `--backend=virtual` | **Consolidate (B8):** Keep emulator on a single frame-transport path (either move `rabbiz-emu-viewer` onto Rabbiz Link over loopback/Unix socket or share the `de-capture` seam so there is one frame-transport path, not two). |
| **`rabbiz-emu-viewer`** | `tools/rabbiz-sdk/crates/rabbiz-emu-viewer/src/lib.rs` (`de`/`sdk` lane) | Connects to `EmuFrameServer` via `rabbiz_emu_proto` (`RAW_IF_NEWER`), renders `720×1600` + 320 px Studio Dock; sends `InjectTouch` / `InjectKey` / `InjectHwButton` | Host-side polling over TCP loopback | Localhost TCP (emulator only) | **Update in B8** to use the unified frame-transport path. |
| **`zwlr_screencopy_manager_v1`** | `de-core/src/protocols/screencopy.rs` (`de` lane) | Wayland global (`Copy`, `CopyWithDamage`). Calls `composite::capture_output(self)` on the compositor thread and copies into client `wl_shm` (`Argb8888`/`Xrgb8888`) | **CPU-only `composite_output()`**: skips dmabuf/EGL surfaces (they render as `CLEAR` `[22,16,26,255]`). Synchronous on compositor thread | Filtered by `Privileged::Screencopy` (`de-config/src/privileged.rs`: `/usr/bin/grim`, `/usr/bin/wayvnc`, OSK). Returns blank `CLEAR` frame when `de.nav.current == Screen::Lock` | **Narrow / Replace for mirroring & screenshots (B2):** Move screenshots and mirroring onto the in-compositor capture seam (`de-capture`); remove `/usr/bin/wayvnc` from default allow-list (or keep `grim` only if needed for CLI tools). |
| **`rabbiz-screenshot`** | `rootfs-overlay/usr/local/bin/rabbiz-screenshot` (`userland` lane); also `ctl_ipc::ctl_screenshot` & `power.rs` | Shell script invokes `grim` over `zwlr_screencopy_manager_v1` and writes `$RUNTIME/a21s-screenshot.state`. `ctl_ipc` also has `IpcCommand::Screenshot { save }` calling `capture_output()` + `encode_png` | Same CPU `composite_output()` + PNG encode on compositor thread | `grim` is in `Privileged::Screencopy`; `ctl_ipc` blocks `Screenshot` while locked | **Reuse `de-capture` seam (B2):** Move `ctl_screenshot` and `rabbiz-screenshot` onto the shared capture seam so screenshots also capture GPU/dmabuf apps and never stall the compositor. |
| **Debug frame dump** (`rabbiz-de.shot`) | `de-core/src/runtime/frame_io.rs` (`dump_frame_if_triggered`) | Polled every 500 ms in `event_loop.rs`; writes `rabbiz-de-shot.ppm` | CPU `composite_output()` + PPM write | `$XDG_RUNTIME_DIR` file trigger; shm-only | **Leave or route via `de-capture`** (debug-only). |
| **`a21s-wayvnc`** | `scripts/mirror_setup.sh`, `a21s-wayvnc.service` (`userland` lane) | `wayvnc 0.10.2` (`neatvnc`, `aml`) as `rabbiz` user; uses `wlr-screencopy` + `zwlr_virtual_pointer_v1` + `zwp_virtual_keyboard_v1`; listens on `172.16.42.1:5900` and `127.0.0.1:5900` | Unknown on A21s (not yet measured; uses `wlr-screencopy` shm path so GPU clients are invisible) | **Unauthenticated (`None`)** on both `usb0` and `127.0.0.1` (finding `2026-11-18-de-wayvnc-unauthenticated-localhost.md`) | **Retire default autostart (B4):** Keep package installable for outside tools, but disabled by default and never auto-started. |
| **`ctl_ipc` `Inject*` commands** | `de-core/src/runtime/ctl_ipc.rs` (`InjectTouch`, `InjectKey`, `InjectHwButton`) | JSON-lines on `$XDG_RUNTIME_DIR/rabbiz-de-ctl.sock` (`0600`) or `--ctl-listen` TCP (virtual mode) | Negligible | `0600` Unix socket (same-user); **`command_allowed_while_locked` currently allows `InjectTouch`/`InjectKey`/`InjectHwButton` while locked!** | **Restrict (B5):** Remove/gate `Inject*` outside `--backend=virtual` (coordinate with hardening), and route Rabbiz Link input directly inside the compositor. |
| **Virtual input globals** (`zwlr_virtual_pointer_v1`, `zwp_virtual_keyboard_v1`) | `de-core/src/protocols/virtual_pointer.rs`, `de-core/src/runtime/osk.rs` | Wayland globals dispatching into `De::pointer_motion`/`pointer_button`/`pointer_axis_frame` and seat keyboard | Negligible | Filtered by `Privileged::VirtualPointer` (`wayvnc`, OSK) and `Privileged::VirtualKeyboard` (`wayvnc`, OSK). Note: `pointer_shell_owned` + `locked()` checks route clicks on lock screen to `lock_tap` | **Leave for OSK / optional external tools**, but Rabbiz Link injects directly inside the compositor (B5) so it does not depend on Wayland virtual globals and supports multi-touch slots + hw buttons. |
| **Rabbiz Mirror (Swift RFB client)** | `tools/rabbiz-mirror/Sources/RabbizMirror/` (~2,000 lines Swift) | Raw BSD TCP socket to port 5900, RFB 3.8 handshake (Security None), ZRLE decoder (`ZRLEDecoder.swift`, `Inflater.swift`), `Framebuffer.swift` (`CGContext`), `MirrorView.swift` | Host-side ZRLE decode + `CGContext` draw | No auth, no encryption | **Rewrite from spec & delete RFB/ZRLE/Inflater (B6)** once parity is reached. |

### A1.2 Rabbiz Mirror Feature Parity Checklist (Must Preserve in Rewrite)

From reading all files in `tools/rabbiz-mirror/Sources/RabbizMirror/`:
1. **Window & chrome (`MainWindowController.swift`, `AppDelegate.swift`):**
   - Resizable dark-Aqua window (`400×900` initial, `380×560` min), `fullSizeContentView`, transparent titlebar, `NSVisualEffectView` top toolbar & bottom status bar.
   - Top bar: host text field (default `172.16.42.1`, strips `vnc://` or `rabbiz://` prefix if entered), Connect/Stop button (`⌘R`), Start-on-Device button (`⇧⌘R`, currently SSH `rabbiz-mirror start`), Fullscreen button (`⌘F`).
   - Bottom bar: status dot (gray/yellow/green/red), status label, live resolution + FPS (`720×1600 · 30 fps` or `idle`).
   - Menu bar: App menu, Edit menu, Connection menu (`Connect ⌘R`, `Start on Device ⇧⌘R`, `Disconnect ⌘.`, `Fullscreen ⌘F`), Window menu.
   - Auto-connect on launch (`0.6s` delay) + retry timer (`2.5s` interval, up to 10 attempts).
   - `--snapshot <path>` CLI flag for automated UI screenshot capture without screen-recording permission.
2. **Rendering & scaling (`MirrorView.swift`, `Framebuffer.swift`):**
   - Aspect-fit scaling centered on black background with `.high` interpolation.
   - Dynamic resize / rotation handling when phone output dimensions change.
3. **Input mapping (`MirrorView.swift`, `KeyMap.swift`):**
   - Pointer: left/right/middle mouse down/up/move/drag mapped from view coordinates to `0..framebufferSize`.
   - Scroll: both discrete and `hasPreciseScrollingDeltas` (accumulated at 12 pt threshold) vertical & horizontal scroll.
   - Keyboard: `keyDown`/`keyUp` + `flagsChanged` (Shift, Control, Option, Command/Super, CapsLock), special keys (Return, Tab, Backspace, Escape, Delete, Home/End, PageUp/Down, Arrows, F1–F13), Unicode characters; `⌘` shortcuts reserved for Mac menus.
4. **macOS Local Network permission (`LocalNetworkPermission.swift`):**
   - Triggers `Network.framework` (`NWConnection`) so macOS 15+ attributes traffic to the app and shows the Local Network permission prompt (using `Network.framework` natively for QUIC/TCP+TLS in the rewrite handles this cleanly).
5. **Headless self-test (`MirrorSelfTest.swift`):**
   - `--selftest --host <ip> --port <p> --out <png> --frames <n> --timeout <s>` mode used by `scripts/verify_mirror.sh`.

**Recommendation (A1):** Proceed with replacing `a21s-wayvnc` + RFB/ZRLE with Rabbiz Link (`rabbiz-link-server` + rewritten Swift Rabbiz Mirror), moving screenshots onto the shared in-compositor capture seam, and retiring `a21s-wayvnc` from default autostart.

---

## A2. Capture Path: In-Compositor (`de-capture`) vs. Screencopy Client

### Comparison

| Criterion | Privileged `wlr-screencopy` Client (Current Proposal) | In-Compositor Capture Seam (`de-capture`, Recommended) |
|---|---|---|
| **Copies per frame** | Compositor renders to DRM/GBM → `composite_output()` re-composites on CPU → copies to client `wl_shm` → IPC socket → encoder reads shm | Compositor renders once to GPU/GBM → async readback (or dmabuf) directly into pooled capture buffer handed to encoder thread (1 readback copy, 0 Wayland IPC copies) |
| **dmabuf / GPU client content** | **Broken today:** `composite_output()` skips dmabuf/EGL buffers (`CLEAR` colour only) | **Works:** captures the actual composited framebuffer including Panfrost/EGL/GTK/WebRender/Qt-GPU clients |
| **Compositor thread stall** | `screencopy_copy` / `screencopy_flush_pending` runs `composite_output()` synchronously on the compositor thread | Compositor only initiates async readback / hands off frame reference + damage rect and returns immediately |
| **Privilege surface** | Requires `Privileged::Screencopy` Wayland global + `/proc/<pid>/exe` allow-list (bypassable by same-user proxy) | Internal Rust seam inside `rabbiz-de`; no Wayland global needed for mirroring or built-in screenshots |
| **Lock rule enforcement** | Checked in `composite::capture_output()` (`de.nav.current == Screen::Lock`), only if called via `capture_output` | Enforced **once** inside `de-capture` at frame submission: while `locked() \|\| is_session_locked()`, subscribers receive only the lock screen (or blank frame per policy) — impossible for a subscriber to bypass |
| **Physical screen dimming** | Hard to exclude a compositor dim overlay if capturing post-composite | Trivial: `de-capture` taps the scene *before* the physical-only dim overlay is applied in `DisplayArea::Critical` |

**Recommendation (A2):** Use an **in-compositor capture seam (`de-capture`)** inside `rabbiz-de`, sharing it with screenshots and future screen recording. Keep `zwlr_screencopy_manager_v1` only if needed for legacy CLI tools (with `wayvnc` removed from the built-in allow-list once Rabbiz Link ships).

---

## A3. GPU Readback Cost on Mali-G52 / Panfrost (L2 — PENDING)

> **Status:** **BLOCKED / PENDING ON-DEVICE MEASUREMENT.**
> The A21s (`172.16.42.1`) was not reachable over USB or Wi-Fi at the time of this report (`ssh: connect to host 172.16.42.1 port 22: Operation timed out`; host Mac `ifconfig` shows no `172.16.42.x` interface).

### Planned Measurement Matrix (to run on A21s via `scripts/device_session.sh`)
1. **Full-output readback (`720×1600`, RGBA8 = 4,608,000 bytes/frame):**
   - Synchronous `glReadPixels(0, 0, 720, 1600, GL_RGBA, GL_UNSIGNED_BYTE, buf)` — median & p95 ms, plus compositor frame-time delta.
   - PBO-based async readback (`GL_PIXEL_PACK_BUFFER`, double-buffered PBOs: `glReadPixels` on frame $N$, `glMapBufferRange` on frame $N+1$) — median & p95 ms on compositor thread vs. worker thread.
   - `dmabuf` export + `mmap` (if Panfrost/exynos-drm permits linear/tiled CPU mapping and cache sync via `DMA_BUF_IOCTL_SYNC`).
2. **Partial readback (damage rectangles only):**
   - Small rect (e.g. `64×64` clock/cursor), medium rect (e.g. `720×200` status/banner), full screen (`720×1600`).
3. **Pipelining conclusion:** Confirm whether frame $N$ `glReadPixels` → PBO, consumed at frame $N+1$ (1-frame pipeline) keeps compositor `cpu_ms` within the 16.6 ms vblank budget.

---

## A4. Encoders (Code Audit Complete; L2 Benchmarks PENDING)

### A4.1 Hardware Encoder (Exynos 850 MFC)
- **Local codebase evidence:**
  - `devices/samsung/a21s/kernel/a21s_linux_defconfig` sets `CONFIG_VIDEO_SAMSUNG_S5P_MFC=m`.
  - `socs/exynos850/drivers/vendor-include/exynos_smc.h` references `SMC_DRM_PPMP_MFCFW_PROT` / `PROT_MFC` (TrustZone firmware protection constants from vendor headers).
  - No MFC DTS node or patch exists in `devices/samsung/a21s/kernel/patches/0001..0012` (confirmed by searching `devices/samsung/a21s/kernel/patches/`).
- **On-device verification needed (L2):** Check `/dev/video*`, `/sys/bus/platform/devices/*mfc*`, and `v4l2-ctl --list-devices` on the running A21s kernel (`7.3.0`) to confirm whether MFC is probed or absent.
- **Preliminary classification:** **Unknown / likely requires `kernel` lane DTS + clock/power-domain + firmware bring-up** (cannot be assumed for v1). Per Decision #4, a software encoder is acceptable on A21s with a measured frame-rate cap.

### A4.2 Software Video & Tile Encoders (L2 Benchmarks PENDING)
To be benchmarked on the A21s A55 cores (`nproc` = 8× Cortex-A55):
1. **Software video:** `x264` (`--preset ultrafast --tune zerolatency`) and `openh264` at `720×1600` and `360×800` (0.5× scale): fps, CPU %, and TMU temperatures after 10 minutes.
2. **Tile path:** `64×64` tiles from damage rects, compressed with `zstd` (levels 1–3), `lz4`, and `QOI`, measuring CPU ms/changed-MP and wire bytes/s across 3 scripted scenarios:
   - Scenario 1: Idle home screen with clock tick
   - Scenario 2: Scrolling a list (e.g. App Drawer / Settings)
   - Scenario 3: Playing a video / continuous full-screen animation

---

## A5. Transport Over the Real Link (Design Ready; L2 Link Tests PENDING)

- **Protocol choice (Decision #3):** **QUIC primary** (`quinn` + `rustls` on device; `Network.framework` `NWProtocolQUIC` on macOS), with **TCP + TLS 1.3 fallback** using identical length-prefixed framing.
- **Proposed stream layout:**
  - **Stream 0 (bidirectional, reliable):** `Control` — version negotiation, capabilities, pairing, keep-alive, rotation/size changes, quality/thermal hints.
  - **Stream 1 (unidirectional server→client, reliable or per-frame/keyframe):** `Video` — frame headers (seq, timestamp, damage rects, codec id) + tile/video payloads. (Using QUIC streams per GOP or frame-group avoids head-of-line blocking across dropped frames; alternatively, reliable stream with backpressure drop-before-send in the server queue.)
  - **Stream 2 (unidirectional client→server, reliable):** `Input` — multi-touch slots, pointer, key, scroll, hardware buttons. (Reliable ordered stream so touch-up/key-up events are never lost.)
  - **Optional Datagrams:** Only for droppable telemetry/RTT probes if needed; input and control must be reliable.
- **On-device L2 tests needed:** Verify UDP/QUIC and TCP+TLS throughput, RTT, and packet loss over `usb0` (`172.16.42.1`) and `wlan0` to the Mac, plus throttled-link (`tc netem`) congestion behavior.

---

## A6. Security Design

### A6.1 Threat Model (One-Page Summary)

| Threat | Attacker Capability | Impact Without Control | Control | Verification Test |
|---|---|---|---|---|
| **T1: Same-user malicious app on phone** | Runs as `rabbiz` (uid 1001); can connect to `127.0.0.1`, read world-readable files in `~/.config`, or spawn allowed binaries | Connects to mirror port (today's `wayvnc` loopback hole), steals pinned keys, or injects input via `ctl_ipc` | 1. **No loopback listener**; bind only user-enabled interfaces (`usb0` by default).<br>2. **Server off by default**; enabling requires unlocked device + explicit user action.<br>3. **Mutual TLS/QUIC key pinning**; client certs/keys stored with `0600` (and ideally in a root-owned or `rabbiz-link` privileged helper / hardware-backed or restricted dir so arbitrary user apps cannot silently add a trusted peer without on-screen confirmation).<br>4. **Phone-side confirmation UI** in `de-ui` for any new peer key. | Unit + L2 test: `ss -ltnp`/`ss -lunp` shows no `127.0.0.1` listener; unpinned peer rejected; new peer triggers on-screen prompt. |
| **T2: Attacker on same Wi-Fi** | Can send packets to phone's `wlan0` IP, sniff/MITM unencrypted traffic | Views screen or injects input if Wi-Fi mirroring is enabled | 1. **Wi-Fi listener disabled by default** (`usb0` only).<br>2. **TLS 1.3 / QUIC** (`rustls`) with mutual auth.<br>3. **Short-code SAS verification** on both screens during pairing prevents MITM. | L2 test: Wi-Fi interface not listening by default; unauthenticated TLS/QUIC handshake fails closed. |
| **T3: Stolen locked phone plugged into attacker's Mac** | Physical USB access (`usb0` `172.16.42.1`) while phone is locked | Views home/apps or unlocks phone via remote input | 1. **Cannot enable server or pair while locked**.<br>2. **Lock rule in `de-capture`:** while locked, capture yields only lock screen or blank frame.<br>3. **Input while locked:** remote input can *only* reach the lock screen's normal PIN/passphrase flow; no debug/mock/trusted-peer unlock shortcut. | L0/L2 test: capture while locked never contains home/app pixels; remote input while locked cannot launch apps or bypass Keyguard. |
| **T4: Malicious or compromised "paired" computer** | Holds a previously pinned key; sends malformed/oversized frames or floods input | Crashes compositor (`rabbiz-de`), OOMs device, or persists after user revokes trust | 1. **Hostile peer parser:** `#![forbid(unsafe_code)]` in `rabbiz-link-proto`, hard size limits, length-prefixed framing, fuzz-tested decoders.<br>2. **Out-of-compositor server:** networking/crypto/encode run off the compositor thread.<br>3. **Revocation in Settings:** immediately drops active session and removes pinned key.<br>4. **Persistent mirror indicator** on phone screen. | `cargo-fuzz` + unit tests for oversized/truncated/unknown messages; L2 revocation test drops active connection. |

### A6.2 Pairing, Key Storage & Session Indicator
- **Pairing flow:**
  1. Device must be **unlocked** and mirroring enabled by user.
  2. Mac connects over QUIC (TLS 1.3) presenting its Ed25519/ECDSA client certificate/public key; phone presents its device certificate/public key.
  3. Both sides derive a 6-digit Short Authentication String (SAS) from the TLS exporter / handshake transcript (`HKDF` over server pubkey + client pubkey + exporter secret) and display it on both screens.
  4. User taps **Approve** on the phone's `de-ui` modal; phone pins the Mac's key fingerprint, Mac pins the phone's key fingerprint.
  5. Subsequent connections from a pinned key succeed without prompt **only while the device is unlocked** (or show lock screen/blank per policy if locked).
- **Key storage:**
  - Stored under `$XDG_CONFIG_HOME/rabbiz-link/trusted_peers.toml` (or `/var/lib/rabbiz-link/` if separated) with `0700` dir and `0600` file permissions, atomic write-and-rename. Note: because third-party apps currently run as `rabbiz`, **enabling** mirroring and **approving** a new peer MUST require interactive confirmation in `rabbiz-de`'s compositor-owned `de-ui` (never auto-approved by a background file edit without user tap).
- **Revocation & Indicator:**
  - Settings → Mirroring lists pinned peers by name + fingerprint with a **Revoke** button (terminates active session immediately and deletes key).
  - Persistent status-bar/overlay pill ("Mirroring to \<Mac\>") + optional physical screen dim overlay (excluded from `de-capture`).

### A6.3 Input While Locked
- Remote input while locked is routed through the exact same `De` input path as physical touch/keyboard, **which enforces `Keyguard` isolation**:
  - Only `DisplayArea::Keyguard` (PIN keypad / passphrase bouncer) receives events.
  - `ctl_ipc` `Inject*` commands are removed/restricted to `--backend=virtual` only.
  - No "trusted peer auto-unlock" exists.

---

## A7. Thermal and Battery (Existing Facts + Adaptive Controller Plan)

### Existing Verified A21s Facts
- **Thermal zones** (`devices/samsung/a21s/findings/2026-11-18-flash-thermal-fg-dsi-live.md`): `little0-thermal`, `little1-thermal`, `gpu-thermal` are live via `exynos850-tmu.ko`, with **80 °C passive CPU trips** (`cpufreq-cpu0`/`cpufreq-cpu4`) and **95 °C critical trips**. Under 8-core `yes` load, temps plateaued at 76–77 °C.
- **Panfrost GPU load** (`devices/samsung/a21s/findings/2026-11-18-panfrost-thermal-run.md`): 90 s blur probe peaked at `51.1 / 51.6 / 47.3 °C` (LITTLE0 / LITTLE1 / G3D).
- **Battery telemetry** (`devices/samsung/a21s/findings/2026-11-18-fuel-gauge-s2mu106-monout-bogus.md`): Use `s2mu106-fuel-gauge` `voltage_now`, `current_now`, `capacity` only (never `*_avg`).
- **Governor:** `performance` (`devices/samsung/a21s/AGENTS.md`).

### Adaptive Controller Design (Thresholds to Refine with L2 Data)
- **Inputs:** Transport RTT & throughput, send queue depth, encoder frame time (ms), `little0/little1/gpu-thermal` max temp (°C), battery `capacity` & `current_now`, user mode (`Quality` / `Balanced (Auto)` / `Battery`).
- **Initial threshold targets (hypotheses, to be calibrated by A4/A7 L2 runs):**
  - `temp < 65 °C`: full resolution (`720×1600`), up to 30–60 fps (per A3/A4 cap), tile + video auto-switch.
  - `65 °C ≤ temp < 75 °C` or `battery < 20%` (discharging): cap FPS to 20–30 fps, prefer 0.75× or 0.5× scale for video regions.
  - `temp ≥ 75 °C` (5 °C below passive trip) or `battery < 10%`: cap FPS to 15 fps, force 0.5× scale, pause video encoder in favor of low-rate tiles.

---

## A8. Decision Record (Preliminary — Pending L2 Measurements)

| Choice | Options Considered | Evidence / Rationale | Preliminary Decision |
|---|---|---|---|
| **Protocol & Crate Name** | Rabbiz Link vs. extending VNC/RFB | Owner decision #1; VNC lacks thermal/battery/input/pairing semantics | **Rabbiz Link** (`rabbiz-link-proto`, `rabbiz-link-server`, `rabbiz-link-client`); Mac app stays **Rabbiz Mirror** |
| **v1 Scope** | Video+Input+Control vs. +Clipboard/Audio | Owner decision #2 | **Video + Input + Control** in v1; Clipboard & Audio in v2 |
| **Capture Seam** | `wlr-screencopy` client vs. in-compositor `de-capture` | Code audit (A2): `screencopy` skips dmabuf/EGL, runs on compositor thread, and cannot exclude physical dim overlay | **In-compositor `de-capture` seam** (shared with screenshots), pending A3 L2 readback numbers |
| **GPU Readback** | Sync `glReadPixels` vs. PBO async vs. dmabuf mmap | Pending A3 L2 measurement on Mali-G52/Panfrost | **Pending A3 L2 measurement** (expect PBO async $N \to N+1$) |
| **Encoder (v1)** | Hardware MFC vs. x264/openh264 vs. 64×64 Tiles (zstd/lz4/QOI) | No MFC DTS node in `a21s` patches; Decision #4 allows software encoder | **Tiles + software video fallback**, exact codec/cap **pending A4 L2 measurement** |
| **Transport** | QUIC (`quinn` / `Network.framework`) vs. TCP+TLS 1.3 | Owner decision #3 | **QUIC primary**, **TCP+TLS 1.3 fallback** with identical framing; **pending A5 L2 link test** |
| **Mac Client** | Bindings to Rust crate vs. native Swift | Owner decision #5 | **Native Swift rewrite from spec** (`tools/rabbiz-mirror/`); delete RFB/ZRLE/Inflater at parity |
| **Legacy `wayvnc`** | Keep running vs. disable by default | Unauthenticated localhost exposure (`2026-11-18-de-wayvnc-unauthenticated-localhost.md`) + Decision #5 | **Disabled by default**; never auto-started |
