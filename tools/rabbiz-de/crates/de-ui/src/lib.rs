//! Screen scenes. No I/O, no renderer, no OS: a screen returns a [`Scene`].
//!
//! The first slice only needs the P0 boot frame. Later milestones add the
//! `home`, `drawer`, `shade`, `lock` modules next to it.

use de_api::{Scene, Size};

pub mod boot;

pub fn boot_frame(size: Size) -> Scene {
    boot::build(size)
}
