//! Screen scenes. No I/O, no renderer, no OS: a screen returns a [`Scene`].
//!
//! The host slice ships the P0 boot frame and the P1 lock screen. Later
//! milestones add the `home`, `drawer` and `shade` modules next to them.

use de_api::{Scene, Size};

pub mod boot;
pub mod lock;

pub fn boot_frame(size: Size) -> Scene {
    boot::build(size)
}

pub fn lock_frame(size: Size) -> Scene {
    lock::build(size)
}
