# DE Lane Structural & Architecture Audit Handoff (2026-11-19)

Lane `de`, scope `tools/rabbiz-de` (14 Cargo workspace crates, ~29.4k LOC of Rust).  
Evidence level: **L0 (static audit + seam gate)**. Read-only audit; no code changes applied.  
Verified command: `python3 tools/rabbiz-de/scripts/check_seams.py` → `seams ok: 14 crates, one-way`.  
**Not tested on device.**

---

## 1. Current Baseline (What Works Well)

- **Clean crate-level layering**: `de-api` has zero internal dependencies (`std` + `serde` only) and separates host-testable crates (`de-api`, `de-design`, `de-paint`, `de-ui`, `de-config`, `de-apps`, `de-system`, `de-sim`) from Linux/Smithay crates (`de-backend`, `de-render`, `de-protocols`, `de-core`, `de-bin`).
- **Automated seam enforcement**: [check_seams.py](../tools/rabbiz-de/scripts/check_seams.py) enforces one-way dependencies and checks for cycles via `cargo metadata`.
- **Workspace lints**: [Cargo.toml](../tools/rabbiz-de/Cargo.toml) sets `unsafe_code = "warn"` and `clippy::pedantic = "warn"` across `crates/*`.
- **Host simulator & visual regression**: `de-sim` + `tests/golden/` (60+ PNGs across 720×1600 and 1080×2400 in `en`/`th`).
- **Build hygiene**: `target/` and `target-linux-arm64/` are ignored in [.gitignore](../.gitignore).

---

## 2. Project Structure Audit (`tools/rabbiz-de`)

### P1 — `de-core` is becoming a flat monolith
[de-core/src/lib.rs](../tools/rabbiz-de/crates/de-core/src/lib.rs) exposes **28 flat `.rs` files** with no subdirectory boundaries:

- **God struct**: `De` in [state.rs](../tools/rabbiz-de/crates/de-core/src/runtime/state.rs) holds ~70 fields spanning DRM/renderer, Wayland protocol globals, seat/pointer/keyboard tracking, launcher/icon/wallpaper caches, OSK lifecycle, screencopy, power state, lock overlay, and screenshot FX.
- **Oversized files**:
  - [frame.rs](../tools/rabbiz-de/crates/de-core/src/runtime/frame.rs) (**1,792 LOC**) mixes the event loop, render-element assembly, GPU blur preparation, texture upload/sync, cursor rasterization (`cursor_pixels`/`fill_poly`), shell IM polling, and debug frame dumping.
  - [input.rs](../tools/rabbiz-de/crates/de-core/src/runtime/input.rs) (**1,531 LOC**) is a single `impl De` block handling all touch, pointer, and keyboard routing.
  - [panel_ui.rs](../tools/rabbiz-de/crates/de-core/src/shell_ui/panel_ui.rs) (714 LOC), [state.rs](../tools/rabbiz-de/crates/de-core/src/runtime/state.rs) (646 LOC), [power.rs](../tools/rabbiz-de/crates/de-core/src/runtime/power.rs) (638 LOC), [morph.rs](../tools/rabbiz-de/crates/de-core/src/shell_ui/morph.rs) (635 LOC).
- **Mixed concerns**: Wayland protocol handlers (`comp.rs`, `xdg.rs`, `layer_shell.rs`, `seat.rs`, `selection.rs`, `screencopy.rs`, `virtual_pointer.rs`, `popups.rs`, `misc.rs`), compositor runtime (`frame.rs`, `composite.rs`, `input.rs`, `power.rs`, `osk.rs`), and shell UI controllers (`home_ui.rs`, `panel_ui.rs`, `lock_ui.rs`, `shot_ui.rs`, `shell.rs`, `shell_menu.rs`, `morph.rs`, `navigation.rs`) sit in one flat directory.
- **Anti-home filename**: `misc.rs` violated the repo naming rule (*no `misc` — name the concept*); renamed to [output_dmabuf.rs](../tools/rabbiz-de/crates/de-core/src/protocols/output_dmabuf.rs) (holds output/viewporter/fractional-scale/dmabuf protocol handlers).

