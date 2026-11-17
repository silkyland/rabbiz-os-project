//! RabbizOS DE configuration and legacy `a21s-shell/prefs.json` migration.

pub mod prefs;

pub use prefs::{
    default_de_prefs_path, default_legacy_prefs_path, load_or_migrate, save_atomic, ConfigError,
    Prefs,
};
