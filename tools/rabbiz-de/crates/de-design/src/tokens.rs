//! Design tokens from plan §7.17 (+ §Decision 9 light "Cream Glass" map).
//! All values are physical px at the 720x1600 panel (multiply by 1.5 for the
//! tablet seam later — `crate::type_scale::TABLET_SCALE`).
//!
//! Colors are sRGB `[r, g, b, a]`, matching `de-api`'s premultiplied contract
//! at the paint boundary (callers premultiply where `a < 255`).

/// Active theme palette.
#[derive(Debug, Clone, Copy)]
pub struct Theme {
    pub bg: [u8; 4],
    pub canvas: [u8; 4],
    pub surface1: [u8; 4],
    pub surface2: [u8; 4],
    pub surface3: [u8; 4],
    pub text: [u8; 4],
    pub text2: [u8; 4],
    pub text3: [u8; 4],
    pub text_on_accent: [u8; 4],
    pub accent: [u8; 4],
    pub accent_hi: [u8; 4],
    pub accent_dim: [u8; 4],
    pub success: [u8; 4],
    pub warning: [u8; 4],
    pub danger: [u8; 4],
    pub line: [u8; 4],
    pub scrim: [u8; 4],
    /// Translucent card/pill fill over wallpaper (never glass-on-glass).
    pub glass: [u8; 4],
    /// Stronger glass for elements needing more legibility.
    pub glass_hi: [u8; 4],
    /// Card shadow color (premultiplied alpha baked in at draw time).
    pub shadow: [u8; 4],
    /// First-party icon tile fill (sand gradient base).
    pub icon_tile: [u8; 4],
    /// First-party icon glyph color.
    pub icon_glyph: [u8; 4],
}

/// Dark "plum" theme (plan §7.17 base).
pub const DARK: Theme = Theme {
    bg: [0x16, 0x10, 0x1A, 0xFF],
    canvas: [0x1D, 0x15, 0x22, 0xFF],
    surface1: [0x24, 0x1A, 0x2A, 0xFF],
    surface2: [0x2E, 0x21, 0x36, 0xFF],
    surface3: [0x3A, 0x2A, 0x43, 0xFF],
    text: [0xF4, 0xED, 0xE1, 0xFF],
    text2: [0xCD, 0xBF, 0xC4, 0xFF],
    text3: [0x9C, 0x8A, 0x93, 0xFF],
    text_on_accent: [0x2A, 0x17, 0x10, 0xFF],
    accent: [0xD0, 0x8A, 0x5A, 0xFF],
    accent_hi: [0xE0, 0xA2, 0x73, 0xFF],
    accent_dim: [0x8C, 0x5B, 0x3B, 0xFF],
    success: [0x86, 0xC9, 0xA3, 0xFF],
    warning: [0xE8, 0xC4, 0x6A, 0xFF],
    danger: [0xE2, 0x80, 0x7F, 0xFF],
    line: [0xF4, 0xED, 0xE1, 0x1A],
    scrim: [0x0A, 0x06, 0x0C, 0xB8],
    // rgba(36,26,42,.72) / .86 over wallpaper.
    glass: [0x24, 0x1A, 0x2A, 0xB8],
    glass_hi: [0x24, 0x1A, 0x2A, 0xDB],
    shadow: [0x08, 0x04, 0x0A, 0x59],
    icon_tile: [0xDA, 0xC9, 0xB3, 0xFF],
    icon_glyph: [0x30, 0x21, 0x35, 0xFF],
};

/// Light "Cream Glass" theme — the owner-preferred v1 default (plan
/// Decision 9 + `docs/DE-DESIGN-LIGHT-TOKENS.md`). Warm cream canvas, frosted
/// white-glass pills, copper accent, plum text.
///
/// §7.17 light map: bg #F4EDE1, surface #FFF9F0, text #2A1D2F,
/// text2 #6B5A63, accent #A55C32, line rgba(42,29,47,.12).
pub const LIGHT: Theme = Theme {
    bg: [0xF4, 0xED, 0xE1, 0xFF],       // cream-200
    canvas: [0xF9, 0xF5, 0xEE, 0xFF],   // cream-100
    surface1: [0xFF, 0xF9, 0xF0, 0xFF], // light map surface
    surface2: [0xFD, 0xFB, 0xF7, 0xFF], // cream-50
    surface3: [0xEA, 0xDD, 0xC9, 0xFF], // cream-300
    text: [0x30, 0x21, 0x35, 0xFF],     // plum
    text2: [0x58, 0x43, 0x5E, 0xFF],    // plum-muted
    text3: [0x7A, 0x64, 0x82, 0xFF],    // plum-subtle
    text_on_accent: [0xFD, 0xFB, 0xF7, 0xFF],
    accent: [0xA6, 0x5A, 0x2E, 0xFF], // copper (darkened to meet 4.5:1 with text_on_accent)
    accent_hi: [0xE2, 0x97, 0x6C, 0xFF], // copper-light (glow)
    // copper-dark pressed. #A55C32 from §7.17 only reaches 4.32:1 on bg;
    // deepened to meet the 4.5:1 accent contract (contrast.rs).
    accent_dim: [0x9E, 0x55, 0x30, 0xFF],
    success: [0x3F, 0x9E, 0x8A, 0xFF], // mint
    warning: [0xA6, 0x7B, 0x28, 0xFF], // darkened gold, >=3:1 on surface1
    danger: [0xD9, 0x5C, 0x51, 0xFF],  // coral
    line: [0x2A, 0x1D, 0x2F, 0x1F],    // rgba(42,29,47,.12)
    scrim: [0x30, 0x21, 0x35, 0x66],   // plum 40%
    // white 45% glass card / white 50% pill over wallpaper.
    glass: [0xFF, 0xFF, 0xFF, 0x73],
    glass_hi: [0xFF, 0xFF, 0xFF, 0x80],
    // plum ~6% shadow base (0 10 30 -5 plum 6%).
    shadow: [0x30, 0x21, 0x35, 0x0F],
    icon_tile: [0xDA, 0xC9, 0xB3, 0xFF],  // cream-400 sand tile
    icon_glyph: [0x30, 0x21, 0x35, 0xFF], // plum glyph
};

