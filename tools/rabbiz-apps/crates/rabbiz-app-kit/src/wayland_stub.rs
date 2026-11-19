//! Non-Linux host stub for `rabbiz_app_kit::wayland`.
//!
//! Keeps all 12 `rabbiz-*` apps compiling and running their unit/headless/CLI
//! tests natively on macOS without requiring `smithay-client-toolkit` (Linux).

use std::time::Duration;

use de_api::Point;
use de_design::tokens::Theme;
use de_paint::Canvas;
use wayland_protocols::wp::text_input::zv3::client::zwp_text_input_v3::ContentPurpose;

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
    PointerMove {
        pos: Point,
    },
    Scroll {
        dx: f32,
        dy: f32,
    },
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

#[derive(Default)]
pub struct AppFrame {
    pub dirty: bool,
    pub animating: bool,
    pub quit: bool,
}

#[derive(Default)]
pub struct TextInputCtl;

impl TextInputCtl {
    pub fn activate(&self, _purpose: ContentPurpose) {}
    pub fn set_surrounding(&self, _text: &str, _cursor_bytes: usize) {}
    pub fn deactivate(&self) {}
}

pub trait AppState {
    fn app_id(&self) -> &'static str;
    fn title(&self) -> &'static str;
    fn handle_event(&mut self, event: AppEvent, ti: &TextInputCtl) -> AppFrame;
    fn render(&mut self, canvas: &mut Canvas, theme: &Theme);
}

pub fn run_app<S: AppState + 'static>(_state: S) {
    panic!("rabbiz-app-kit Wayland runner requires Linux");
}
