//! `de-core` — compositor state, Wayland protocol handlers and frame loop
//! (Linux only).

pub mod comp;
pub mod composite;
pub mod frame;
pub mod input;
pub mod launch;
mod layer_shell;
pub mod misc;
mod morph;
mod osk;
mod popups;
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
