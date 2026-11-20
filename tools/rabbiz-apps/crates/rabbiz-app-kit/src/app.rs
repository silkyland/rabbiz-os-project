//! Shared application traits and input/frame types across Wayland, host
//! simulator, and headless test harness.

use std::time::Duration;

use de_api::Point;
use de_design::tokens::Theme;
use de_paint::Canvas;

use crate::wayland::TextInputCtl;

/// evdev `BTN_LEFT` — primary pointer button routed through `Touch*` events.
pub const BTN_LEFT: u32 = 0x110;
/// evdev `BTN_RIGHT` — secondary pointer button.
pub const BTN_RIGHT: u32 = 0x111;
/// evdev `BTN_MIDDLE` — middle pointer button.
pub const BTN_MIDDLE: u32 = 0x112;

#[derive(Debug, Clone)]
pub enum AppEvent {
    TouchDown {
        id: i32,
        pos: Point,
    },
    TouchMove {
        id: i32,
        pos: Point,
    },
    TouchUp {
        id: i32,
        pos: Point,
    },
    /// Mouse hover motion with no button held (touch never produces this).
    PointerMove {
        pos: Point,
    },
    /// Mouse wheel / touchpad scroll, in surface pixels (`wl_pointer` axis
    /// `absolute` values; positive dy scrolls content down).
    Scroll {
        dx: f32,
        dy: f32,
    },
    /// Non-primary mouse button edge. `button` is the evdev code
    /// (`0x110` left is routed to `Touch*` instead; right = `0x111`, middle =
    /// `0x112`, others pass through).
    PointerButton {
        button: u32,
        pressed: bool,
        pos: Point,
    },
    Key {
        keysym: u32,
        utf8: Option<String>,
    },
    TextCommit(String),
    TextBackspace(usize),
    Tick(Duration),
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct AppFrame {
    pub dirty: bool,
    pub animating: bool,
    pub quit: bool,
}

pub trait AppState {
    fn app_id(&self) -> &'static str;
    fn title(&self) -> &'static str;
    fn handle_event(&mut self, event: AppEvent, ti: &TextInputCtl) -> AppFrame;
    fn render(&mut self, canvas: &mut Canvas, theme: &Theme);
}