### P1 — Crate boundary drift (`de-apps` & `de-backend`)
- **System prefs inside `de-apps`**: `AppPrefs` in [de-apps/src/prefs.rs](../tools/rabbiz-de/crates/de-apps/src/prefs.rs) owns `set_wallpaper`, `set_do_not_disturb`, `set_keyboard`, and `set_allow_third_party_keyboards`. These are OS/shell preferences, not app-catalog state, and belong in `de-config`.
- **`de-apps` dependency creep**: [de-apps/Cargo.toml](../tools/rabbiz-de/crates/de-apps/Cargo.toml) pulls in `de-design`, `de-paint`, and `de-system` (5 internal crates), turning it into a mid-stack catch-all.
- **`de-backend` → `de-render` coupling**: [de-backend/Cargo.toml](../tools/rabbiz-de/crates/de-backend/Cargo.toml) depends on `de-render` (for `probe.rs`), pulling `de-paint` transitively into the hardware backend crate.

### P2 — Seam doc drift, unused dependencies & manifest gaps
- **5 unused direct deps in `de-bin`**: [de-bin/Cargo.toml](../tools/rabbiz-de/crates/de-bin/Cargo.toml) lists `de-api`, `de-render`, `de-ui`, `de-paint`, and `serde_json`, none of which are imported in [de-bin/src/main.rs](../tools/rabbiz-de/crates/de-bin/src/main.rs).
- **Stale [DE-SEAMS.md](DE-SEAMS.md)**: Dependency tree diverges from [check_seams.py](../tools/rabbiz-de/scripts/check_seams.py) and actual `Cargo.toml` manifests (`de-apps`, `de-backend`, `de-smoke`).
- **Missing lints on `de-smoke`**: [tests/smoke/Cargo.toml](../tools/rabbiz-de/tests/smoke/Cargo.toml) omits `[lints] workspace = true` and `rust-version.workspace = true`.
- **`check_seams.py` blind spot**: Only inspects normal dependencies (`kind is None`), skipping `dev-dependencies` and `build-dependencies`.

### P2 — `de-ui` flat layout & legacy skeleton overlap
- **Flat 25-file tree**: [de-ui/src/lib.rs](../tools/rabbiz-de/crates/de-ui/src/lib.rs) mixes interaction primitives (`gestures.rs` 896 LOC, `nav.rs`, `pager.rs`, `pull.rs`, `transitions.rs`), shared chrome (`chrome.rs` 634 LOC, `menu.rs` 532 LOC, `nav_bar.rs`), and screens (`home.rs`, `panels.rs`, `drawer.rs`, `recents.rs`, `lock.rs`, `settings/*`).
- **Legacy skeleton overlap**: [qs.rs](../tools/rabbiz-de/crates/de-ui/src/screens/qs.rs), [status.rs](../tools/rabbiz-de/crates/de-ui/src/screens/status.rs), and [first_run.rs](../tools/rabbiz-de/crates/de-ui/src/screens/first_run.rs) are only called by `de-sim`; [shade.rs](../tools/rabbiz-de/crates/de-ui/src/screens/shade.rs) coexists with the real split-panel UI in [panels.rs](../tools/rabbiz-de/crates/de-ui/src/screens/panels.rs) (still referenced by `Screen::Shade` in [de-core/src/shell.rs](../tools/rabbiz-de/crates/de-core/src/shell_ui/shell.rs)).

