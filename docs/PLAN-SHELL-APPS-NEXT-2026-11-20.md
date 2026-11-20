# Deep Plan: RabbizOS Shell (`rabbiz-de`, `rabbiz-osk`) & Native Apps (`rabbiz-apps`)

- **Date:** 2026-11-20
- **Lane:** `docs` (implementation spans `de` across `tools/rabbiz-de`, `tools/rabbiz-osk`, `tools/rabbiz-apps`)
- **Baseline commit:** `9cf05c3` (after Tier 0–1 refactors, `ctl` IPC v1, 4 Tier-2 RFCs, and Tier-2 Phase 1–3 foundations)
- **Evidence target:** Everything in Waves 4–6 is designed to be **L0 host-tested on macOS** and **L1 built/tested in `rabbiz-de-build:arm64`**, followed by targeted **L2 on-device verification** when the A21s returns.

---

## 1. Where We Stand Today (Measured Inventory)

| Workspace | Scale | Current Strength | Biggest Remaining Leverage |
|---|---|---|---|
| **`tools/rabbiz-de`** (14 crates) | ~36 kLOC, 265 macOS + 47 arm64 tests, 0 warnings | Clean one-way seams, `DisplayStack` 12-band Z-order, `ctl` IPC v1 (7 cmds + lock guard), Phase 1–3 foundations (`RegionCache`, `HunFx`, `LockoutPolicy`, `LifecycleManager`, `MprisTracker`, `SessionLockTracker`, `upload_pixels_damaged`) | Foundations are built & tested, but **interactive wiring** (HUN touch gestures + live notification enqueue, Bouncer unlock flow, Shade `MediaCard` + `NotificationGroup`, split `StatusBar` partial upload, and `De` struct decomposition) is not yet live end-to-end. |
| **`tools/rabbiz-apps`** (13 crates: `rabbiz-app-kit` + 12 apps) | ~34 kLOC, 308 host tests, 0 warnings | Zero >1,300 LOC monoliths left; 100% macOS host-testable; 0 `unsafe` warnings; built-in CLI + MCP headless layer across apps | `rabbiz-app-kit` is thin (~225 LOC `ui.rs`). Every app duplicates **scroll/fling physics, Thai-safe `TextBuf`, common MD3 widgets, theme hot-reload, and cross-app intent handoff**. 3 medium files (`notes`, `sms`, `photos` `main.rs`, 878–1,068 LOC) can also be slimmed once shared kit absorbs boilerplate. |
| **`tools/rabbiz-osk`** (3 crates) | Modularized `osk-wayland` (9 modules), 45 host tests | Clean core/render/wayland split, Thai + English layouts, clipboard strip | Needs `content_purpose::Password` privacy mode (for Keyguard bouncer), swipe-spacebar cursor/language ergonomics, and shared `de-config` theme/haptic reactivity. |

---

## 2. Five High-Impact Architectural Ideas

### Idea A — Promote `rabbiz-app-kit` from Thin Drawing Helpers to a True Mobile UI Kit
Today each of the 12 native Rust apps re-implements basic mobile UX differently (different scroll friction, different Thai backspace handling, different header/tab/sheet geometry).

**Upgrade `rabbiz-app-kit` into 6 focused, 100% host-testable modules:**
1. **`scroll.rs` — Unified Mobile Scroll & Fling Physics:**
   - Single `ScrollView` with consistent touch-slop (`12 px`), velocity window (`dt >= 12 ms`), exponential deceleration, rubber-band overscroll (`0.35×` resistance), and spring snap-back — identical feel across Files, Contacts, Music, SMS, Notes, Settings, Photos.
2. **`text_input.rs` — Thai-Safe `TextBuffer` + Selection + IME Preedit:**
   - Extract the battle-tested Thai grapheme-cluster deletion logic from `rabbiz-contacts` into `rabbiz-app-kit::text_input`, add cursor movement, preedit underline rendering, and `content_purpose` hints (`Normal`, `Number`, `Phone`, `Password`, `Search`).
3. **`widgets.rs` — Standard MD3 Mobile Chrome:**
   - `TopAppBar` (title, back button, trailing actions), `BottomTabBar`, `SearchPill`, `Fab`, `ListRow`, `ToggleSwitch`, `Chip`, `Toast`, `EmptyState`, and `BottomSheet` modal container.
   - Guarantees consistent `48×48 dp` minimum touch targets and safe-area (`TOP_SAFE` / `BOTTOM_SAFE` / `ime_bottom`) compliance automatically.
4. **`icons.rs` — Shared Material Symbol Codepoints:**
   - Eliminate duplicate `ICON_*` constants scattered across `music`, `dialer`, `files`, `contacts`, `clock`.
