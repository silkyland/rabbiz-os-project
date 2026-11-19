//! `de-config` — flags, prefs, wallpaper/accent ids.

pub mod device_profile;
pub mod flags;
pub mod ipc;
pub mod keyboards;
pub mod prefs;
pub mod privileged;

pub use flags::{Cli, Command, Flags, RendererSelect};
pub use prefs::Prefs;
pub use privileged::{Policy, Privileged};