### P3 — Host test gate & dual-shell placement
- **`de-system` tests skipped on macOS**: [scripts/de_test_host.sh](../scripts/de_test_host.sh) omits `-p de-system` from `HOST_CRATES`, even though `de-system` already builds on macOS as a dependency of `de-apps`.
- **Dual-shell coexistence**: Active Rust shell is in `tools/rabbiz-de/`, while legacy Quickshell remains at top-level `shell/` and [AGENTS.md](../AGENTS.md) still documents `shell/Rabbiz/` as the primary shell layout.

---

## 3. Gap Analysis vs. Modern Mobile Shells (Android SystemUI / WindowManager / SurfaceFlinger)

`rabbiz-de` currently behaves as a **Fullscreen Handset Compositor + Immediate-Mode CPU Shell**. To evolve into a modern mobile/desktop shell, six foundational architecture layers are still missing or early:

### Pillar 1: Scene Graph, Z-Order Layer Hierarchy & Window Insets (Critical)
- **Current**: Binary `Foreground::Shell | Foreground::App`, single output scale `1.0`, hardcoded draw order in `frame.rs`, all toplevels forced fullscreen.
- **Missing**:
  - Formal Z-ordered DisplayArea hierarchy (`Wallpaper → App Tasks → Modal/Dialog → Split/PiP → System Bars → IME → Heads-Up → Shade/QS → System Dialogs → Keyguard → Critical/Power`).
  - Window Insets / Safe Area contract (`status_bar`, `nav_bar`, `ime`, cutout/rounded corners) so apps resize/pan cleanly around OSK and bars.
  - Windowing policy engine (`xdg_toplevel::set_parent` modal handling, Task stack, Split-Screen, PiP, Floating/Desktop mode).
  - Display & Orientation Manager (`portrait ↔ landscape` auto-rotate, real fractional scaling, multi-output).

### Pillar 2: Retained UI Tree & Interruptible Transition Coordinator
- **Current**: `de-ui` renders full-screen software bitmaps (`fn render(...) -> Pixels`) on CPU; layout/draw live in `de-ui` while touch/hit-testing/state live in `de-core/*_ui.rs`; transitions are separate ad-hoc state machines.
- **Missing**:
  - Retained Widget/Layer tree with unified layout + hit-test + partial dirty-rect redraw.
  - Single interruptible, gesture-driven, retargetable Transition Coordinator (`ShellTransitions` equivalent) shared by app launch/close, predictive back, recents scrub, shade pull, and rotation.
  - Focus traversal engine (keyboard/gamepad) & Accessibility tree (AT-SPI).

### Pillar 3: Reactive Service Bus & Notification/Media Pipeline
- **Current**: Clean `de-api/src/services.rs` seam exists, but `de-core` still uses a 2 s polling cache (`meta_cache`) and `de-system` lacks several mobile controllers.
- **Missing**:
  - Event-driven D-Bus controllers for NetworkManager, BlueZ, ModemManager, PipeWire, UPower, MPRIS.
  - Full Notification pipeline: Heads-Up banners (HUN), grouping, inline actions/reply, ongoing pills (call/timer/recording/hotspot), lockscreen redaction.
  - Media/Audio routing (MPRIS card, per-stream volume, speaker/BT/headphone route switcher).
  - Idle/WakeLock policy (`ext-idle-notify-v1`, `zwp_idle_inhibit_manager_v1`), AOD/Doze, thermal/low-battery alerts.

### Pillar 4: Security, Real Keyguard & Privilege UI
- **Current**: Lock overlay + fingerprint watcher + client identity check (`privileged.rs`).
- **Missing**:
  - Real Keyguard authentication (PAM/PIN/Password/Pattern + lockout backoff) + `ext-session-lock-v1` + strict input/screencopy isolation when locked.
  - In-shell Polkit authentication agent & XDG Desktop Portal UI (camera/mic/screen-share permissions, FileChooser).
  - Full-screen intent over lockscreen (incoming call, alarm).

