//! `de-core` — compositor state, Wayland protocol handlers and frame loop
//! (Linux only).

pub mod comp;
pub mod frame;
pub mod input;
pub mod misc;
pub mod seat;
pub mod selection;
pub mod state;
pub mod xdg;

pub use frame::{event_loop, Run};
pub use state::{init, De, DeClientState, InitError};