5. **`intent.rs` — Typed Cross-App Intent & URI Router (`rabbiz-intent`):**
   - Standardize how apps hand off tasks to each other (and how Shell launches them):
     - `tel:<number>` → `rabbiz-dialer --dial <number>`
     - `sms:<number>[?body=...]` → `rabbiz-sms --compose <number>`
     - `file://<path>` (MIME dispatch: `.pdf` → `rabbiz-pdf`, `image/*` → `rabbiz-photos`, `audio/*` → `rabbiz-music`, `text/*` → `rabbiz-notes`)
     - `rabbiz://settings/<section>` → `rabbiz-settings --page <section>`
   - Pure URL/MIME parser + resolver in `rabbiz-app-kit` (L0 host-tested), backed by `de-apps` / `rabbiz-de ctl launch` on Linux.
6. **`theme_watch.rs` — Live Theme & Inset Reactivity:**
   - Apps re-read `de-config::Prefs` on `SIGUSR1` or lightweight mtime check on resume, so toggling Dark/Light mode in Shade updates every open native app immediately without restarting.

### Idea B — Complete Interactive End-to-End Shell Flows (Turn Phase 1–3 Foundations into Live UX)
We already built the pure state machines and renderers. Now connect the last mile inside `de-core` and `de-ui`:

1. **Live HUN Pipeline:**
   - `de-system::notifications` pushes new entries → `De` calls `hun_fx.enqueue(...)` with `HunContext { dnd, shade_open, locked }`.
   - `runtime/input/touch.rs` intercepts touches inside `hun_fx.banner_rect(output)`:
     - Touch down → `hun_fx.set_touch_held(true)` (pauses 5 s timer)
     - Swipe up → `hun_fx.swipe_up()` (snooze banner, keep in shade)
     - Swipe left/right → `hun_fx.swipe_horizontal()` (dismiss + `NotificationClosed(id, 2)`)
     - Tap action pill → emit `ActionInvoked(id, key)` + dismiss HUN
2. **Live Shade Upgrade (`MediaCard` + `NotificationGroup` + `LockscreenPrivacy`):**
   - Wire `de-ui::screens::media_card` into `panels.rs` (above notifications when `MprisTracker::active()` is `Some`).
   - Wire `de-ui::screens::notification_groups::group_notifications` into Shade (expandable per-app stacks) and `redact_for_lockscreen` into `lock.rs` using `Prefs::lockscreen_notifications`.
3. **Live Keyguard Bouncer Flow:**
   - When `Prefs::lock_method != "swipe"` (or `session_lock_tracker.needs_recovery_bouncer()`), swiping up on `lock.rs` transitions to `lock_bouncer.rs` instead of unlocking.
   - Connect keypad hits to `LockoutPolicy` + hashed PIN verifier (`de-system/src/auth.rs`, Argon2/SHA-256 salted verifier in helper thread) + Emergency dialer button.
4. **Live Retained-UI Fast Path for `StatusBar` & `Osd`:**
   - Use `RegionCache` + `Canvas::paint_clipped` + `upload_pixels_damaged` on `StatusBar` first (clock tick repaints only the ~90×48 px clock box instead of a 720×1600 full-screen upload — >98% bandwidth reduction on idle ticks).

### Idea C — Structural Cleanliness in `de-core` (Break `De` God-Struct + Unified Hit-Testing)
- **`runtime/state.rs` (750 LOC, ~70 flat fields):** Group into 5 cohesive sub-structs on `De`:
  - `display: DisplayState` (output, insets, scale, wallpaper)
  - `shell_overlays: OverlayState` (`osd_fx`, `hun_fx`, `shot_fx`, `panel`)
  - `security: SecurityState` (`lock`, `session_lock_tracker`, `lockout`, `bouncer`)
  - `apps: AppRuntimeState` (`lifecycle`, `morph`, `windows`, `recents`)
  - `input_state: InputState` (gestures, drag, touch targets)
- **Top-Down Input Routing via `DisplayStack`:** Mirror `frame/render.rs::render_with` so hit-testing walks `Critical → PowerMenu → Keyguard → Overlay (HUN/OSD) → Shade → Drawer → Recents → App → Home` in one declarative order — making "ghost clicks hitting a hidden layer" structurally impossible.

### Idea D — Shell-Level Universal Search & Command Palette (RabbizOS Differentiator)
Every `rabbiz-*` app already ships a **headless CLI + MCP tool layer** (`contacts`, `files`, `notes`, `clock`, `calculator`, `dialer`, `music`, `sms`, `pdf`, `terminal`). Almost no mobile OS has this natively.

- Upgrade `de-ui/src/screens/search.rs` into a **Universal Action Bar** (swipe-down on Home or tap Search pill):
  - **Instant Calculator:** typing `1250 * 1.07` evaluates inline via pure expression evaluator (no need to open Calculator).
  - **Instant Contacts & Quick Actions:** typing `sarah` shows `Sarah Chen` with one-tap `Call` (`tel:`) and `SMS` (`sms:`) pills.
  - **Instant Notes / Files / Settings Jumps:** typing `wifi`, `dark`, `alarm 07:00` jumps directly to the matching setting or triggers the app action via `rabbiz-intent`.
  - Pure query-router + ranking engine in `de-ui/src/interaction/universal_search.rs` (100% L0 host-testable).

