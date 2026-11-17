//! Rabbiz design tokens: plum surfaces, copper accent, cream text.
//!
//! This is the only place a colour or a type size is written down. Screens
//! consume [`Tokens`]; they never contain literals. `contrast_ratio` is the
//! small gate that keeps the palette honest.

use de_api::Color;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Tokens {
    pub bg: Color,
    pub canvas: Color,
    pub surface: Color,
    pub surface_high: Color,
    pub text: Color,
    pub text_dim: Color,
    pub accent: Color,
    pub accent_ink: Color,
    pub mark_body: Color,
    pub mark_cream: Color,
    pub mark_copper: Color,
    pub line: Color,
    pub display_px: f32,
    pub title_px: f32,
    pub body_px: f32,
    pub label_px: f32,
    pub caption_px: f32,
    pub page_margin: f32,
    pub radius_card: f32,
}

pub const DARK: Tokens = Tokens {
    bg: Color::rgb(0x16, 0x10, 0x1A),
    canvas: Color::rgb(0x1D, 0x15, 0x22),
    surface: Color::rgb(0x24, 0x1A, 0x2A),
    surface_high: Color::rgb(0x2E, 0x21, 0x36),
    text: Color::rgb(0xF4, 0xED, 0xE1),
    text_dim: Color::rgb(0xCD, 0xBF, 0xC4),
    accent: Color::rgb(0xD0, 0x8A, 0x5A),
    accent_ink: Color::rgb(0x2A, 0x17, 0x10),
    mark_body: Color::rgb(0x38, 0x26, 0x3D),
    mark_cream: Color::rgb(0xF4, 0xED, 0xE1),
    mark_copper: Color::rgb(0xA5, 0x5C, 0x32),
    line: Color::rgba(0xF4, 0xED, 0xE1, 0x1A),
    display_px: 104.0,
    title_px: 40.0,
    body_px: 26.0,
    label_px: 22.0,
    caption_px: 20.0,
    page_margin: 24.0,
    radius_card: 28.0,
};

/// Relative luminance per WCAG 2.1 (sRGB, no gamma shortcut).
pub fn relative_luminance(c: Color) -> f32 {
    fn channel(v: u8) -> f32 {
        let v = v as f32 / 255.0;
        if v <= 0.04045 {
            v / 12.92
        } else {
            ((v + 0.055) / 1.055).powf(2.4)
        }
    }
    0.2126 * channel(c.r) + 0.7152 * channel(c.g) + 0.0722 * channel(c.b)
}

pub fn contrast_ratio(a: Color, b: Color) -> f32 {
    let (la, lb) = (relative_luminance(a), relative_luminance(b));
    let (hi, lo) = if la > lb { (la, lb) } else { (lb, la) };
    (hi + 0.05) / (lo + 0.05)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn body_text_meets_aa_on_every_surface() {
        for surface in [DARK.bg, DARK.canvas, DARK.surface, DARK.surface_high] {
            let ratio = contrast_ratio(DARK.text, surface);
            assert!(ratio >= 4.5, "text on {surface:?} is only {ratio:.2}:1");
        }
    }

    #[test]
    fn accent_is_readable_on_the_base_surface() {
        assert!(contrast_ratio(DARK.accent, DARK.bg) >= 4.5);
    }

    #[test]
    fn luminance_is_monotonic() {
        assert!(relative_luminance(DARK.text) > relative_luminance(DARK.bg));
    }
}
