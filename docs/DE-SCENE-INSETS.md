# RFC: `DisplayArea` Z-Order Stack & `WindowInsets` Contract (Tier 1.5)

- **Lane**: `de`
- **Status**: Approved (owner sign-off 2026-11-19; Step 1 landed in `de-api` v1.1)
- **Parent handoff**: [HANDOFF-de-audit-2026-11-19.md](HANDOFF-de-audit-2026-11-19.md) §3 Pillar 1 & §4 Tier 1.5

---

## 1. Problem

Today `de-core` models the screen as a binary `Foreground::Shell | Foreground::App` switch ([state.rs](../tools/rabbiz-de/crates/de-core/src/state.rs)) and assembles compositor layers through hardcoded call order in [frame.rs](../tools/rabbiz-de/crates/de-core/src/frame.rs) (`push_home_layers`, `push_window_elements`, `push_layer_elements`, `push_panel`, `push_lock`, `push_menu_over`, `push_shot`, `push_fade`). Every `xdg_toplevel` is forced fullscreen at the full output size (`720×1600`), with no awareness of system bars, cutouts, or the on-screen keyboard (OSK).

This blocks modern mobile shell capabilities:
- **OSK / IME overlap**: Apps cannot shrink or pan above the keyboard because no inset contract exists.
- **Transient/modal dialogs**: `xdg_toplevel::set_parent` dialogs get stretched fullscreen instead of floating in a modal layer.
- **Z-order fragility**: Adding heads-up notifications, Polkit/Portal prompts, incoming-call overlays over lockscreen, or split-screen/PiP requires threading more ad-hoc branches through `frame.rs` and `input.rs`.
- **Input routing drift**: Visual stacking in `frame.rs` and touch/pointer hit-testing in `input.rs` are maintained in two separate places and can disagree.

---

## 2. Goals & Non-Goals

### Goals
1. Define a single, explicit **Z-ordered `DisplayArea` enum** shared by both rendering (`frame.rs`) and input routing (`input.rs`).
2. Define a **`WindowInsets` / `SafeArea` value type** (`top`, `bottom`, `left`, `right`, `ime`, `cutout`) that computes the usable app/shell frame for any output size and orientation.
3. Stay **100% additive on `de-api`**: do not rename or break any existing v1 fields in [geom.rs](../tools/rabbiz-de/crates/de-api/src/geom.rs) or [scene.rs](../tools/rabbiz-de/crates/de-api/src/scene.rs).
4. Keep `de-api` dependency-free (`std` + `serde` only) and host-testable on macOS/Linux.

### Non-Goals (deferred to Tier 2/3)
- Full retained widget tree in `de-ui` (Tier 2.7).
- Split-screen / PiP / Desktop freeform layout math (Tier 3.12) — this RFC only reserves their `DisplayArea` slots and inset inputs.
- Multi-output `DisplayManager` (Tier 3.12).

---

## 3. Proposed `de-api` Additions (v1.1 Additive)

### 3.1 `DisplayArea` Z-Order Stack (`crates/de-api/src/scene.rs`)

Ordered bottom-to-top (`#[repr(u8)]`, derives `PartialOrd, Ord`):

```rust
/// Canonical Z-order bands for both compositing (bottom -> top)
/// and input hit-testing (top -> bottom).
#[repr(u8)]
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum DisplayArea {
    /// Wallpaper and ambient background surfaces.
    Wallpaper = 0,
    /// Home screen / launcher grid / dock / desktop surface.
    Home = 10,
    /// Normal application toplevels (fullscreen, split, or tiled).
    #[default]
    App = 20,
    /// Floating windows, Picture-in-Picture, and app-owned modal dialogs.
    AppOverlay = 30,
    /// Persistent system chrome: status bar, navigation/gesture bar.
    SystemBars = 40,
    /// On-screen keyboard (IME) and input-method popups.
    Ime = 50,
    /// Transient banners: heads-up notifications, volume/brightness OSD, screenshot toast.
    Overlay = 60,
    /// Pull-down notification shade, quick settings, app drawer, recents, search.
    Shade = 70,
    /// Shell-owned menus and system dialogs (context menu, Polkit, Portal permissions).
    SystemDialog = 80,
    /// Lock screen / Keyguard (strictly isolates lower areas when active).
    Keyguard = 90,
    /// Full-screen intents allowed above Keyguard (incoming call, alarm).
    KeyguardOverlay = 100,
    /// Critical system overlays: power menu, screen-off fade, screenshot flash, cursor.
    Critical = 110,
}
```

**Why gaps of 10?** Leaves room for future sub-bands without renumbering serialized values.

### 3.2 `EdgeInsets` & `WindowInsets` (`crates/de-api/src/geom.rs`)

