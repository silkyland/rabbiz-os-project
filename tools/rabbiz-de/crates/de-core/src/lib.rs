//! `de-core` — compositor state, Wayland protocol handlers and frame loop
//! (Linux only).

pub mod comp;
pub mod composite;
pub mod frame;
pub mod input;
pub mod launch;
mod home_ui;
mod layer_shell;
mod lock_ui;
pub mod misc;
mod morph;
mod navigation;
mod panel_ui;
mod osk;
mod popups;
mod power;
mod shell_menu;
mod shot_ui;
mod privileged;
pub mod screencopy;
pub mod seat;
pub mod selection;
pub mod shell;
pub mod snapshots;
pub mod state;
pub mod virtual_pointer;
pub mod xdg;

pub use frame::{event_loop, Run};
pub use state::{init, De, DeClientState, InitError};
