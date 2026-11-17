//! `de-design` — the Rabbiz design tokens (plan §7.17).
//!
//! M0 ships only what the boot/golden scenes need (dark base palette). The
//! complete token set, type scale, motion presets and light map are M2-T1.

pub mod tokens;

pub use tokens::Theme;
pub use tokens::{DARK, DEFAULT};
