//! `de-core` — compositor state and frame loop (Linux only).

pub mod frame;
pub mod state;

pub use frame::{event_loop, Run};
pub use state::{init, De, InitError};