### Pillar 5: App Lifecycle, Memory (2.8 GB RAM) & Input/Gaming Ecosystem
- **Current**: `de-apps/src/launch.rs` spawns processes with no lifecycle/cgroup management; `xwayland` feature is enabled in Smithay `Cargo.toml` but not wired in `de-core`.
- **Missing**:
  - Activity/Process lifecycle & cgroup freezing / OOM priority coordination (protect compositor + foreground app on 2.8 GB RAM).
  - IME + Window Insets coordination (`text-input-v3` + `input-method-v2`), clipboard history UI, cross-app DnD.
  - XWayland lifecycle, `pointer-constraints-v1`, `relative-pointer-v1` (needed for Steam/games/desktop).

### Pillar 6: Shell Control IPC & Extensibility
- **Current**: Monolithic binary with CLI flags and ad-hoc file triggers.
- **Missing**:
  - Runtime Control IPC (D-Bus or Unix socket) for external Settings, `rabbiz-mirror`, hardware key daemons, and E2E test harnesses.
  - Pluggable registry for Quick Settings tiles and Home widgets.

---

## 4. Prioritized Backlog (Execution Order)

### Tier 0 — Low-Risk Hygiene (do first, host-verifiable)
1. **Manifest & Seam Hygiene** — **done (2026-11-19)**:
   - Removed 5 unused direct deps from `tools/rabbiz-de/crates/de-bin/Cargo.toml` and tightened `de-bin` in `check_seams.py`.
   - Added `[lints] workspace = true` and `rust-version.workspace = true` to `tools/rabbiz-de/tests/smoke/Cargo.toml`.
   - Added `-p de-system` to `HOST_CRATES` in `scripts/de_test_host.sh`.
   - Updated `docs/DE-SEAMS.md` to match `check_seams.py` and extended `check_seams.py` to check `dev-dependencies` and `build-dependencies`.
   - **Evidence**: L0 (`check_seams.py` → 14 crates one-way; `cargo test` across all 8 host crates including `de-system` passed; `check_links.mjs` → 107 md files ok). Note: `de-sim --check` golden diffs in this working tree are from pre-existing uncommitted `de-ui` edits.
2. **Fix Ownership Leaks** — **prefs delegation done in working tree (2026-11-19)**:
   - Moved transactional OS/shell preference mutators (`commit_to`, `set_wallpaper`, `set_do_not_disturb`, `set_keyboard`, `set_allow_third_party_keyboards`) onto `de_config::Prefs`; `de-apps::prefs::AppPrefs` now delegates to `Prefs`.
   - Remaining: decide & clean up legacy `de-ui::{shade, qs}` vs `de-ui::panels`.

### Tier 1 — Structural Refactor & Foundation (do before adding major features)
3. **Sub-module `de-core`** — **directory split done in working tree (2026-11-19)**:
   - Organized `de-core/src/` into `protocols/` (10 files), `runtime/` (13 files), and `shell_ui/` (8 files) with crate-root re-exports in `lib.rs`.
   - Renamed anti-home `misc.rs` → `protocols/output_dmabuf.rs`.
   - Extracted pointer cursor sprite rasterization out of `frame.rs` into `runtime/cursor.rs` (and fixed cursor Z-order bug).
   - Added `runtime/display_stack.rs` (`RenderStack<E>` Smithay front-to-back adapter backed by `de_api::DisplayStack`).
   - Remaining: split `runtime/frame.rs` & `runtime/input.rs` further; break `De` god-struct into sub-structs.
   - **Evidence**: L1 (`rabbiz-de-build:arm64` `cargo test -p de-core --lib --locked` → 28 passed, 0 failed, 0 warnings).
4. **Sub-module `de-ui`** — **done in working tree (2026-11-19)**:
   - Organized `de-ui/src/` into `interaction/` (6 files), `chrome/` (`mod.rs`, `menu.rs`, `nav_bar.rs`), and `screens/` (17 modules incl. `settings/`) with crate-root re-exports in `lib.rs`.
   - **Evidence**: L0 (`cargo test -p de-ui -p de-sim` → 88 passed, 0 failed; `check_seams.py` ok; `check_links.mjs` → 111 md files ok).
