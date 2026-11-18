//! `osk-core` — every piece of the on-screen keyboard that needs neither Wayland nor a GPU:
//! layouts and key geometry, the shift/caps state machine, touch gestures, and the
//! key-press → command translation. Host-testable with `cargo test`.

pub mod gesture;
pub mod emoji;
pub mod layout;
pub mod predict;
pub mod state;

pub use gesture::{GestureDetector, GestureEvent, TouchEvent, TouchKind};
pub use predict::Predictor;
pub use layout::{Geometry, Key, KeyAction, KeyPos, Layer, Layout, LayoutError, Rect};
pub use emoji::EmojiIndex;
pub use state::{Command, InputPurpose, Keyboard, ShiftState};
