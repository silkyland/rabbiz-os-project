//! Core compositor state, frame loop, stats, and desktop contract env.

pub mod env;
pub mod frame;
pub mod state;

pub use frame::run;
pub use state::{RendererChoice, RunConfig};
