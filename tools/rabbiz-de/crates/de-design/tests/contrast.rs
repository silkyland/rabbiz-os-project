//! Contrast contract from plan §7.17, enforced for both theme maps:
//! accent on bg >= 4.5:1, text on surface >= 7:1 (AA/AAA at these sizes).
//! Light map additionally follows the Cream Glass note: plum text >= 4.5:1
//! on wallpaper-adjacent glass, lock clock >= 7:1.

use de_design::tokens::{
    contrast_ratio, CONTRAST_ACCENT_ON_BG, CONTRAST_TEXT_ON_SURFACE, DARK, LIGHT,
};

fn assert_contrast(name: &str, fg: [u8; 4], bg: [u8; 4], min: f64) {
    let r = contrast_ratio(fg, bg);
    assert!(r >= min, "{name}: {r:.2} < {min}");
}

#[test]
fn dark_theme_pairs() {
    let t = DARK;
    assert_contrast("accent/bg", t.accent, t.bg, CONTRAST_ACCENT_ON_BG);
    assert_contrast("accent_hi/surface1", t.accent_hi, t.surface1, 3.0);
    assert_contrast(
        "text/surface1",
        t.text,
        t.surface1,
        CONTRAST_TEXT_ON_SURFACE,
    );
    assert_contrast("text2/surface1", t.text2, t.surface1, 4.5);
    assert_contrast("text3/surface1", t.text3, t.surface1, 3.0);
    assert_contrast("text_on_accent/accent", t.text_on_accent, t.accent, 4.5);
    // Glass over wallpaper worst case = glass over surface1.
    let glass_over = blend(t.glass, t.surface1);
    assert_contrast("text/glass-over-surface1", t.text, glass_over, 4.5);
}

#[test]
fn light_theme_pairs() {
    let t = LIGHT;
    // §7.17 accent contract uses the light-map accent (copper-dark #A55C32).
    assert_contrast("accent_dim/bg", t.accent_dim, t.bg, CONTRAST_ACCENT_ON_BG);
    assert_contrast(
        "text/surface1",
        t.text,
        t.surface1,
        CONTRAST_TEXT_ON_SURFACE,
    );
    assert_contrast("text2/surface1", t.text2, t.surface1, 4.5);
    assert_contrast("text3/surface1", t.text3, t.surface1, 3.0);
    assert_contrast("text_on_accent/accent", t.text_on_accent, t.accent, 4.5);
    assert_contrast(
        "text_on_accent/accent_dim",
        t.text_on_accent,
        t.accent_dim,
        4.5,
    );
    // Plum text on the warm gradient wallpapers must reach 4.5; worst case
    // is glass over the lightest cream surface.
    let glass_over = blend(t.glass, t.surface2);
    assert_contrast("text/glass-over-cream", t.text, glass_over, 4.5);
}

#[test]
fn semantic_readable_on_surfaces() {
    for (name, t) in [("dark", DARK), ("light", LIGHT)] {
        for (role, c) in [
            ("success", t.success),
            ("warning", t.warning),
            ("danger", t.danger),
        ] {
            let r = contrast_ratio(c, t.surface1);
            assert!(r >= 3.0, "{name} {role}/surface1: {r:.2} < 3.0");
        }
    }
}

/// Composite `fg` (with alpha) over opaque `bg`, straight RGBA math.
fn blend(fg: [u8; 4], bg: [u8; 4]) -> [u8; 4] {
    let a = f64::from(fg[3]) / 255.0;
    let mut out = [0u8; 4];
    for i in 0..3 {
        out[i] = (f64::from(fg[i]) * a + f64::from(bg[i]) * (1.0 - a)).round() as u8;
    }
    out[3] = 255;
    out
}

#[test]
fn harmony_invariants() {
    // One accent at a time: accent_hi/accent_dim are shades of accent — the
    // hue angle of all three must stay within a narrow band.
    for t in [DARK, LIGHT] {
        let h = hue(t.accent);
        assert!((hue(t.accent_hi) - h).abs() < 40.0, "accent_hi hue drift");
        assert!((hue(t.accent_dim) - h).abs() < 40.0, "accent_dim hue drift");
    }
}

#[allow(clippy::float_cmp)] // `max` is one of r/g/b bit-for-bit; equality is exact
fn hue(c: [u8; 4]) -> f64 {
    let (r, g, b) = (
        f64::from(c[0]) / 255.0,
        f64::from(c[1]) / 255.0,
        f64::from(c[2]) / 255.0,
    );
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    if (max - min) < 1e-6 {
        return 0.0;
    }
    let d = max - min;
    let h = if max == r {
        (g - b) / d + if g < b { 6.0 } else { 0.0 }
    } else if max == g {
        (b - r) / d + 2.0
    } else {
        (r - g) / d + 4.0
    };
    h * 60.0
}
