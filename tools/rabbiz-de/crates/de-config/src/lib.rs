//! `de-config` — flags, prefs, wallpaper/accent ids.

pub mod flags;
pub mod prefs;

pub use flags::{Cli, Command, Flags, RendererSelect};
pub use prefs::Prefs;
