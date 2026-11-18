//! Theme selection: the light Cream Glass map is the v1 default
//! (Decision 9), `DARK` is the dark mode. `Display > dark/light` in P15
//! writes the mode; `~/.config/rabbiz-de/theme` carries `dark|light`
//! (gamer-owned, same convention as `rabbiz-de/mode`).

use crate::tokens::{Theme, DARK, DEFAULT, LIGHT};

/// UI theme mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Mode {
    /// Cream Glass light (v1 default).
    #[default]
    Light,
    /// Dark plum.
    Dark,
}

impl Mode {
    /// Parse a mode file's contents (`dark` / `light`; anything else keeps
    /// the default).
    #[must_use]
    pub fn parse(s: &str) -> Self {
        match s.trim() {
            "dark" => Self::Dark,
            _ => Self::Light,
        }
    }

    /// Palette for this mode.
    #[must_use]
    pub const fn theme(self) -> Theme {
        match self {
            Self::Light => LIGHT,
            Self::Dark => DARK,
        }
    }
}

/// Palette for a mode string, defaulting to `DEFAULT` on unknown input.
#[must_use]
pub fn theme_for(mode: &str) -> Theme {
    let t = Mode::parse(mode).theme();
    // Mode::parse defaults to Light which is also DEFAULT; keep both
    // explicit so flipping DEFAULT later doesn't silently change callers.
    if mode.trim() == "dark" {
        t
    } else if mode.trim() == "light" {
        LIGHT
    } else {
        DEFAULT
    }
}

/// Mode file path under `$XDG_CONFIG_HOME` (defaults `~/.config`).
#[must_use]
pub fn mode_file() -> std::path::PathBuf {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(std::path::PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| std::path::PathBuf::from(h).join(".config")))
        .unwrap_or_else(|| std::path::PathBuf::from("."));
    base.join("rabbiz-de").join("theme")
}

/// Read the persisted mode, defaulting to `Mode::default()` (Light).
#[must_use]
pub fn persisted_mode() -> Mode {
    std::fs::read_to_string(mode_file())
        .map(|s| Mode::parse(&s))
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_modes() {
        assert_eq!(Mode::parse("dark"), Mode::Dark);
        assert_eq!(Mode::parse("light\n"), Mode::Light);
        assert_eq!(Mode::parse("weird"), Mode::Light);
        assert_eq!(theme_for("dark").bg, DARK.bg);
        assert_eq!(theme_for("").bg, DEFAULT.bg);
    }
}
