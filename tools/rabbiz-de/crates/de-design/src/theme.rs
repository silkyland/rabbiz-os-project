//! Theme selection: the light Cream Glass map is the v1 default
//! (Decision 9), `DARK` is the dark mode. `Display > dark/light` in P15
//! writes the mode; `~/.config/rabbiz-de/theme` carries `dark|light`
//! (rabbiz-owned, same convention as `rabbiz-de/mode`).

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

// ---------------------------------------------------------------------------
// Accent resolution (`deAccent` in prefs.json)
// ---------------------------------------------------------------------------

/// Resolved accent ramp for one `deAccent` id + mode: the four `Theme`
/// fields the accent system owns (`accent`, `accent_hi`, `accent_dim`,
/// `text_on_accent`). Every other token comes from the mode map.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AccentColors {
    /// Primary accent fill (buttons, active states).
    pub accent: [u8; 4],
    /// Lighter shade — glow/highlight over surfaces.
    pub accent_hi: [u8; 4],
    /// Darker shade — pressed state.
    pub accent_dim: [u8; 4],
    /// Legible text/glyph color drawn on `accent`/`accent_dim` fills.
    pub text_on_accent: [u8; 4],
}

/// `deAccent` ids understood by [`accent_colors`] — the brand `copper`
/// default plus the legacy shell palette (`ShellPrefs.accents`). Kept in
/// sync with the validation list in `rabbiz-settings` (`cli.rs::ACCENT_IDS`).
pub const ACCENT_IDS: &[&str] = &["copper", "mono", "blue", "emerald", "amber", "crimson"];

const ON_LIGHT: [u8; 4] = LIGHT.text_on_accent;
const ON_DARK: [u8; 4] = DARK.text_on_accent;

const COPPER_LIGHT: AccentColors = AccentColors {
    accent: LIGHT.accent,
    accent_hi: LIGHT.accent_hi,
    accent_dim: LIGHT.accent_dim,
    text_on_accent: LIGHT.text_on_accent,
};
const COPPER_DARK: AccentColors = AccentColors {
    accent: DARK.accent,
    accent_hi: DARK.accent_hi,
    accent_dim: DARK.accent_dim,
    text_on_accent: DARK.text_on_accent,
};
const MONO_LIGHT: AccentColors = AccentColors {
    accent: [0x3F, 0x3F, 0x46, 0xFF],     // zinc-700
    accent_hi: [0x71, 0x71, 0x7A, 0xFF],  // zinc-500
    accent_dim: [0x27, 0x27, 0x2A, 0xFF], // zinc-800
    text_on_accent: ON_LIGHT,
};
const MONO_DARK: AccentColors = AccentColors {
    accent: [0xE4, 0xE4, 0xE7, 0xFF], // zinc-200 (legacy shell `mono` color)
    accent_hi: [0xF4, 0xF4, 0xF5, 0xFF], // zinc-100
    accent_dim: [0xA1, 0xA1, 0xAA, 0xFF], // zinc-400
    text_on_accent: ON_DARK,
};
const BLUE_LIGHT: AccentColors = AccentColors {
    accent: [0x1D, 0x4E, 0xD8, 0xFF], // blue-700 (500 is under 4.5:1 on cream)
    accent_hi: [0x60, 0xA5, 0xFA, 0xFF], // blue-400
    accent_dim: [0x1E, 0x40, 0xAF, 0xFF], // blue-800
    text_on_accent: ON_LIGHT,
};
const BLUE_DARK: AccentColors = AccentColors {
    accent: [0x60, 0xA5, 0xFA, 0xFF],     // blue-400
    accent_hi: [0x93, 0xC5, 0xFD, 0xFF],  // blue-300
    accent_dim: [0x25, 0x63, 0xEB, 0xFF], // blue-600
    text_on_accent: ON_DARK,
};
const EMERALD_LIGHT: AccentColors = AccentColors {
    accent: [0x15, 0x80, 0x3D, 0xFF],     // green-700
    accent_hi: [0x4A, 0xDE, 0x80, 0xFF],  // green-400
    accent_dim: [0x16, 0x65, 0x34, 0xFF], // green-800
    text_on_accent: ON_LIGHT,
};
const EMERALD_DARK: AccentColors = AccentColors {
    accent: [0x4A, 0xDE, 0x80, 0xFF],     // green-400
    accent_hi: [0x86, 0xEF, 0xAC, 0xFF],  // green-300
    accent_dim: [0x16, 0xA3, 0x4A, 0xFF], // green-600
    text_on_accent: ON_DARK,
};
const AMBER_LIGHT: AccentColors = AccentColors {
    accent: [0xB4, 0x53, 0x09, 0xFF], // amber-700 (500 is unreadable on cream)
    accent_hi: [0xFB, 0xBF, 0x24, 0xFF], // amber-400
    accent_dim: [0x92, 0x40, 0x0E, 0xFF], // amber-800
    text_on_accent: ON_LIGHT,
};
const AMBER_DARK: AccentColors = AccentColors {
    accent: [0xFB, 0xBF, 0x24, 0xFF],     // amber-400
    accent_hi: [0xFC, 0xD3, 0x4D, 0xFF],  // amber-300
    accent_dim: [0xB4, 0x53, 0x09, 0xFF], // amber-700
    text_on_accent: ON_DARK,
};
const CRIMSON_LIGHT: AccentColors = AccentColors {
    accent: [0xB9, 0x1C, 0x1C, 0xFF],     // red-700
    accent_hi: [0xF8, 0x71, 0x71, 0xFF],  // red-400
    accent_dim: [0x7F, 0x1D, 0x1D, 0xFF], // red-900
    text_on_accent: ON_LIGHT,
};
const CRIMSON_DARK: AccentColors = AccentColors {
    accent: [0xF8, 0x71, 0x71, 0xFF],     // red-400
    accent_hi: [0xFC, 0xA5, 0xA5, 0xFF],  // red-300
    accent_dim: [0xB9, 0x1C, 0x1C, 0xFF], // red-700
    text_on_accent: ON_DARK,
};

