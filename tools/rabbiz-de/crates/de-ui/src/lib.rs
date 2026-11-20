//! `de-ui` — the in-process shell screens (P0–P20), shared chrome, and
//! interaction/navigation/i18n primitives.

pub mod chrome;
pub mod interaction;
pub mod screens;

// Re-export module paths at crate root so internal and downstream imports stay concise.
pub use chrome::{menu, nav_bar};
pub use interaction::{gestures, hun, i18n, nav, pager, pull, transitions};
pub use screens::{
    boot, drawer, error, first_run, home, hun_banner, launch_morph, lock, lock_bouncer, media_card,
    notification_groups, osd, panels, power, qs, recents, screenshot, search, settings, shade,
    status,
};