/// The default theme for v1: light Cream Glass (owner preference, Decision 9).
/// `DARK` ships as the dark mode.
pub const DEFAULT: Theme = LIGHT;

/// Spacing scale (px): 4, 8, 12, 16, 24, 32, 48, 64.
pub const SPACING: [i32; 8] = [4, 8, 12, 16, 24, 32, 48, 64];
/// Page margin (px).
pub const PAGE_MARGIN: i32 = 24;
/// Corner radii (px): chips 12, rows 20, cards 28, sheets 36, pills 999.
/// (Cream Glass cards may go up to ~54 px at device scale; `RADIUS_CARD`
/// stays the §7.17 contract value for standard cards.)
pub const RADIUS_CHIP: f32 = 12.0;
pub const RADIUS_ROW: f32 = 20.0;
pub const RADIUS_CARD: f32 = 28.0;
pub const RADIUS_SHEET: f32 = 36.0;
pub const RADIUS_PILL: f32 = 999.0;

/// Status bar height / top safe inset (px).
pub const STATUS_BAR_HEIGHT: i32 = 56;
/// Left exclusion for the punch-hole camera cutout (px).
pub const CUTOUT_INSET: i32 = 72;
/// Bottom gesture bar height (px).
pub const GESTURE_BAR_HEIGHT: i32 = 32;

/// First-party icon tile geometry (px, §7.17 Icons row).
pub const ICON_TILE_SIZE: i32 = 96;
pub const ICON_TILE_RADIUS: f32 = 28.0;
/// Third-party icons: original artwork in a 96 px box, no recolor.
pub const ICON_BOX: i32 = 96;

/// Wallpaper ids (`assets/wallpapers-v2/`): `dusk`, `ember` (default), `sand`.
pub const WALLPAPERS: [&str; 3] = ["dusk", "ember", "sand"];
pub const WALLPAPER_DEFAULT: &str = "ember";

/// 2 % grain overlay applied to wallpapers to stop banding.
pub const WALLPAPER_GRAIN: f32 = 0.02;

/// Elevation: `0 8 24 rgba(8,4,10,.35)` dark / plum ~5 % light; plus a 1 px
/// inner top highlight. No blur shadows in software mode.
pub const ELEVATION_SHADOW: (i32, i32, i32) = (0, 8, 24);
pub const ELEVATION_HIGHLIGHT_DARK: [u8; 4] = [0xFF, 0xFF, 0xFF, 0x14];
pub const ELEVATION_HIGHLIGHT_LIGHT: [u8; 4] = [0xFF, 0xFF, 0xFF, 0xB3];

/// Contrast contract (§7.17 + `de-design/tests/contrast.rs`).
pub const CONTRAST_ACCENT_ON_BG: f64 = 4.5;
pub const CONTRAST_TEXT_ON_SURFACE: f64 = 7.0;

/// WCAG relative-luminance contrast ratio of two sRGB colors (alpha ignored).
#[must_use]
pub fn contrast_ratio(a: [u8; 4], b: [u8; 4]) -> f64 {
    let l1 = luminance(a);
    let l2 = luminance(b);
    let (hi, lo) = if l1 >= l2 { (l1, l2) } else { (l2, l1) };
    (hi + 0.05) / (lo + 0.05)
}

fn luminance(c: [u8; 4]) -> f64 {
    fn ch(v: u8) -> f64 {
        let s = f64::from(v) / 255.0;
        if s <= 0.04045 {
            s / 12.92
        } else {
            ((s + 0.055) / 1.055).powf(2.4)
        }
    }
    0.2126 * ch(c[0]) + 0.7152 * ch(c[1]) + 0.0722 * ch(c[2])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn contrast_sanity() {
        assert!((contrast_ratio([0, 0, 0, 255], [255, 255, 255, 255]) - 21.0).abs() < 0.1);
        assert!((contrast_ratio([255, 255, 255, 255], [255, 255, 255, 255]) - 1.0).abs() < 0.01);
    }
}
