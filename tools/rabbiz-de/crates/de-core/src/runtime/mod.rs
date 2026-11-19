//! Compositor runtime: state, event loop, rendering, input, power, OSK, and IPC.

pub mod composite;
pub(crate) mod ctl_ipc;
pub(crate) mod cursor;
pub(crate) mod display_stack;
pub mod frame;
pub(crate) mod frame_io;
pub(crate) mod haptics;
pub mod input;
pub mod launch;
pub(crate) mod osk;
pub(crate) mod power;
pub(crate) mod shortcuts;
pub mod snapshots;
pub mod state;
