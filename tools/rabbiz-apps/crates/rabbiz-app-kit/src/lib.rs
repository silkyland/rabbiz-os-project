//! Shared Material Design 3 + native Wayland (`wl_shm` + `xdg_toplevel` +
//! `wl_touch` + `zwp_text_input_v3`) and Tier 1 host simulator runtime for
//! all `rabbiz-*` Rust apps.

pub mod app;
pub mod harness;
pub mod icons;
pub mod intent;
pub mod mcp;
pub mod preview;
pub mod raster_image;
pub mod scroll;
pub mod sim_config;
pub mod text_input;
pub mod theme_watch;
pub mod ui;
#[cfg(target_os = "linux")]
pub mod wayland;
#[cfg(not(target_os = "linux"))]
#[path = "host_window.rs"]
pub mod wayland;
pub mod widgets;

pub use app::{AppEvent, AppFrame, AppState, BTN_LEFT, BTN_MIDDLE, BTN_RIGHT};
pub use de_api::{Point, Rect, Size};
pub use de_design::tokens::{self, Theme};
pub use de_paint::{text, Canvas};
pub use harness::{GoldenCompare, TestHarness};
pub use intent::Intent;
pub use scroll::ScrollView;
pub use sim_config::{DevicePreset, SimOptions};
pub use text_input::{ContentPurpose, TextBuffer};
pub use theme_watch::ThemeWatcher;
pub use wayland::{run_app, TextInputCtl};

/// Load active Light/Dark `Theme` from `de-config::Prefs`, with the
/// persisted `deAccent` ramp applied.
#[must_use]
pub fn load_theme() -> Theme {
    let prefs = de_config::Prefs::load();
    de_design::theme::theme_with(&prefs.theme_mode, &prefs.accent)
}
