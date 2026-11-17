//! Input events delivered from the libinput backend to `de-core`/`de-ui`.
//!
//! Every event carries the kernel/libinput timestamp (`time_ms`, CLOCK_MONOTONIC
//! milliseconds) so gesture code can compute release velocity for the
//! spring-based motion contract (plan §7.17a).

use serde::{Deserialize, Serialize};

use crate::geom::Point;

/// Touch/contact phase for a single slot.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TouchPhase {
    Down,
    Motion,
    Up,
    Cancel,
}

/// Logical key/button state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum KeyState {
    Pressed,
    Released,
}

/// A key identified by evdev keycode plus (optionally) the resolved keysym.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct KeyEvent {
    /// evdev keycode (`KEY_*`).
    pub keycode: u32,
    /// Keysym when the backend can resolve one (xkbcommon), else `None`.
    pub keysym: Option<u32>,
    pub state: KeyState,
    pub time_ms: u64,
}

/// A pointer (mouse) event.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum PointerEvent {
    Motion {
        pos: Point,
        time_ms: u64,
    },
    Button {
        /// evdev button code (`BTN_LEFT`, ...).
        button: u32,
        state: KeyState,
        time_ms: u64,
    },
    Axis {
        /// Vertical scroll delta (positive = content up / wheel down), px.
        delta: f64,
        time_ms: u64,
    },
}

/// A touchscreen slot event.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct TouchEvent {
    /// Slot/finger identifier, stable for the contact's lifetime.
    pub slot: i32,
    pub phase: TouchPhase,
    pub pos: Point,
    pub time_ms: u64,
}

/// A tablet-mode hardware switch event (lid, tablet-mode).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SwitchEvent {
    Lid { closed: bool, time_ms: u64 },
    TabletMode { on: bool, time_ms: u64 },
}

/// The unified input stream entering `de-core`.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum InputEvent {
    Touch(TouchEvent),
    Pointer(PointerEvent),
    Key(KeyEvent),
    Switch(SwitchEvent),
}

impl InputEvent {
    #[must_use]
    pub fn time_ms(&self) -> u64 {
        match *self {
            Self::Touch(t) => t.time_ms,
            Self::Pointer(PointerEvent::Motion { time_ms, .. })
            | Self::Pointer(PointerEvent::Button { time_ms, .. })
            | Self::Pointer(PointerEvent::Axis { time_ms, .. }) => time_ms,
            Self::Key(k) => k.time_ms,
            Self::Switch(SwitchEvent::Lid { time_ms, .. })
            | Self::Switch(SwitchEvent::TabletMode { time_ms, .. }) => time_ms,
        }
    }
}
