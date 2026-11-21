//! `de-design` — the Rabbiz design tokens (plan §7.17/§7.17a).
//!
//! M2-T1 surface: full [`tokens`] (dark + Cream Glass light), [`type_scale`],
//! [`motion`] (spring presets, fallback bezier ladder, degrade ladder) and
//! [`theme`] (mode selection + persisted mode file). Contrast contract is
//! enforced by `tests/contrast.rs`.

pub mod motion;
pub mod theme;
pub mod tokens;
pub mod type_scale;

pub use theme::{accent_colors, theme_for, theme_with, AccentColors, Mode, ACCENT_IDS};
pub use tokens::Theme;
pub use tokens::{DARK, DEFAULT, LIGHT};
