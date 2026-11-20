//! Shared Material Icons Round codepoints (verified against the bundled
//! `MaterialIconsRound-Regular.ttf` cmap in `tools/rabbiz-de/assets/fonts/`).
//!
//! Centralizes commonly duplicated icon constants across `rabbiz-*` apps.

pub const BACK: &str = "\u{e5c4}";
pub const CLOSE: &str = "\u{e5cd}";
pub const SEARCH: &str = "\u{e8b6}";
pub const ADD: &str = "\u{e145}";
pub const DELETE: &str = "\u{e872}";
pub const EDIT: &str = "\u{e3c9}";
pub const CHECK: &str = "\u{e5ca}";
pub const CALL: &str = "\u{e0b0}";
pub const MESSAGE: &str = "\u{e0c9}";
pub const PLAY: &str = "\u{e037}";
pub const PAUSE: &str = "\u{e034}";
pub const SKIP_NEXT: &str = "\u{e044}";
pub const SKIP_PREV: &str = "\u{e045}";
pub const FOLDER: &str = "\u{e2c7}";
pub const FILE: &str = "\u{e24d}";
pub const SETTINGS: &str = "\u{e8b8}";
pub const SHARE: &str = "\u{e80d}";
pub const STAR: &str = "\u{e838}";
pub const STAR_OUTLINE: &str = "\u{e83a}";
pub const MENU: &str = "\u{e5d2}";
pub const CHEVRON_RIGHT: &str = "\u{e5cc}";
pub const MORE_VERT: &str = "\u{e5d4}";
pub const REFRESH: &str = "\u{e5d5}";
pub const WARNING: &str = "\u{e002}";
pub const INFO: &str = "\u{e88e}";

#[cfg(test)]
mod tests {
    use super::*;

    const ALL: &[(&str, &str)] = &[
        ("BACK", BACK),
        ("CLOSE", CLOSE),
        ("SEARCH", SEARCH),
        ("ADD", ADD),
        ("DELETE", DELETE),
        ("EDIT", EDIT),
        ("CHECK", CHECK),
        ("CALL", CALL),
        ("MESSAGE", MESSAGE),
        ("PLAY", PLAY),
        ("PAUSE", PAUSE),
        ("SKIP_NEXT", SKIP_NEXT),
        ("SKIP_PREV", SKIP_PREV),
        ("FOLDER", FOLDER),
        ("FILE", FILE),
        ("SETTINGS", SETTINGS),
        ("SHARE", SHARE),
        ("STAR", STAR),
        ("STAR_OUTLINE", STAR_OUTLINE),
        ("MENU", MENU),
        ("CHEVRON_RIGHT", CHEVRON_RIGHT),
        ("MORE_VERT", MORE_VERT),
        ("REFRESH", REFRESH),
        ("WARNING", WARNING),
        ("INFO", INFO),
    ];

    #[test]
    fn all_icons_are_single_scalar_material_pua_codepoints() {
        for &(name, glyph) in ALL {
            assert!(!glyph.is_empty(), "{name} must not be empty");
            let mut chars = glyph.chars();
            let ch = chars.next().expect("non-empty icon");
            assert!(
                chars.next().is_none(),
                "{name} must be a single Unicode scalar"
            );
            let cp = u32::from(ch);
            assert!(
                (0xE000..=0xF8FF).contains(&cp),
                "{name} (U+{cp:04X}) must be in the Material PUA range U+E000..=U+F8FF"
            );
        }
    }
}
