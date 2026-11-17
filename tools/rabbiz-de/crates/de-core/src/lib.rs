//! Core compositor state, frame loop, stats, input policy and seat wiring.

pub mod frame;
pub mod input;
pub mod present;
pub mod seat;
pub mod state;

pub use frame::run;
pub use state::{RendererChoice, RunConfig};