5. **Blueprint & Implement Scene Graph + Layer Z-Order + Window Insets** — **Step 1 landed, Step 2 (`RenderStack`) & Step 4 (IME insets) wired in working tree (2026-11-19)**:
   - See [DE-SCENE-INSETS.md](DE-SCENE-INSETS.md) (approved by owner).
   - Landed on `main`: `de-api` v1.1 (`DisplayArea`, `DisplayStack<T>` with Keyguard isolation, `EdgeInsets`, `WindowInsets` + 6 unit tests).
   - Wired in working tree & **L1 verified** (`rabbiz-de-build:arm64`): `runtime/frame.rs::render_with` now buckets backdrop, `SystemBars`, `Overlay`, `Shade`, `Keyguard`, and `Critical` through `RenderStack` (enforcing Keyguard isolation when strictly locked); `De.insets` (`runtime/state.rs`) and `De::sync_layer_insets()` (`protocols/layer_shell.rs`) auto-reconfigure toplevels on exclusive-zone OSK changes.
   - Remaining: route `runtime/input.rs` hit-testing top-down through `DisplayStack`.
6. **Shell Control IPC** — **RFC + wire types + CLI client landed; `de-core` server wired in working tree (2026-11-19)**:
   - See [DE-IPC.md](DE-IPC.md) for the Unix-socket JSON-lines protocol.
   - Landed on `main`: `de-config::ipc` (wire types, 16 KiB guard, `send_request` Unix socket client + `UnixListener` round-trip test), `de-config::flags` (`ctl` subcommand), `de-bin` (`rabbiz-de ctl`).
   - Wired in working tree & **L1 verified** (`rabbiz-de-build:arm64`): `de-core/src/ctl_ipc.rs` (non-blocking `0600` Unix listener dispatching `GetState`, `Lock`, `Launch`, `ReloadTheme`, `SetPanel(close)`) polled in `frame.rs`. Kept uncommitted alongside the pre-existing `de-core` edits.

### Tier 2 — Mobile OS Readiness
7. Retained UI / partial dirty-rect damage in `de-paint`/`de-ui`.
8. Real Keyguard (PAM/PIN + `ext-session-lock-v1` + lock isolation) + `zwp_idle_inhibit_manager_v1` — **idle inhibit + `ext-session-lock-v1` landed (2026-11-19)**:
   - Implemented `protocols/idle_inhibit.rs` (`zwp_idle_inhibit_manager_v1`) wired into `runtime/power.rs` so live mapped surfaces (video/games) prevent screen dim/sleep and dead surfaces are auto-pruned.
   - Implemented `protocols/session_lock.rs` (`ext-session-lock-v1`, gated by default-deny privileged policy) coordinated with built-in Keyguard (`power_lock` / `lock.unlock_now`).
   - Remaining: PAM/PIN Keyguard auth UI.
9. Full Notification pipeline (HUN, actions, MPRIS media card) + reactive D-Bus services.
10. App lifecycle / systemd scope cgroups / OOM priority policy.

### Tier 3 — Modern Flagship Features
11. Unified interruptible Transition Coordinator (Predictive Back, gesture scrub).
12. Display rotation, fractional scale, multi-output, and Split/PiP/Desktop windowing policies.
13. XWayland + pointer constraints + Polkit/Portal UI + AT-SPI accessibility — **relative-pointer + pointer-constraints landed (2026-11-19)**:
    - Implemented `protocols/pointer_constraints.rs` (`zwp_relative_pointer_manager_v1` + `zwp_pointer_constraints_v1`) wired into `runtime/state.rs` (pairs with existing `ptr.relative_motion` in `runtime/input.rs`).
    - Remaining: XWayland lifecycle, Polkit/Portal UI, AT-SPI.
