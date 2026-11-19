//! Shared Material Design 3 + native Wayland (`wl_shm` + `xdg_toplevel` +
//! `wl_touch` + `zwp_text_input_v3`) runtime for all `rabbiz-*` Rust apps.

pub mod ui;
pub mod wayland;

pub use de_api::{Point, Rect, Size};
pub use de_design::tokens::{self, Theme};
pub use de_paint::{text, Canvas};
pub use wayland::{run_app, AppEvent, AppFrame, AppState, TextInputCtl};

/// Load active Light/Dark `Theme` from `de-config::Prefs`.
#[must_use]
pub fn load_theme() -> Theme {
    let prefs = de_config::Prefs::load();
    de_design::theme::theme_for(&prefs.theme_mode)
}
