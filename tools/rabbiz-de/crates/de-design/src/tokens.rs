//! Design tokens from plan §7.17. All values are physical px at the
//! 720x1600 panel (multiply by 1.5 for the tablet seam later).
//!
//! Colors are sRGB `[r, g, b, a]`, matching `de-api`'s premultiplied contract
//! at the paint boundary (callers premultiply where `a < 255`).

/// Active theme palette. Dark is the M0 default; the light "Parchment" map
/// lands with M2-T1 (owner prefers the "Cream Glass" direction — see
/// `docs/DE-DESIGN-LIGHT-TOKENS.md` and plan Decision 9 note).
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
};

/// The default theme for v1 boot (dark-first per plan Decision 9; the Cream
/// Glass light direction is recorded but not yet applied — see plan note).
pub const DEFAULT: Theme = DARK;

/// Spacing scale (px): 4, 8, 12, 16, 24, 32, 48, 64.
pub const SPACING: [i32; 8] = [4, 8, 12, 16, 24, 32, 48, 64];
/// Page margin (px).
pub const PAGE_MARGIN: i32 = 24;
/// Corner radii (px): chips 12, rows 20, cards 28, sheets 36, pills 999.
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
