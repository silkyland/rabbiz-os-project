//! XDG `.desktop` catalog, AppCatalog dedupe/preference groups, and launch policy.

pub mod catalog;
pub mod desktop;
pub mod launch;

pub use catalog::{visible_entries, PREFERRED_GROUPS};
pub use desktop::{load_desktop_dir, parse_desktop_entry, parse_desktop_file, DesktopEntry};
pub use launch::{LaunchOverride, LaunchPolicy, LaunchSpec};