/// Accent ramp for `id` in `mode`. Returns `None` for unknown ids so callers
/// can keep the mode's base palette (copper — the brand default).
///
/// The ramps follow the base themes' shape: light mode uses a dark,
/// AA-contrast accent with near-white text; dark mode uses a lighter accent
/// with plum-dark text. `copper` reproduces the base maps exactly, so
/// `theme_with(m, "copper") == theme_for(m)`.
#[must_use]
pub fn accent_colors(mode: Mode, id: &str) -> Option<AccentColors> {
    Some(match (mode, id.trim()) {
        (Mode::Light, "copper") => COPPER_LIGHT,
        (Mode::Dark, "copper") => COPPER_DARK,
        (Mode::Light, "mono") => MONO_LIGHT,
        (Mode::Dark, "mono") => MONO_DARK,
        (Mode::Light, "blue") => BLUE_LIGHT,
        (Mode::Dark, "blue") => BLUE_DARK,
        (Mode::Light, "emerald") => EMERALD_LIGHT,
        (Mode::Dark, "emerald") => EMERALD_DARK,
        (Mode::Light, "amber") => AMBER_LIGHT,
        (Mode::Dark, "amber") => AMBER_DARK,
        (Mode::Light, "crimson") => CRIMSON_LIGHT,
        (Mode::Dark, "crimson") => CRIMSON_DARK,
        _ => return None,
    })
}

/// [`theme_for`] with the `deAccent` override applied: only the four
/// accent-owned fields change — surfaces, text, glass and semantics stay
/// on the mode map. Unknown/empty ids resolve to the base palette (copper).
#[must_use]
pub fn theme_with(mode: &str, accent: &str) -> Theme {
    let mut t = theme_for(mode);
    if let Some(a) = accent_colors(Mode::parse(mode), accent) {
        t.accent = a.accent;
        t.accent_hi = a.accent_hi;
        t.accent_dim = a.accent_dim;
        t.text_on_accent = a.text_on_accent;
    }
    t
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

    #[test]
    fn accent_table_covers_every_id_in_both_modes() {
        for id in ACCENT_IDS {
            for mode in [Mode::Light, Mode::Dark] {
                assert!(
                    accent_colors(mode, id).is_some(),
                    "missing ramp for {id}/{mode:?}"
                );
            }
        }
        assert!(accent_colors(Mode::Light, "neon-pink").is_none());
        assert!(accent_colors(Mode::Dark, "").is_none());
    }

    #[test]
    fn copper_reproduces_the_base_palettes() {
        // `copper` is the brand default — applying it must be a no-op.
        assert_eq!(
            theme_with("light", "copper").accent,
            theme_for("light").accent
        );
        assert_eq!(
            theme_with("dark", "copper").accent,
            theme_for("dark").accent
        );
        assert_eq!(theme_with("light", "copper").bg, LIGHT.bg);
    }

    #[test]
    fn theme_with_swaps_only_the_accent_fields() {
        let t = theme_with("light", "blue");
        assert_eq!(t.accent, [0x1D, 0x4E, 0xD8, 0xFF]);
        assert_ne!(t.accent, LIGHT.accent);
        // Non-accent fields come straight from the mode map.
        assert_eq!(t.surface1, LIGHT.surface1);
        assert_eq!(t.text, LIGHT.text);
        assert_eq!(t.bg, LIGHT.bg);
        // Unknown ids keep the base (copper) ramp.
        assert_eq!(theme_with("dark", "bogus").accent, DARK.accent);
    }

    #[test]
    fn accent_text_pairs_stay_legible() {
        use crate::tokens::contrast_ratio;
        for id in ACCENT_IDS {
            for mode in [Mode::Light, Mode::Dark] {
                let a = accent_colors(mode, id).unwrap();
                let on_accent = contrast_ratio(a.text_on_accent, a.accent);
                assert!(
                    on_accent >= 4.5,
                    "{id}/{mode:?}: text_on_accent/accent = {on_accent:.2}"
                );
                let on_bg = contrast_ratio(a.accent, mode.theme().bg);
                assert!(on_bg >= 4.0, "{id}/{mode:?}: accent/bg = {on_bg:.2}");
                // The shipped contract only requires text_on_accent on
                // accent_dim for LIGHT (contrast.rs `light_theme_pairs`) —
                // dark-mode dims are deep pressed shades on plum surfaces.
                if mode == Mode::Light {
                    let on_dim = contrast_ratio(a.text_on_accent, a.accent_dim);
                    assert!(
                        on_dim >= 4.5,
                        "{id}/Light: text_on_accent/accent_dim = {on_dim:.2}"
                    );
                }
            }
        }
    }
}