### Idea E — OSK (`rabbiz-osk`) Security & Ergonomics Polish
- **Password / Secret Mode:** When `zwp_text_input_v3` signals `content_purpose::Password` or `Pin` (e.g. Keyguard bouncer or login fields), disable key-popup persistence, prediction/learning, and clipboard auto-capture.
- **Spacebar Cursor Scrubbing & Quick Language Toggle:** Dragging left/right on the spacebar moves the text cursor smoothly (like iOS/Gboard); tapping globe or swiping spacebar toggles EN ↔ TH cleanly.
- **Consistent Theme Sync:** Read `de-config::Prefs` directly (or shared theme tokens) so OSK matches Light/Dark shell switches immediately.

---

## 3. Execution Roadmap (Host-First Waves)

Every wave below is structured for **parallel disjoint subagents**, **0 warnings**, **L0 macOS + L1 arm64 Docker verification**, and clean per-lane commits.

### Wave 4 — `rabbiz-app-kit` Shared Mobile Kit (High Reuse, 100% L0 Host-Testable)
- [ ] `rabbiz-app-kit/src/scroll.rs` — unified `ScrollView` (touch slop, velocity fling, rubber-band overscroll, snap-back) + unit tests
- [ ] `rabbiz-app-kit/src/text_input.rs` — Thai grapheme-safe `TextBuffer` + cursor + `ContentPurpose` + unit tests
- [ ] `rabbiz-app-kit/src/widgets.rs` + `icons.rs` — MD3 `TopAppBar`, `BottomTabBar`, `SearchPill`, `ListRow`, `ToggleSwitch`, `Toast`, `EmptyState`, `BottomSheet`, shared Material icon constants + hit-test unit tests
- [ ] `rabbiz-app-kit/src/intent.rs` — `Intent` parser/resolver (`tel:`, `sms:`, `file://`, `rabbiz://settings/...`) + unit tests
- [ ] Migrate 1–2 apps (`rabbiz-notes`, `rabbiz-sms`) as reference adopters & slim their `main.rs` (< 500 LOC)

### Wave 5 — Shell Live Interactive Wiring (`de-ui` + `de-core`)
- [ ] Wire `de-system::notifications` → `De::hun_fx.enqueue(...)` + touch gesture interception in `runtime/input/touch.rs` (hold-pause, swipe-up snooze, swipe-horizontal dismiss, action tap)
- [ ] Wire `MediaCard` + `NotificationGroup` into `de-ui/src/screens/panels.rs` and `redact_for_lockscreen` into `de-ui/src/screens/lock.rs`
- [ ] Wire `lock_bouncer.rs` + `LockoutPolicy` + `Prefs::lock_method` into `shell_ui/lock_ui.rs` (with salted PIN hash verification in `de-system/src/auth.rs`)
- [ ] Wire `RegionCache` + `Canvas::paint_clipped` + `upload_pixels_damaged` onto `StatusBar` rendering path in `de-core`

### Wave 6 — Universal Search, `De` Refactor & OSK Polish
- [ ] `de-ui/src/interaction/universal_search.rs` — inline calculator, settings shortcuts, contact/app ranking + `search.rs` UI
- [ ] Decompose `De` in `de-core/src/runtime/state.rs` into cohesive sub-structs (`DisplayState`, `OverlayState`, `SecurityState`, `AppRuntimeState`)
- [ ] Top-down `DisplayStack` input dispatcher in `de-core/src/runtime/input/`
- [ ] `rabbiz-osk`: `Password`/`Pin` privacy mode + spacebar cursor scrubbing in `osk-core` (L0 host-tested)

### Wave 7 — On-Device L2 Verification & Benchmarks (When A21s Returns)
- [ ] Verify `8be1ca6` swipe-up-and-hold Recents fix on touch hardware
- [ ] Build & deploy `rabbiz-de`, `rabbiz-osk`, and updated `rabbiz-apps`
- [ ] Measure `StatusBar` partial upload CPU/bandwidth delta on Exynos 850 and record finding under `docs/findings/`
- [ ] Verify HUN gestures, Shade media card, PIN bouncer lockout, and `rabbiz-de ctl` end-to-end on A21s

---

## 4. Risks & Guardrails

1. **Frozen `de-api` v1.1:** None of Waves 4–6 require breaking changes to `de-api`.
2. **One-way crate seams:** Every change in `tools/rabbiz-de` must pass `python3 tools/rabbiz-de/scripts/check_seams.py` (`14 crates, one-way`).
3. **Never restyle third-party apps:** `rabbiz-app-kit` upgrades apply strictly to our own `rabbiz-*` native apps; third-party Linux/KDE/Chrome apps keep their upstream branding and icons.
4. **Honest evidence levels:** Waves 4–6 are **L0/L1** until verified on the A21s in Wave 7.
