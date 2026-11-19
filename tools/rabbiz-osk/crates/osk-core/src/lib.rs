//! `osk-core` — every piece of the on-screen keyboard that needs neither Wayland nor a GPU:
//! layouts and key geometry, the shift/caps state machine, touch gestures, and the
//! key-press → command translation. Host-testable with `cargo test`.

pub mod emoji;
pub mod gesture;
pub mod layout;
pub mod predict;
pub mod state;

pub use emoji::EmojiIndex;
pub use gesture::{GestureDetector, GestureEvent, TouchEvent, TouchKind};
pub use layout::{Geometry, Key, KeyAction, KeyPos, Layer, Layout, LayoutError, Rect};
pub use predict::Predictor;
pub use state::{Command, InputPurpose, Keyboard, ShiftState};

/// Hardware-keyboard mode: when `RABBIZ_HW_KEYBOARD` is set (any value, even empty) the
/// keyboard must serve `zwp_input_method_v2` but never show its layer-shell window.
/// `rabbiz-de` sets this env when a physical keyboard is hotplugged and respawns the OSK,
/// so this is read once at startup — no polling.
pub fn hw_keyboard_mode() -> bool {
    std::env::var_os("RABBIZ_HW_KEYBOARD").is_some()
}
