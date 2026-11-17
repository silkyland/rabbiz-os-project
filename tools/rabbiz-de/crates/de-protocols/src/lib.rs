//! Wayland protocol state and Smithay handlers for `rabbiz-de`.

pub mod dmabuf;
pub mod seat;
pub mod selection;
pub mod state;
pub mod xdg;

pub use state::{ClientState, ProtocolState};