```rust
/// Physical-pixel insets from the four edges of an output or rect.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct EdgeInsets {
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
    pub left: i32,
}

impl EdgeInsets {
    pub const ZERO: Self = Self { top: 0, right: 0, bottom: 0, left: 0 };

    #[must_use]
    pub const fn new(top: i32, right: i32, bottom: i32, left: i32) -> Self {
        Self { top, right, bottom, left }
    }

    /// Component-wise maximum of two inset sets.
    #[must_use]
    pub const fn max(self, other: Self) -> Self {
        Self {
            top: if self.top > other.top { self.top } else { other.top },
            right: if self.right > other.right { self.right } else { other.right },
            bottom: if self.bottom > other.bottom { self.bottom } else { other.bottom },
            left: if self.left > other.left { self.left } else { other.left },
        }
    }

    /// Apply these insets to `rect`, clamping width/height at 0.
    #[must_use]
    pub const fn deflate(self, rect: Rect) -> Rect {
        let w = rect.w - self.left - self.right;
        let h = rect.h - self.top - self.bottom;
        Rect::new(
            rect.x + self.left,
            rect.y + self.top,
            if w > 0 { w } else { 0 },
            if h > 0 { h } else { 0 },
        )
    }
}

/// Complete inset breakdown for a single output.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct WindowInsets {
    /// Status bar + navigation/gesture bar.
    pub system_bars: EdgeInsets,
    /// Display cutout / notch / punch-hole safe area.
    pub cutout: EdgeInsets,
    /// Active on-screen keyboard height (typically `bottom > 0`).
    pub ime: EdgeInsets,
}

impl WindowInsets {
    /// Usable rect for an app that respects system bars, cutouts, and IME.
    #[must_use]
    pub const fn app_frame(&self, output: Size) -> Rect {
        let combined = self.system_bars.max(self.cutout).max(self.ime);
        combined.deflate(Rect::new(0, 0, output.w, output.h))
    }

    /// Usable rect for an immersive/fullscreen app (ignores bars, still respects IME if open).
    #[must_use]
    pub const fn immersive_frame(&self, output: Size) -> Rect {
        self.ime.deflate(Rect::new(0, 0, output.w, output.h))
    }
}
```

### 3.3 `Layer` compatibility
Existing `Layer` fields in `de-api/src/scene.rs` stay frozen. A new `LayerBand` wrapper (or `Scene::push_in(area: DisplayArea, layer: Layer)`) can sit in `de-core` first, or add an optional `area: DisplayArea` field to `Layer` only if owner approves a v1.1 struct extension. **Recommendation**: Keep `Layer` untouched in v1 and introduce `SceneBand { area: DisplayArea, ... }` in `de-core` during Phase 1 so zero existing tests break.

---

## 4. Mapping Current `de-core` Surfaces to `DisplayArea`

| Current `de-core` surface | Target `DisplayArea` | Input rule |
|---|---|---|
| Wallpaper texture | `Wallpaper` | Pass-through |
| `home_ui` (grid, dock, drag ghost) | `Home` | Active when no `App` or `Shade` covers it |
| `space` XDG toplevels + `morph` | `App` | Fullscreen or `insets.app_frame(output)` |
| XDG popups & `set_parent` modals | `AppOverlay` | Clamped to `output` minus `system_bars` |
| `wlr-layer-shell` Bottom/Background | `Wallpaper` / `Home` | Per layer-shell spec |
| `gesture_bar` + status bar | `SystemBars` | Top/bottom edge gesture zones |
| `wlr-layer-shell` Top/Overlay + OSK (`osk.rs`) | `Ime` (when OSK) / `Overlay` | Updates `WindowInsets.ime` on map/unmap |
| `shot_ui` toast, volume/brightness OSD | `Overlay` | Non-blocking except toast tap |
| `panel_ui` (notifications / quick actions), Drawer, Recents, Search | `Shade` | Blocks touch to `App` / `Home` |
| `shell_menu` | `SystemDialog` | Outside tap dismisses |
| `lock_ui` | `Keyguard` | **Hard barrier**: blocks all input & screencopy below `Keyguard` |
| Power menu, fade-to-black, screenshot flash, cursor | `Critical` | Always topmost |

---

## 5. IME / OSK Inset Flow (Solving Keyboard Overlap)

1. When the OSK surface maps or resizes (`layer_shell.rs` / `osk.rs`), `de-core` updates `de.insets.ime.bottom = osk_height_px`.
2. `de-core` re-configures the focused app toplevel with `insets.app_frame(output)` (or `immersive_frame` for fullscreen games/video) via `xdg_toplevel::configure`.
3. When the OSK unmaps, `de.insets.ime = EdgeInsets::ZERO` and the focused toplevel is re-configured back to full usable bounds.
4. For compositor-rendered inputs (`Search`, `Drawer`), `de-ui` receives `WindowInsets` directly so results and pills never render behind the OSK.

---

## 6. Migration Plan (Incremental, Safe)

1. **Step 1 (RFC sign-off + `de-api` types)**: Add `DisplayArea`, `EdgeInsets`, `WindowInsets` + unit tests to `de-api` (no breaking changes).
2. **Step 2 (`de-core` render stack)**: Introduce a `LayerStack` collector in `de-core` that buckets render elements by `DisplayArea` and sorts before submitting to `de-render`. Verify 0 visual regression via `de-smoke`.
3. **Step 3 (`de-core` input stack)**: Route pointer/touch hit-testing top-down through active `DisplayArea`s, enforcing `Keyguard` as an unconditional input barrier.
4. **Step 4 (`WindowInsets` wiring)**: Wire `status_bar` + `gesture_bar` + `osk` heights into `De.insets`, send updated `xdg_toplevel` configure bounds on IME show/hide, and pass `WindowInsets` into `de-ui` screens.

---

## 7. Open Questions for Owner Sign-Off

1. Approve additive `DisplayArea`, `EdgeInsets`, `WindowInsets` types in frozen `de-api`?
2. Should standard handset apps receive `app_frame` (below status bar + above gesture bar) or stay edge-to-edge (`immersive_frame`) by default, shrinking only when `ime.bottom > 0`?
   - *Recommendation*: Keep edge-to-edge by default on handset (matching current A21s look) and apply `ime` inset whenever the OSK is visible, while exposing `system_bars` insets to shell screens and future tablet/desktop modes.
